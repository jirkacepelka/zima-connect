package store

import (
	"database/sql"
	"fmt"
	"time"
)

// Device represents a registered device in the system.
type Device struct {
	ID        string  `json:"id"`
	Name      string  `json:"name"`
	Platform  string  `json:"platform"`
	PublicKey string  `json:"public_key,omitempty"`
	PairedAt  *string `json:"paired_at,omitempty"`
	LastSeen  *string `json:"last_seen,omitempty"`
	IsOnline  bool    `json:"is_online"`
}

// InsertDevice creates a new device record.
func (s *Store) InsertDevice(d *Device) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, err := s.db.Exec(`INSERT INTO devices (id, name, platform, public_key, paired_at, last_seen, is_online)
		VALUES (?, ?, ?, ?, ?, ?, ?)`,
		d.ID, d.Name, d.Platform, d.PublicKey, d.PairedAt, d.LastSeen, boolToInt(d.IsOnline))
	if err != nil {
		return fmt.Errorf("insert device: %w", err)
	}
	return nil
}

// GetDevice retrieves a single device by ID.
func (s *Store) GetDevice(id string) (*Device, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	d := &Device{}
	var isOnline int
	err := s.db.QueryRow(`SELECT id, name, platform, public_key, paired_at, last_seen, is_online
		FROM devices WHERE id = ?`, id).Scan(
		&d.ID, &d.Name, &d.Platform, &d.PublicKey, &d.PairedAt, &d.LastSeen, &isOnline)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("get device: %w", err)
	}
	d.IsOnline = isOnline != 0
	return d, nil
}

// ListDevices returns all registered devices.
func (s *Store) ListDevices() ([]*Device, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	rows, err := s.db.Query(`SELECT id, name, platform, public_key, paired_at, last_seen, is_online FROM devices ORDER BY name`)
	if err != nil {
		return nil, fmt.Errorf("list devices: %w", err)
	}
	defer rows.Close()

	var devices []*Device
	for rows.Next() {
		d := &Device{}
		var isOnline int
		if err := rows.Scan(&d.ID, &d.Name, &d.Platform, &d.PublicKey, &d.PairedAt, &d.LastSeen, &isOnline); err != nil {
			return nil, fmt.Errorf("scan device: %w", err)
		}
		d.IsOnline = isOnline != 0
		devices = append(devices, d)
	}
	return devices, rows.Err()
}

// ListPairedDevices returns only devices that have been paired.
func (s *Store) ListPairedDevices() ([]*Device, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	rows, err := s.db.Query(`SELECT id, name, platform, public_key, paired_at, last_seen, is_online
		FROM devices WHERE paired_at IS NOT NULL ORDER BY name`)
	if err != nil {
		return nil, fmt.Errorf("list paired devices: %w", err)
	}
	defer rows.Close()

	var devices []*Device
	for rows.Next() {
		d := &Device{}
		var isOnline int
		if err := rows.Scan(&d.ID, &d.Name, &d.Platform, &d.PublicKey, &d.PairedAt, &d.LastSeen, &isOnline); err != nil {
			return nil, fmt.Errorf("scan device: %w", err)
		}
		d.IsOnline = isOnline != 0
		devices = append(devices, d)
	}
	return devices, rows.Err()
}

// UpdateDeviceOnline sets the is_online flag and updates last_seen.
func (s *Store) UpdateDeviceOnline(id string, online bool) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now().UTC().Format(time.RFC3339)
	_, err := s.db.Exec(`UPDATE devices SET is_online = ?, last_seen = ? WHERE id = ?`,
		boolToInt(online), now, id)
	if err != nil {
		return fmt.Errorf("update device online: %w", err)
	}
	return nil
}

// UpdateDevicePaired marks a device as paired.
func (s *Store) UpdateDevicePaired(id string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now().UTC().Format(time.RFC3339)
	_, err := s.db.Exec(`UPDATE devices SET paired_at = ? WHERE id = ?`, now, id)
	if err != nil {
		return fmt.Errorf("update device paired: %w", err)
	}
	return nil
}

// DeleteDevice removes a device by ID.
func (s *Store) DeleteDevice(id string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, err := s.db.Exec(`DELETE FROM devices WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("delete device: %w", err)
	}
	return nil
}

// DeviceCount returns the total number of registered devices.
func (s *Store) DeviceCount() (int, error) {
	var count int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM devices`).Scan(&count)
	return count, err
}

// OnlineDeviceCount returns the number of currently online devices.
func (s *Store) OnlineDeviceCount() (int, error) {
	var count int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM devices WHERE is_online = 1`).Scan(&count)
	return count, err
}

// ResetAllOnline sets all devices to offline (used at startup).
func (s *Store) ResetAllOnline() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, err := s.db.Exec(`UPDATE devices SET is_online = 0`)
	return err
}

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}
