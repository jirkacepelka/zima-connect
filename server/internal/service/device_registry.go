package service

import (
	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/store"
)

// DeviceRegistry manages device lifecycle and online/offline tracking.
type DeviceRegistry struct {
	store *store.Store
	log   zerolog.Logger
}

// NewDeviceRegistry creates a new DeviceRegistry.
func NewDeviceRegistry(s *store.Store, log zerolog.Logger) *DeviceRegistry {
	return &DeviceRegistry{
		store: s,
		log:   log.With().Str("service", "device_registry").Logger(),
	}
}

// GetDevice retrieves a device by ID.
func (r *DeviceRegistry) GetDevice(id string) (*store.Device, error) {
	return r.store.GetDevice(id)
}

// ListPairedDevices returns all paired devices.
func (r *DeviceRegistry) ListPairedDevices() ([]*store.Device, error) {
	return r.store.ListPairedDevices()
}

// ListAllDevices returns every registered device.
func (r *DeviceRegistry) ListAllDevices() ([]*store.Device, error) {
	return r.store.ListDevices()
}

// SetOnline marks a device as online and updates last_seen.
func (r *DeviceRegistry) SetOnline(deviceID string) error {
	if err := r.store.UpdateDeviceOnline(deviceID, true); err != nil {
		return err
	}
	r.log.Info().Str("device_id", deviceID).Msg("device online")
	return nil
}

// SetOffline marks a device as offline and updates last_seen.
func (r *DeviceRegistry) SetOffline(deviceID string) error {
	if err := r.store.UpdateDeviceOnline(deviceID, false); err != nil {
		return err
	}
	r.log.Info().Str("device_id", deviceID).Msg("device offline")
	return nil
}

// ResetAll sets all devices to offline (used at startup).
func (r *DeviceRegistry) ResetAll() error {
	return r.store.ResetAllOnline()
}
