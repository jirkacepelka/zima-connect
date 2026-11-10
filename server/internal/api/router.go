// Package api provides HTTP and WebSocket handlers for the Zima Connect server.
package api

import (
	"fmt"
	"strings"
	"time"

	"github.com/gofiber/fiber/v2"
	"github.com/gofiber/fiber/v2/middleware/cors"
	"github.com/gofiber/fiber/v2/middleware/limiter"
	"github.com/gofiber/fiber/v2/middleware/recover"
	"github.com/gofiber/fiber/v2/middleware/requestid"
	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/config"
	"github.com/zima-connect/server/internal/service"
	"github.com/zima-connect/server/internal/store"
)

// ErrorResponse is the standard JSON error envelope.
type ErrorResponse struct {
	Error   string `json:"error"`
	Code    int    `json:"code"`
	Details string `json:"details,omitempty"`
}

// jsonError writes a consistent error response.
func jsonError(c *fiber.Ctx, status int, msg string) error {
	return c.Status(status).JSON(ErrorResponse{
		Error: msg,
		Code:  status,
	})
}

// Services groups all service-layer dependencies needed by handlers.
type Services struct {
	Store          *store.Store
	Auth           *service.AuthService
	DeviceRegistry *service.DeviceRegistry
	MessageRouter  *service.MessageRouter
	FileStaging    *service.FileStaging
	Queue          *service.Queue
}

// NewApp creates and configures the Fiber application with all routes and middleware.
func NewApp(cfg *config.Config, svc *Services, log zerolog.Logger) *fiber.App {
	app := fiber.New(fiber.Config{
		AppName:               cfg.ServerName,
		ReadTimeout:           30 * time.Second,
		WriteTimeout:          30 * time.Second,
		IdleTimeout:           120 * time.Second,
		BodyLimit:             int(cfg.MaxFileSize) + 1024*1024, // file size + overhead
		DisableStartupMessage: true,
	})

	// --- Global middleware ---

	app.Use(recover.New(recover.Config{
		EnableStackTrace: true,
	}))

	app.Use(requestid.New())

	// Structured logging middleware.
	app.Use(func(c *fiber.Ctx) error {
		start := time.Now()
		err := c.Next()
		log.Info().
			Str("method", c.Method()).
			Str("path", c.Path()).
			Int("status", c.Response().StatusCode()).
			Dur("latency", time.Since(start)).
			Str("request_id", c.GetRespHeader("X-Request-Id")).
			Msg("request")
		return err
	})

	app.Use(cors.New(cors.Config{
		AllowOrigins: cfg.CORSOrigins,
		AllowMethods: "GET,POST,PUT,DELETE,OPTIONS",
		AllowHeaders: "Origin,Content-Type,Accept,Authorization",
	}))

	app.Use(limiter.New(limiter.Config{
		Max:        cfg.RateLimitMax,
		Expiration: cfg.RateLimitWindow,
		KeyGenerator: func(c *fiber.Ctx) string {
			return c.IP()
		},
		LimitReached: func(c *fiber.Ctx) error {
			return jsonError(c, fiber.StatusTooManyRequests, "rate limit exceeded")
		},
	}))

	// --- Health check ---
	app.Get("/health", func(c *fiber.Ctx) error {
		return c.JSON(fiber.Map{
			"status":  "ok",
			"server":  cfg.ServerName,
			"version": "1.0.0",
		})
	})

	// --- API v1 routes ---
	v1 := app.Group("/api/v1")

	// Auth routes (public).
	registerAuthRoutes(v1, svc.Auth)

	// WebSocket route (authenticated via query token).
	registerWebSocketRoute(v1, svc, cfg, log)

	// Protected routes.
	protected := v1.Group("", jwtMiddleware(svc.Auth))
	registerDeviceRoutes(protected, svc.DeviceRegistry)
	registerFileRoutes(protected, svc.FileStaging)
	registerMessageRoutes(protected, svc)
	registerAdminRoutes(protected, svc, cfg)

	return app
}

// NewDashboardApp creates a separate Fiber app for the admin dashboard.
func NewDashboardApp(cfg *config.Config, svc *Services, log zerolog.Logger) *fiber.App {
	app := fiber.New(fiber.Config{
		AppName:               cfg.ServerName + " Dashboard",
		DisableStartupMessage: true,
	})

	app.Use(recover.New())

	app.Get("/health", func(c *fiber.Ctx) error {
		return c.JSON(fiber.Map{"status": "ok"})
	})

	// Dashboard routes don't require device JWT — they are on a separate port
	// and would typically be protected by network-level access control or a
	// separate admin auth mechanism.
	registerAdminRoutes(app.Group("/api/v1"), svc, cfg)

	return app
}

// jwtMiddleware creates Fiber middleware that validates the Authorization header.
func jwtMiddleware(auth *service.AuthService) fiber.Handler {
	return func(c *fiber.Ctx) error {
		header := c.Get("Authorization")
		if header == "" {
			return jsonError(c, fiber.StatusUnauthorized, "missing authorization header")
		}

		parts := strings.SplitN(header, " ", 2)
		if len(parts) != 2 || !strings.EqualFold(parts[0], "bearer") {
			return jsonError(c, fiber.StatusUnauthorized, "invalid authorization format")
		}

		claims, err := auth.ValidateToken(parts[1])
		if err != nil {
			return jsonError(c, fiber.StatusUnauthorized, fmt.Sprintf("invalid token: %v", err))
		}

		c.Locals("device_id", claims.DeviceID)
		c.Locals("claims", claims)
		return c.Next()
	}
}
