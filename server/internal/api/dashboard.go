// Package api provides HTTP and WebSocket handlers for the Zima Connect server.
package api

import (
	"github.com/gofiber/fiber/v2"

	"github.com/zima-connect/server/internal/config"
	"github.com/zima-connect/server/internal/service"
	"github.com/zima-connect/server/internal/store"
)

// AdminStats represents summary metrics of the server.
type AdminStats struct {
	TotalDevices    int                  `json:"total_devices"`
	OnlineDevices   int                  `json:"online_devices"`
	TotalMessages   int                  `json:"total_messages"`
	PendingMessages int                  `json:"pending_messages"`
	Storage         *service.StorageInfo `json:"storage"`
	ServerName      string               `json:"server_name"`
	Version         string               `json:"version"`
}

// registerAdminRoutes registers administration and dashboard endpoints.
func registerAdminRoutes(router fiber.Router, svc *Services, cfg *config.Config) {
	g := router.Group("/admin")

	// GET /api/v1/admin/stats — Overview statistics
	g.Get("/stats", func(c *fiber.Ctx) error {
		devCount, err := svc.Store.DeviceCount()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}

		onlineCount, err := svc.Store.OnlineDeviceCount()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}

		msgCount, err := svc.Store.MessageCount()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}

		pendingCount, err := svc.Store.PendingMessageCount()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}

		storageInfo, err := svc.FileStaging.GetStorageInfo()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}

		return c.JSON(AdminStats{
			TotalDevices:    devCount,
			OnlineDevices:   onlineCount,
			TotalMessages:   msgCount,
			PendingMessages: pendingCount,
			Storage:         storageInfo,
			ServerName:      cfg.ServerName,
			Version:         "1.0.0",
		})
	})

	// GET /api/v1/admin/storage — Storage usage details
	g.Get("/storage", func(c *fiber.Ctx) error {
		info, err := svc.FileStaging.GetStorageInfo()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		return c.JSON(info)
	})

	// GET /api/v1/admin/devices — All registered devices with status
	g.Get("/devices", func(c *fiber.Ctx) error {
		devices, err := svc.DeviceRegistry.ListAllDevices()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		if devices == nil {
			devices = []*store.Device{}
		}
		return c.JSON(fiber.Map{
			"devices": devices,
			"count":   len(devices),
		})
	})

	// PUT /api/v1/admin/config — Update server configuration
	g.Put("/config", func(c *fiber.Ctx) error {
		var update struct {
			ServerName  *string `json:"server_name"`
			MaxFileSize *int64  `json:"max_file_size"`
			MaxStorage  *int64  `json:"max_storage"`
			CORSOrigins *string `json:"cors_origins"`
		}
		if err := c.BodyParser(&update); err != nil {
			return jsonError(c, fiber.StatusBadRequest, "invalid request body")
		}

		if update.ServerName != nil {
			cfg.ServerName = *update.ServerName
		}
		if update.MaxFileSize != nil && *update.MaxFileSize > 0 {
			cfg.MaxFileSize = *update.MaxFileSize
		}
		if update.MaxStorage != nil && *update.MaxStorage > 0 {
			cfg.MaxStorage = *update.MaxStorage
		}
		if update.CORSOrigins != nil {
			cfg.CORSOrigins = *update.CORSOrigins
		}

		return c.JSON(fiber.Map{
			"status": "updated",
			"config": cfg,
		})
	})
}
