package store

import (
	"database/sql"
	"fmt"
	"time"
)

// Message represents a store-and-forward message between devices.
type Message struct {
	ID          string  `json:"id"`
	FromDevice  string  `json:"from_device"`
	ToDevice    string  `json:"to_device"`
	Type        string  `json:"type"`
	Payload     string  `json:"payload"`
	CreatedAt   string  `json:"created_at"`
	ExpiresAt   *string `json:"expires_at,omitempty"`
	DeliveredAt *string `json:"delivered_at,omitempty"`
	Acked       bool    `json:"acked"`
}

// InsertMessage stores a new message.
func (s *Store) InsertMessage(m *Message) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, err := s.db.Exec(`INSERT INTO messages (id, from_device, to_device, type, payload, created_at, expires_at)
		VALUES (?, ?, ?, ?, ?, ?, ?)`,
		m.ID, m.FromDevice, m.ToDevice, m.Type, m.Payload, m.CreatedAt, m.ExpiresAt)
	if err != nil {
		return fmt.Errorf("insert message: %w", err)
	}
	return nil
}

// GetMessage retrieves a single message by ID.
func (s *Store) GetMessage(id string) (*Message, error) {
	m := &Message{}
	var acked int
	err := s.db.QueryRow(`SELECT id, from_device, to_device, type, payload, created_at, expires_at, delivered_at, acked
		FROM messages WHERE id = ?`, id).Scan(
		&m.ID, &m.FromDevice, &m.ToDevice, &m.Type, &m.Payload,
		&m.CreatedAt, &m.ExpiresAt, &m.DeliveredAt, &acked)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("get message: %w", err)
	}
	m.Acked = acked != 0
	return m, nil
}

// PendingMessages returns all undelivered, unexpired messages for a device.
func (s *Store) PendingMessages(deviceID string) ([]*Message, error) {
	rows, err := s.db.Query(`SELECT id, from_device, to_device, type, payload, created_at, expires_at, delivered_at, acked
		FROM messages
		WHERE to_device = ?
		  AND delivered_at IS NULL
		  AND acked = 0
		  AND (expires_at IS NULL OR expires_at > datetime('now'))
		ORDER BY created_at ASC`, deviceID)
	if err != nil {
		return nil, fmt.Errorf("pending messages: %w", err)
	}
	defer rows.Close()

	var msgs []*Message
	for rows.Next() {
		m := &Message{}
		var acked int
		if err := rows.Scan(&m.ID, &m.FromDevice, &m.ToDevice, &m.Type, &m.Payload,
			&m.CreatedAt, &m.ExpiresAt, &m.DeliveredAt, &acked); err != nil {
			return nil, fmt.Errorf("scan message: %w", err)
		}
		m.Acked = acked != 0
		msgs = append(msgs, m)
	}
	return msgs, rows.Err()
}

// MarkDelivered sets the delivered_at timestamp for a message.
func (s *Store) MarkDelivered(messageID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now().UTC().Format(time.RFC3339)
	_, err := s.db.Exec(`UPDATE messages SET delivered_at = ? WHERE id = ?`, now, messageID)
	if err != nil {
		return fmt.Errorf("mark delivered: %w", err)
	}
	return nil
}

// AckMessage marks a message as acknowledged.
func (s *Store) AckMessage(messageID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, err := s.db.Exec(`UPDATE messages SET acked = 1 WHERE id = ?`, messageID)
	if err != nil {
		return fmt.Errorf("ack message: %w", err)
	}
	return nil
}

// CleanupExpiredMessages deletes messages past their expiry date.
func (s *Store) CleanupExpiredMessages() (int64, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	result, err := s.db.Exec(`DELETE FROM messages WHERE expires_at IS NOT NULL AND expires_at <= datetime('now')`)
	if err != nil {
		return 0, fmt.Errorf("cleanup expired messages: %w", err)
	}
	return result.RowsAffected()
}

// MessageCount returns the total number of messages.
func (s *Store) MessageCount() (int, error) {
	var count int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM messages`).Scan(&count)
	return count, err
}

// PendingMessageCount returns the number of undelivered messages.
func (s *Store) PendingMessageCount() (int, error) {
	var count int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM messages WHERE delivered_at IS NULL AND acked = 0`).Scan(&count)
	return count, err
}
