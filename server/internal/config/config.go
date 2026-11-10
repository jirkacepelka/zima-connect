// Package config provides application configuration loaded from environment
// variables with sensible defaults for ZimaOS deployment.
package config

import (
	"os"
	"strconv"
	"time"
)

// Config holds all server configuration values.
type Config struct {
	// ServerName is a human-friendly name for this Zima Connect instance.
	ServerName string
	// Port is the main API/WebSocket listen port.
	Port int
	// DashboardPort is the admin dashboard listen port.
	DashboardPort int
	// MaxFileSize is the maximum allowed upload size in bytes (default 2 GB).
	MaxFileSize int64
	// MaxStorage is the total staged file storage budget in bytes (default 10 GB).
	MaxStorage int64
	// FileTTL is how long staged files are kept before automatic cleanup.
	FileTTL time.Duration
	// JWTSecret is the HMAC-SHA256 signing key for device tokens.
	JWTSecret string
	// DataDir is the root directory for SQLite database and staged files.
	DataDir string
	// ExternalURL is the publicly reachable base URL of this server.
	ExternalURL string
	// LogLevel controls zerolog verbosity (trace, debug, info, warn, error).
	LogLevel string
	// CORSOrigins is a comma-separated list of allowed CORS origins.
	CORSOrigins string
	// RateLimitMax is the maximum number of requests per RateLimitWindow.
	RateLimitMax int
	// RateLimitWindow is the sliding window for rate limiting.
	RateLimitWindow time.Duration
}

// Load reads configuration from environment variables, applying defaults where
// values are not set.
func Load() *Config {
	return &Config{
		ServerName:      envStr("ZC_SERVER_NAME", "Zima Connect"),
		Port:            envInt("ZC_PORT", 9120),
		DashboardPort:   envInt("ZC_DASHBOARD_PORT", 9121),
		MaxFileSize:     envInt64("ZC_MAX_FILE_SIZE", 2*1024*1024*1024),    // 2 GB
		MaxStorage:      envInt64("ZC_MAX_STORAGE", 10*1024*1024*1024),     // 10 GB
		FileTTL:         envDuration("ZC_FILE_TTL", 48*time.Hour),
		JWTSecret:       envStr("ZC_JWT_SECRET", "change-me-in-production"),
		DataDir:         envStr("ZC_DATA_DIR", "./data"),
		ExternalURL:     envStr("ZC_EXTERNAL_URL", "http://localhost:9120"),
		LogLevel:        envStr("ZC_LOG_LEVEL", "info"),
		CORSOrigins:     envStr("ZC_CORS_ORIGINS", "*"),
		RateLimitMax:    envInt("ZC_RATE_LIMIT_MAX", 60),
		RateLimitWindow: envDuration("ZC_RATE_LIMIT_WINDOW", time.Minute),
	}
}

// StagingDir returns the path to the file staging directory.
func (c *Config) StagingDir() string {
	return c.DataDir + "/staging"
}

// DatabasePath returns the path to the SQLite database file.
func (c *Config) DatabasePath() string {
	return c.DataDir + "/zima-connect.db"
}

// --- helpers ----------------------------------------------------------------

func envStr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

func envInt(key string, fallback int) int {
	if v := os.Getenv(key); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			return n
		}
	}
	return fallback
}

func envInt64(key string, fallback int64) int64 {
	if v := os.Getenv(key); v != "" {
		if n, err := strconv.ParseInt(v, 10, 64); err == nil {
			return n
		}
	}
	return fallback
}

func envDuration(key string, fallback time.Duration) time.Duration {
	if v := os.Getenv(key); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			return d
		}
	}
	return fallback
}
