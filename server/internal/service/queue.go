package service

import (
	"encoding/json"

	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/store"
)

// Queue implements store-and-forward: enqueue messages for offline devices
// and flush them when the device reconnects.
type Queue struct {
	store  *store.Store
	sender MessageSender
	log    zerolog.Logger
}

// NewQueue creates a new store-and-forward Queue.
func NewQueue(s *store.Store, log zerolog.Logger) *Queue {
	return &Queue{
		store: s,
		log:   log.With().Str("service", "queue").Logger(),
	}
}

// SetSender sets the WebSocket sender (called after hub is created).
func (q *Queue) SetSender(sender MessageSender) {
	q.sender = sender
}

// FlushPending delivers all pending messages for a device that just came online.
// Returns the number of messages delivered.
func (q *Queue) FlushPending(deviceID string) (int, error) {
	pending, err := q.store.PendingMessages(deviceID)
	if err != nil {
		return 0, err
	}

	if len(pending) == 0 {
		return 0, nil
	}

	delivered := 0
	for _, msg := range pending {
		// Reconstruct the envelope for delivery.
		env := Envelope{
			MessageID:  msg.ID,
			FromDevice: msg.FromDevice,
			ToDevice:   msg.ToDevice,
			Type:       msg.Type,
			Payload:    json.RawMessage(msg.Payload),
			Timestamp:  msg.CreatedAt,
		}

		wireMsg := map[string]interface{}{
			"event":    "message",
			"envelope": env,
		}
		wireData, err := json.Marshal(wireMsg)
		if err != nil {
			q.log.Warn().Err(err).Str("message_id", msg.ID).Msg("failed to marshal pending message")
			continue
		}

		if q.sender != nil && q.sender.SendToDevice(deviceID, wireData) {
			if err := q.store.MarkDelivered(msg.ID); err != nil {
				q.log.Warn().Err(err).Str("message_id", msg.ID).Msg("failed to mark delivered")
			}
			delivered++
		}
	}

	if delivered > 0 {
		q.log.Info().
			Str("device_id", deviceID).
			Int("count", delivered).
			Msg("flushed pending messages")
	}

	return delivered, nil
}

// CleanupExpired removes expired messages from the queue.
func (q *Queue) CleanupExpired() (int64, error) {
	count, err := q.store.CleanupExpiredMessages()
	if err != nil {
		return 0, err
	}
	if count > 0 {
		q.log.Info().Int64("count", count).Msg("cleaned up expired messages")
	}
	return count, nil
}
