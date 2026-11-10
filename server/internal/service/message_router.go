package service

import (
	"encoding/json"
	"time"

	"github.com/google/uuid"
	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/store"
)

// Envelope is the standard message wrapper exchanged between devices.
type Envelope struct {
	MessageID  string          `json:"message_id"`
	FromDevice string          `json:"from_device"`
	ToDevice   string          `json:"to_device"`
	Type       string          `json:"type"`
	Payload    json.RawMessage `json:"payload"`
	Timestamp  string          `json:"timestamp"`
}

// MessageSender is an interface for delivering messages to online devices.
// The WebSocket hub will implement this.
type MessageSender interface {
	// SendToDevice delivers a message to a connected device. Returns true if
	// the device was online and the message was sent.
	SendToDevice(deviceID string, data []byte) bool
}

// MessageRouter decides whether to deliver a message directly via WebSocket
// or queue it for later delivery.
type MessageRouter struct {
	store  *store.Store
	sender MessageSender
	log    zerolog.Logger
}

// NewMessageRouter creates a new MessageRouter.
func NewMessageRouter(s *store.Store, log zerolog.Logger) *MessageRouter {
	return &MessageRouter{
		store: s,
		log:   log.With().Str("service", "message_router").Logger(),
	}
}

// SetSender sets the WebSocket sender (called after hub is created to break circular dep).
func (mr *MessageRouter) SetSender(sender MessageSender) {
	mr.sender = sender
}

// RouteResult describes what happened when a message was routed.
type RouteResult struct {
	MessageID string `json:"message_id"`
	Status    string `json:"status"` // "delivered" or "queued"
}

// Route attempts to deliver an envelope to its target device. If the device is
// online and connected, the message is sent directly. Otherwise, it is stored
// in the queue for later delivery.
func (mr *MessageRouter) Route(env Envelope) (*RouteResult, error) {
	if env.MessageID == "" {
		env.MessageID = uuid.New().String()
	}
	if env.Timestamp == "" {
		env.Timestamp = time.Now().UTC().Format(time.RFC3339)
	}

	// Build the wire message.
	wireMsg := map[string]interface{}{
		"event":    "message",
		"envelope": env,
	}
	wireData, err := json.Marshal(wireMsg)
	if err != nil {
		return nil, err
	}

	// Try direct delivery.
	if mr.sender != nil && mr.sender.SendToDevice(env.ToDevice, wireData) {
		mr.log.Debug().
			Str("message_id", env.MessageID).
			Str("to", env.ToDevice).
			Msg("message delivered directly")

		return &RouteResult{MessageID: env.MessageID, Status: "delivered"}, nil
	}

	// Queue for offline delivery.
	payloadBytes, _ := json.Marshal(env.Payload)
	expiresAt := time.Now().UTC().Add(48 * time.Hour).Format(time.RFC3339)
	msg := &store.Message{
		ID:         env.MessageID,
		FromDevice: env.FromDevice,
		ToDevice:   env.ToDevice,
		Type:       env.Type,
		Payload:    string(payloadBytes),
		CreatedAt:  env.Timestamp,
		ExpiresAt:  &expiresAt,
	}

	if err := mr.store.InsertMessage(msg); err != nil {
		return nil, err
	}

	mr.log.Debug().
		Str("message_id", env.MessageID).
		Str("to", env.ToDevice).
		Msg("message queued for offline delivery")

	return &RouteResult{MessageID: env.MessageID, Status: "queued"}, nil
}
