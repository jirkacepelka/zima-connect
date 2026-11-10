package service

import (
	"crypto/sha256"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"

	"github.com/google/uuid"
	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/config"
	"github.com/zima-connect/server/internal/store"
)

// FileStaging handles chunked file uploads, storage management, and cleanup.
type FileStaging struct {
	store *store.Store
	cfg   *config.Config
	log   zerolog.Logger
}

// NewFileStaging creates a new FileStaging service.
func NewFileStaging(s *store.Store, cfg *config.Config, log zerolog.Logger) *FileStaging {
	return &FileStaging{
		store: s,
		cfg:   cfg,
		log:   log.With().Str("service", "file_staging").Logger(),
	}
}

// EnsureStagingDir creates the staging directory if it doesn't exist.
func (fs *FileStaging) EnsureStagingDir() error {
	return os.MkdirAll(fs.cfg.StagingDir(), 0o755)
}

// UploadResult contains information about a completed file upload.
type UploadResult struct {
	RefID     string `json:"ref_id"`
	Filename  string `json:"filename"`
	Size      int64  `json:"size"`
	Checksum  string `json:"checksum"`
	ExpiresAt string `json:"expires_at"`
}

// StoreFile saves an uploaded file to the staging directory and creates a
// database record. The reader should contain the complete file data.
func (fs *FileStaging) StoreFile(fromDevice, toDevice, filename string, reader io.Reader) (*UploadResult, error) {
	// Check storage budget.
	totalSize, err := fs.store.TotalFileSize()
	if err != nil {
		return nil, fmt.Errorf("check storage: %w", err)
	}
	if totalSize >= fs.cfg.MaxStorage {
		return nil, fmt.Errorf("storage budget exceeded (%d / %d bytes)", totalSize, fs.cfg.MaxStorage)
	}

	refID := uuid.New().String()
	ext := filepath.Ext(filename)
	storageName := refID + ext
	storagePath := filepath.Join(fs.cfg.StagingDir(), storageName)

	// Write file to disk while computing checksum.
	outFile, err := os.Create(storagePath)
	if err != nil {
		return nil, fmt.Errorf("create staging file: %w", err)
	}
	defer outFile.Close()

	hasher := sha256.New()
	tee := io.TeeReader(reader, hasher)

	written, err := io.Copy(outFile, io.LimitReader(tee, fs.cfg.MaxFileSize+1))
	if err != nil {
		_ = os.Remove(storagePath)
		return nil, fmt.Errorf("write staging file: %w", err)
	}
	if written > fs.cfg.MaxFileSize {
		_ = os.Remove(storagePath)
		return nil, fmt.Errorf("file exceeds maximum size (%d bytes)", fs.cfg.MaxFileSize)
	}

	checksum := fmt.Sprintf("sha256:%x", hasher.Sum(nil))
	expiresAt := time.Now().UTC().Add(fs.cfg.FileTTL).Format(time.RFC3339)

	record := &store.StagedFile{
		RefID:      refID,
		FromDevice: fromDevice,
		ToDevice:   toDevice,
		Filename:   filename,
		Size:       written,
		Checksum:   checksum,
		Path:       storagePath,
		CreatedAt:  time.Now().UTC().Format(time.RFC3339),
		ExpiresAt:  &expiresAt,
	}

	if err := fs.store.InsertFile(record); err != nil {
		_ = os.Remove(storagePath)
		return nil, fmt.Errorf("record file: %w", err)
	}

	fs.log.Info().
		Str("ref_id", refID).
		Str("filename", filename).
		Int64("size", written).
		Str("to_device", toDevice).
		Msg("file staged")

	return &UploadResult{
		RefID:     refID,
		Filename:  filename,
		Size:      written,
		Checksum:  checksum,
		ExpiresAt: expiresAt,
	}, nil
}

// GetFilePath returns the disk path and metadata for a staged file.
func (fs *FileStaging) GetFilePath(refID string) (*store.StagedFile, error) {
	return fs.store.GetFile(refID)
}

// MarkDownloaded records that a file has been downloaded.
func (fs *FileStaging) MarkDownloaded(refID string) error {
	return fs.store.MarkDownloaded(refID)
}

// CleanupExpired removes expired files from disk and database.
func (fs *FileStaging) CleanupExpired() (int, error) {
	expired, err := fs.store.ExpiredFiles()
	if err != nil {
		return 0, err
	}

	removed := 0
	for _, f := range expired {
		if f.Path != "" {
			if err := os.Remove(f.Path); err != nil && !os.IsNotExist(err) {
				fs.log.Warn().Err(err).Str("ref_id", f.RefID).Msg("failed to remove staged file")
			}
		}
		if err := fs.store.DeleteFile(f.RefID); err != nil {
			fs.log.Warn().Err(err).Str("ref_id", f.RefID).Msg("failed to delete file record")
			continue
		}
		removed++
	}

	if removed > 0 {
		fs.log.Info().Int("count", removed).Msg("cleaned up expired files")
	}
	return removed, nil
}

// StorageInfo returns current storage usage statistics.
type StorageInfo struct {
	UsedBytes  int64 `json:"used_bytes"`
	MaxBytes   int64 `json:"max_bytes"`
	FileCount  int   `json:"file_count"`
	UsedPct    float64 `json:"used_pct"`
}

// GetStorageInfo returns current storage usage.
func (fs *FileStaging) GetStorageInfo() (*StorageInfo, error) {
	totalSize, err := fs.store.TotalFileSize()
	if err != nil {
		return nil, err
	}
	fileCount, err := fs.store.FileCount()
	if err != nil {
		return nil, err
	}

	pct := 0.0
	if fs.cfg.MaxStorage > 0 {
		pct = float64(totalSize) / float64(fs.cfg.MaxStorage) * 100
	}

	return &StorageInfo{
		UsedBytes: totalSize,
		MaxBytes:  fs.cfg.MaxStorage,
		FileCount: fileCount,
		UsedPct:   pct,
	}, nil
}
