// Package service implements the core business logic for Zima Connect,
// including authentication, device management, message routing, and file staging.
package service

import (
	"crypto/rand"
	"errors"
	"fmt"
	"math/big"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/google/uuid"
	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/config"
	"github.com/zima-connect/server/internal/store"
)

// Common auth errors.
var (
	ErrDeviceNotFound     = errors.New("device not found")
	ErrDeviceExists       = errors.New("device already registered")
	ErrInvalidToken       = errors.New("invalid or expired token")
	ErrPairingNotFound    = errors.New("pairing request not found")
	ErrPairingExpired     = errors.New("pairing request expired")
	ErrInvalidPIN         = errors.New("invalid PIN")
	ErrAlreadyPaired      = errors.New("device already paired")
	ErrPairingNotPending  = errors.New("no pending pairing between these devices")
)

// DeviceClaims are the JWT claims issued to an authenticated device.
type DeviceClaims struct {
	DeviceID string `json:"device_id"`
	jwt.RegisteredClaims
}

// AuthService handles device registration, pairing, and JWT management.
type AuthService struct {
	store  *store.Store
	cfg    *config.Config
	log    zerolog.Logger
}

// NewAuthService creates a new AuthService.
func NewAuthService(s *store.Store, cfg *config.Config, log zerolog.Logger) *AuthService {
	return &AuthService{
		store: s,
		cfg:   cfg,
		log:   log.With().Str("service", "auth").Logger(),
	}
}

// RegisterRequest is the payload for device registration.
type RegisterRequest struct {
	Name      string `json:"name"`
	Platform  string `json:"platform"`
	PublicKey string `json:"public_key"`
}

// RegisterResponse is returned after successful registration.
type RegisterResponse struct {
	DeviceID string `json:"device_id"`
	Token    string `json:"token"`
}

// Register creates a new device record and returns a JWT.
func (a *AuthService) Register(req RegisterRequest) (*RegisterResponse, error) {
	if req.Name == "" {
		return nil, fmt.Errorf("device name is required")
	}

	id := uuid.New().String()
	device := &store.Device{
		ID:        id,
		Name:      req.Name,
		Platform:  req.Platform,
		PublicKey: req.PublicKey,
	}

	if err := a.store.InsertDevice(device); err != nil {
		return nil, fmt.Errorf("register device: %w", err)
	}

	token, err := a.generateToken(id)
	if err != nil {
		return nil, fmt.Errorf("generate token: %w", err)
	}

	a.log.Info().Str("device_id", id).Str("name", req.Name).Msg("device registered")

	return &RegisterResponse{
		DeviceID: id,
		Token:    token,
	}, nil
}

// PairRequest initiates a pairing flow between two devices.
type PairRequest struct {
	InitiatorID string `json:"initiator_id"`
	TargetID    string `json:"target_id"`
}

// PairResponse is returned after initiating a pairing request.
type PairResponse struct {
	PairingID string `json:"pairing_id"`
	PIN       string `json:"pin"`
	ExpiresAt string `json:"expires_at"`
}

// InitiatePairing creates a new pairing request with a 6-digit PIN.
func (a *AuthService) InitiatePairing(req PairRequest) (*PairResponse, error) {
	// Verify both devices exist.
	initiator, err := a.store.GetDevice(req.InitiatorID)
	if err != nil {
		return nil, err
	}
	if initiator == nil {
		return nil, ErrDeviceNotFound
	}

	target, err := a.store.GetDevice(req.TargetID)
	if err != nil {
		return nil, err
	}
	if target == nil {
		return nil, ErrDeviceNotFound
	}

	// Generate a 6-digit PIN.
	pin, err := generatePIN(6)
	if err != nil {
		return nil, fmt.Errorf("generate pin: %w", err)
	}

	pairingID := uuid.New().String()
	expiresAt := time.Now().UTC().Add(5 * time.Minute).Format(time.RFC3339)

	_, execErr := a.store.DB().Exec(
		`INSERT INTO pairing_requests (id, initiator_id, target_id, pin, expires_at) VALUES (?, ?, ?, ?, ?)`,
		pairingID, req.InitiatorID, req.TargetID, pin, expiresAt)
	if execErr != nil {
		return nil, fmt.Errorf("create pairing request: %w", execErr)
	}

	a.log.Info().
		Str("pairing_id", pairingID).
		Str("initiator", req.InitiatorID).
		Str("target", req.TargetID).
		Msg("pairing initiated")

	return &PairResponse{
		PairingID: pairingID,
		PIN:       pin,
		ExpiresAt: expiresAt,
	}, nil
}

// ConfirmPairRequest is the payload for confirming a pairing.
type ConfirmPairRequest struct {
	PairingID string `json:"pairing_id"`
	PIN       string `json:"pin"`
}

// ConfirmPairing verifies the PIN and marks both devices as paired.
func (a *AuthService) ConfirmPairing(req ConfirmPairRequest) error {
	var initiatorID, targetID, storedPIN, expiresAt string
	var confirmed int

	err := a.store.DB().QueryRow(
		`SELECT initiator_id, target_id, pin, expires_at, confirmed FROM pairing_requests WHERE id = ?`,
		req.PairingID).Scan(&initiatorID, &targetID, &storedPIN, &expiresAt, &confirmed)
	if err != nil {
		return ErrPairingNotFound
	}

	if confirmed != 0 {
		return ErrAlreadyPaired
	}

	// Check expiry.
	expiry, err := time.Parse(time.RFC3339, expiresAt)
	if err != nil || time.Now().UTC().After(expiry) {
		return ErrPairingExpired
	}

	if req.PIN != storedPIN {
		return ErrInvalidPIN
	}

	// Mark pairing confirmed.
	if _, err := a.store.DB().Exec(`UPDATE pairing_requests SET confirmed = 1 WHERE id = ?`, req.PairingID); err != nil {
		return fmt.Errorf("confirm pairing: %w", err)
	}

	// Mark both devices as paired.
	if err := a.store.UpdateDevicePaired(initiatorID); err != nil {
		return err
	}
	if err := a.store.UpdateDevicePaired(targetID); err != nil {
		return err
	}

	a.log.Info().
		Str("pairing_id", req.PairingID).
		Str("initiator", initiatorID).
		Str("target", targetID).
		Msg("pairing confirmed")

	return nil
}

// ValidateToken parses and validates a JWT, returning the device claims.
func (a *AuthService) ValidateToken(tokenStr string) (*DeviceClaims, error) {
	token, err := jwt.ParseWithClaims(tokenStr, &DeviceClaims{}, func(t *jwt.Token) (interface{}, error) {
		if _, ok := t.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", t.Header["alg"])
		}
		return []byte(a.cfg.JWTSecret), nil
	})
	if err != nil {
		return nil, ErrInvalidToken
	}

	claims, ok := token.Claims.(*DeviceClaims)
	if !ok || !token.Valid {
		return nil, ErrInvalidToken
	}

	return claims, nil
}

// generateToken creates a signed JWT for a device.
func (a *AuthService) generateToken(deviceID string) (string, error) {
	claims := DeviceClaims{
		DeviceID: deviceID,
		RegisteredClaims: jwt.RegisteredClaims{
			Issuer:    "zima-connect",
			Subject:   deviceID,
			IssuedAt:  jwt.NewNumericDate(time.Now()),
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(365 * 24 * time.Hour)), // 1 year
		},
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString([]byte(a.cfg.JWTSecret))
}

// generatePIN returns a cryptographically random numeric PIN of the given length.
func generatePIN(length int) (string, error) {
	pin := ""
	for i := 0; i < length; i++ {
		n, err := rand.Int(rand.Reader, big.NewInt(10))
		if err != nil {
			return "", err
		}
		pin += fmt.Sprintf("%d", n.Int64())
	}
	return pin, nil
}
