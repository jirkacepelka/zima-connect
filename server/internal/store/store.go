// Package store provides SQLite database access, schema migrations, and
// typed repository methods for the Zima Connect server.
package store

import (
	"database/sql"
	"embed"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"

	"github.com/rs/zerolog"
	_ "modernc.org/sqlite"
)

//go:embed migrations/*.sql
var migrationsFS embed.FS

// Store wraps a SQLite database connection and provides repository methods.
type Store struct {
	db  *sql.DB
	mu  sync.RWMutex // protects write-heavy paths
	log zerolog.Logger
}

// New opens (or creates) the SQLite database at dbPath, applies any pending
// migrations, and returns a ready-to-use Store.
func New(dbPath string, log zerolog.Logger) (*Store, error) {
	// Ensure parent directory exists.
	if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
		return nil, fmt.Errorf("create data dir: %w", err)
	}

	dsn := fmt.Sprintf("file:%s?_journal_mode=WAL&_busy_timeout=5000&_foreign_keys=on", dbPath)
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("open sqlite: %w", err)
	}

	// Connection pool tuning for SQLite.
	db.SetMaxOpenConns(1) // SQLite supports one writer at a time.
	db.SetMaxIdleConns(2)

	if err := db.Ping(); err != nil {
		return nil, fmt.Errorf("ping sqlite: %w", err)
	}

	s := &Store{db: db, log: log.With().Str("component", "store").Logger()}

	if err := s.migrate(); err != nil {
		return nil, fmt.Errorf("migrate: %w", err)
	}

	s.log.Info().Str("path", dbPath).Msg("database ready")
	return s, nil
}

// Close gracefully closes the underlying database connection.
func (s *Store) Close() error {
	return s.db.Close()
}

// DB returns the raw *sql.DB for advanced queries.
func (s *Store) DB() *sql.DB {
	return s.db
}

// migrate applies embedded SQL migration files that have not yet been applied.
func (s *Store) migrate() error {
	// Ensure the schema_migrations table exists before querying it.
	if _, err := s.db.Exec(`CREATE TABLE IF NOT EXISTS schema_migrations (
		version    INTEGER PRIMARY KEY,
		applied_at TEXT NOT NULL DEFAULT (datetime('now'))
	)`); err != nil {
		return fmt.Errorf("create schema_migrations: %w", err)
	}

	entries, err := migrationsFS.ReadDir("migrations")
	if err != nil {
		return fmt.Errorf("read migrations dir: %w", err)
	}

	// Sort by filename to guarantee ordering.
	sort.Slice(entries, func(i, j int) bool {
		return entries[i].Name() < entries[j].Name()
	})

	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".sql") {
			continue
		}

		version := migrationVersion(entry.Name())
		if version == 0 {
			continue
		}

		// Check if already applied.
		var count int
		if err := s.db.QueryRow("SELECT COUNT(*) FROM schema_migrations WHERE version = ?", version).Scan(&count); err != nil {
			return fmt.Errorf("check migration %d: %w", version, err)
		}
		if count > 0 {
			continue
		}

		data, err := migrationsFS.ReadFile("migrations/" + entry.Name())
		if err != nil {
			return fmt.Errorf("read migration %s: %w", entry.Name(), err)
		}

		tx, err := s.db.Begin()
		if err != nil {
			return fmt.Errorf("begin tx for migration %d: %w", version, err)
		}

		if _, err := tx.Exec(string(data)); err != nil {
			_ = tx.Rollback()
			return fmt.Errorf("exec migration %d: %w", version, err)
		}

		if _, err := tx.Exec("INSERT INTO schema_migrations (version) VALUES (?)", version); err != nil {
			_ = tx.Rollback()
			return fmt.Errorf("record migration %d: %w", version, err)
		}

		if err := tx.Commit(); err != nil {
			return fmt.Errorf("commit migration %d: %w", version, err)
		}

		s.log.Info().Int("version", version).Str("file", entry.Name()).Msg("applied migration")
	}

	return nil
}

// migrationVersion extracts the leading integer from a migration filename
// like "001_init.sql" → 1.
func migrationVersion(name string) int {
	parts := strings.SplitN(name, "_", 2)
	if len(parts) == 0 {
		return 0
	}
	v := 0
	for _, c := range parts[0] {
		if c < '0' || c > '9' {
			break
		}
		v = v*10 + int(c-'0')
	}
	return v
}
