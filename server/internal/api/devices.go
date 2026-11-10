package api

import (
	"github.com/gofiber/fiber/v2"

	"github.com/zima-connect/server/internal/service"
	"github.com/zima-connect/server/internal/store"
)

// registerDeviceRoutes adds device management endpoints to the router.
func registerDeviceRoutes(router fiber.Router, registry *service.DeviceRegistry) {
	g := router.Group("/devices")

	// GET /api/v1/devices — List all paired devices.
	g.Get("/", handleListDevices(registry))

	// GET /api/v1/devices/:id/status — Get a specific device's status.
	g.Get("/:id/status", handleDeviceStatus(registry))
}

// handleListDevices returns a handler that lists all paired devices.
func handleListDevices(registry *service.DeviceRegistry) fiber.Handler {
	return func(c *fiber.Ctx) error {
		devices, err := registry.ListPairedDevices()
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
	}
}

// handleDeviceStatus returns a handler that gets a device's current status.
func handleDeviceStatus(registry *service.DeviceRegistry) fiber.Handler {
	return func(c *fiber.Ctx) error {
		id := c.Params("id")
		if id == "" {
			return jsonError(c, fiber.StatusBadRequest, "device id required")
		}

		device, err := registry.GetDevice(id)
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		if device == nil {
			return jsonError(c, fiber.StatusNotFound, "device not found")
		}

		return c.JSON(device)
	}
}
