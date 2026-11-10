// Package api provides HTTP and WebSocket handlers for the Zima Connect server.
package api

import (
	"encoding/json"
	"sync"
	"time"

	"github.com/gofiber/contrib/websocket"
	"github.com/gofiber/fiber/v2"
	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/config"
	"github.com/zima-connect/server/internal/service"
	"github.com/zima-connect/server/internal/store"
)

// WsClient represents a single connected WebSocket client.
type WsClient struct {
	DeviceID string
	Conn     *websocket.Conn
	Send     chan []byte
	Hub      *Hub
}

// Hub manages active WebSocket connections and routes messages between devices.
type Hub struct {
	mu         sync.RWMutex
	clients    map[string]map[*WsClient]bool // deviceID -> set of clients
	broadcast  chan []byte
	register   chan *WsClient
	unregister chan *WsClient
	services   *Services
	log        zerolog.Logger
}

// NewHub creates and starts a new WebSocket Hub.
func NewHub(services *Services, log zerolog.Logger) *Hub {
	h := &Hub{
		clients:    make(map[string]map[*WsClient]bool),
		broadcast:  make(chan []byte, 256),
		register:   make(chan *WsClient),
		unregister: make(chan *WsClient),
		services:   services,
		log:        log.With().Str("component", "ws_hub").Logger(),
	}

	// Register this hub as the message sender in router and queue
	services.MessageRouter.SetSender(h)
	services.Queue.SetSender(h)

	go h.run()
	return h
}

// run is the main event loop for the Hub.
func (h *Hub) run() {
	for {
		select {
		case client := <-h.register:
			h.mu.Lock()
			if _, ok := h.clients[client.DeviceID]; !ok {
				h.clients[client.DeviceID] = make(map[*WsClient]bool)
			}
			h.clients[client.DeviceID][client] = true
			isFirstConn := len(h.clients[client.DeviceID]) == 1
			h.mu.Unlock()

			if isFirstConn {
				_ = h.services.DeviceRegistry.SetOnline(client.DeviceID)
				h.notifyDeviceStatus(client.DeviceID, true)
				// Flush any messages held in offline queue
				go func(devID string) {
					if _, err := h.services.Queue.FlushPending(devID); err != nil {
						h.log.Error().Err(err).Str("device_id", devID).Msg("failed to flush pending messages")
					}
				}(client.DeviceID)
			}

			h.log.Info().Str("device_id", client.DeviceID).Msg("client connected to websocket")

		case client := <-h.unregister:
			h.mu.Lock()
			if conns, ok := h.clients[client.DeviceID]; ok {
				if _, ok := conns[client]; ok {
					delete(conns, client)
					close(client.Send)
					if len(conns) == 0 {
						delete(h.clients, client.DeviceID)
						h.mu.Unlock()
						_ = h.services.DeviceRegistry.SetOffline(client.DeviceID)
						h.notifyDeviceStatus(client.DeviceID, false)
						h.log.Info().Str("device_id", client.DeviceID).Msg("all connections closed, device offline")
						continue
					}
				}
			}
			h.mu.Unlock()

		case message := <-h.broadcast:
			h.mu.RLock()
			for _, conns := range h.clients {
				for client := range conns {
					select {
					case client.Send <- message:
					default:
					}
				}
			}
			h.mu.RUnlock()
		}
	}
}

// SendToDevice delivers data to a connected device. Implements service.MessageSender.
func (h *Hub) SendToDevice(deviceID string, data []byte) bool {
	h.mu.RLock()
	conns, ok := h.clients[deviceID]
	if !ok || len(conns) == 0 {
		h.mu.RUnlock()
		return false
	}

	sent := false
	for client := range conns {
		select {
		case client.Send <- data:
			sent = true
		default:
			h.log.Warn().Str("device_id", deviceID).Msg("client send channel full, skipping")
		}
	}
	h.mu.RUnlock()
	return sent
}

// notifyDeviceStatus broadcasts presence change to all connected clients.
func (h *Hub) notifyDeviceStatus(deviceID string, online bool) {
	evt := "device_offline"
	if online {
		evt = "device_online"
	}
	payload, _ := json.Marshal(map[string]interface{}{
		"event":     evt,
		"device_id": deviceID,
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	})
	h.broadcast <- payload
}

// WsMessage represents an incoming message from a WebSocket client.
type WsMessage struct {
	Action    string            `json:"action"` // "send", "ack", "ping"
	Envelope  *service.Envelope `json:"envelope,omitempty"`
	MessageID string            `json:"message_id,omitempty"`
}

// registerWebSocketRoute registers the WebSocket endpoint.
func registerWebSocketRoute(router fiber.Router, svc *Services, cfg *config.Config, log zerolog.Logger) {
	hub := NewHub(svc, log)

	router.Use("/ws", func(c *fiber.Ctx) error {
		if websocket.IsWebSocketUpgrade(c) {
			token := c.Query("token")
			if token == "" {
				authHeader := c.Get("Authorization")
				if len(authHeader) > 7 && authHeader[:7] == "Bearer " {
					token = authHeader[7:]
				}
			}

			if token == "" {
				return c.Status(fiber.StatusUnauthorized).JSON(fiber.Map{
					"error": "token query parameter or Authorization header required",
				})
			}

			claims, err := svc.Auth.ValidateToken(token)
			if err != nil {
				return c.Status(fiber.StatusUnauthorized).JSON(fiber.Map{
					"error": "invalid authentication token",
				})
			}

			c.Locals("device_id", claims.DeviceID)
			return c.Next()
		}
		return fiber.ErrUpgradeRequired
	})

	router.Get("/ws", websocket.New(func(c *websocket.Conn) {
		deviceID, _ := c.Locals("device_id").(string)
		if deviceID == "" {
			_ = c.Close()
			return
		}

		client := &WsClient{
			DeviceID: deviceID,
			Conn:     c,
			Send:     make(chan []byte, 64),
			Hub:      hub,
		}

		hub.register <- client

		// Write pump
		go func() {
			for msg := range client.Send {
				if err := c.WriteMessage(websocket.TextMessage, msg); err != nil {
					break
				}
			}
			_ = c.Close()
		}()

		// Read pump
		defer func() {
			hub.unregister <- client
			_ = c.Close()
		}()

		c.SetReadLimit(int64(cfg.MaxFileSize))
		_ = c.SetReadDeadline(time.Now().Add(60 * time.Second))
		c.SetPongHandler(func(string) error {
			_ = c.SetReadDeadline(time.Now().Add(60 * time.Second))
			return nil
		})

		for {
			_, msgBytes, err := c.ReadMessage()
			if err != nil {
				break
			}
			_ = c.SetReadDeadline(time.Now().Add(60 * time.Second))

			var msg WsMessage
			if err := json.Unmarshal(msgBytes, &msg); err != nil {
				log.Warn().Err(err).Msg("invalid ws message format")
				continue
			}

			switch msg.Action {
			case "send":
				if msg.Envelope == nil {
					continue
				}
				msg.Envelope.FromDevice = deviceID
				result, err := svc.MessageRouter.Route(*msg.Envelope)
				if err != nil {
					log.Error().Err(err).Msg("failed to route message")
					continue
				}

				// Send confirmation back to sender
				resp, _ := json.Marshal(map[string]interface{}{
					"event":      result.Status,
					"message_id": result.MessageID,
				})
				select {
				case client.Send <- resp:
				default:
				}

			case "ack":
				if msg.MessageID != "" && svc.Store != nil {
					_ = svc.Store.AckMessage(msg.MessageID)
				}

			case "ping":
				resp, _ := json.Marshal(map[string]string{"event": "pong"})
				select {
				case client.Send <- resp:
				default:
				}
			}
		}
	}))
}

// registerMessageRoutes adds fallback HTTP endpoints for message passing.
func registerMessageRoutes(router fiber.Router, svc *Services) {
	g := router.Group("/messages")

	// GET /api/v1/messages/pending — Get all pending messages for current device
	g.Get("/pending", func(c *fiber.Ctx) error {
		deviceID := c.Locals("device_id").(string)
		pending, err := svc.Store.PendingMessages(deviceID)
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		if pending == nil {
			pending = []*store.Message{}
		}
		return c.JSON(fiber.Map{
			"messages": pending,
			"count":    len(pending),
		})
	})

	// POST /api/v1/messages/send — Send an envelope via HTTP (fallback)
	g.Post("/send", func(c *fiber.Ctx) error {
		deviceID := c.Locals("device_id").(string)
		var env service.Envelope
		if err := c.BodyParser(&env); err != nil {
			return jsonError(c, fiber.StatusBadRequest, "invalid envelope format")
		}
		env.FromDevice = deviceID

		result, err := svc.MessageRouter.Route(env)
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		return c.Status(fiber.StatusAccepted).JSON(result)
	})

	// POST /api/v1/messages/:id/ack — Acknowledge message receipt
	g.Post("/:id/ack", func(c *fiber.Ctx) error {
		id := c.Params("id")
		if id == "" {
			return jsonError(c, fiber.StatusBadRequest, "message id required")
		}
		if err := svc.Store.AckMessage(id); err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		return c.JSON(fiber.Map{"status": "ok"})
	})
}
