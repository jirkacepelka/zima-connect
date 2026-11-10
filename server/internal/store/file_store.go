package store

import (
	"database/sql"
	"fmt"
	"time"
)

// StagedFile represents a file staged for delivery to an offline device.
type StagedFile struct {
	RefID        string  `json:"ref_id"`
	FromDevice   string  `json:"from_device"`
	ToDevice     string  `json:"to_device"`
	Filename     string  `json:"filename"`
	Size         int64   `json:"size"`
	Checksum     string  `json:"checksum"`
	Path         string  `json:"-"` // internal filesystem path
	CreatedAt    string  `json:"created_at"`
	ExpiresAt    *string `json:"expires_at,omitempty"`
	DownloadedAt *string `json:"downloaded_at,omitempty"`
}

// InsertFile creates a new staged file record.
func (s *Store) InsertFile(f *StagedFile) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, err := s.db.Exec(`INSERT INTO files (ref_id, from_device, to_device, filename, size, checksum, path, created_at, expires_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		f.RefID, f.FromDevice, f.ToDevice, f.Filename, f.Size, f.Checksum, f.Path, f.CreatedAt, f.ExpiresAt)
	if err != nil {
		return fmt.Errorf("insert file: %w", err)
	}
	return nil
}

// GetFile retrieves a staged file record by reference ID.
func (s *Store) GetFile(refID string) (*StagedFile, error) {
	f := &StagedFile{}
	err := s.db.QueryRow(`SELECT ref_id, from_device, to_device, filename, size, checksum, path, created_at, expires_at, downloaded_at
		FROM files WHERE ref_id = ?`, refID).Scan(
		&f.RefID, &f.FromDevice, &f.ToDevice, &f.Filename, &f.Size,
		&f.Checksum, &f.Path, &f.CreatedAt, &f.ExpiresAt, &f.DownloadedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("get file: %w", err)
	}
	return f, nil
}

// MarkDownloaded sets the downloaded_at timestamp for a file.
func (s *Store) MarkDownloaded(refID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now().UTC().Format(time.RFC3339)
	_, err := s.db.Exec(`UPDATE files SET downloaded_at = ? WHERE ref_id = ?`, now, refID)
	if err != nil {
		return fmt.Errorf("mark downloaded: %w", err)
	}
	return nil
}

// ExpiredFiles returns all files whose expires_at has passed.
func (s *Store) ExpiredFiles() ([]*StagedFile, error) {
	rows, err := s.db.Query(`SELECT ref_id, from_device, to_device, filename, size, checksum, path, created_at, expires_at, downloaded_at
		FROM files WHERE expires_at IS NOT NULL AND expires_at <= datetime('now')`)
	if err != nil {
		return nil, fmt.Errorf("expired files: %w", err)
	}
	defer rows.Close()

	var files []*StagedFile
	for rows.Next() {
		f := &StagedFile{}
		if err := rows.Scan(&f.RefID, &f.FromDevice, &f.ToDevice, &f.Filename, &f.Size,
			&f.Checksum, &f.Path, &f.CreatedAt, &f.ExpiresAt, &f.DownloadedAt); err != nil {
			return nil, fmt.Errorf("scan file: %w", err)
		}
		files = append(files, f)
	}
	return files, rows.Err()
}

// DeleteFile removes a file record by reference ID.
func (s *Store) DeleteFile(refID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, err := s.db.Exec(`DELETE FROM files WHERE ref_id = ?`, refID)
	if err != nil {
		return fmt.Errorf("delete file: %w", err)
	}
	return nil
}

// FileCount returns the total number of staged files.
func (s *Store) FileCount() (int, error) {
	var count int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM files`).Scan(&count)
	return count, err
}

// TotalFileSize returns the sum of all staged file sizes in bytes.
func (s *Store) TotalFileSize() (int64, error) {
	var total sql.NullInt64
	err := s.db.QueryRow(`SELECT SUM(size) FROM files`).Scan(&total)
	if !total.Valid {
		return 0, err
	}
	return total.Int64, err
}

// PendingFilesForDevice returns files not yet downloaded by the target device.
func (s *Store) PendingFilesForDevice(deviceID string) ([]*StagedFile, error) {
	rows, err := s.db.Query(`SELECT ref_id, from_device, to_device, filename, size, checksum, path, created_at, expires_at, downloaded_at
		FROM files
		WHERE to_device = ?
		  AND downloaded_at IS NULL
		  AND (expires_at IS NULL OR expires_at > datetime('now'))
		ORDER BY created_at ASC`, deviceID)
	if err != nil {
		return nil, fmt.Errorf("pending files for device: %w", err)
	}
	defer rows.Close()

	var files []*StagedFile
	for rows.Next() {
		f := &StagedFile{}
		if err := rows.Scan(&f.RefID, &f.FromDevice, &f.ToDevice, &f.Filename, &f.Size,
			&f.Checksum, &f.Path, &f.CreatedAt, &f.ExpiresAt, &f.DownloadedAt); err != nil {
			return nil, fmt.Errorf("scan file: %w", err)
		}
		files = append(files, f)
	}
	return files, rows.Err()
}
