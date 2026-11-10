package api

import (
	"github.com/gofiber/fiber/v2"

	"github.com/zima-connect/server/internal/service"
)

// registerAuthRoutes adds authentication endpoints to the router.
func registerAuthRoutes(router fiber.Router, auth *service.AuthService) {
	g := router.Group("/auth")

	// POST /api/v1/auth/register — Register a new device.
	g.Post("/register", handleRegister(auth))

	// POST /api/v1/auth/pair — Initiate pairing between two devices.
	g.Post("/pair", handlePair(auth))

	// POST /api/v1/auth/pair/confirm — Confirm pairing with PIN.
	g.Post("/pair/confirm", handlePairConfirm(auth))
}

// handleRegister returns a handler for device registration.
func handleRegister(auth *service.AuthService) fiber.Handler {
	return func(c *fiber.Ctx) error {
		var req service.RegisterRequest
		if err := c.BodyParser(&req); err != nil {
			return jsonError(c, fiber.StatusBadRequest, "invalid request body")
		}

		resp, err := auth.Register(req)
		if err != nil {
			return jsonError(c, fiber.StatusBadRequest, err.Error())
		}

		return c.Status(fiber.StatusCreated).JSON(resp)
	}
}

// handlePair returns a handler for initiating device pairing.
func handlePair(auth *service.AuthService) fiber.Handler {
	return func(c *fiber.Ctx) error {
		var req service.PairRequest
		if err := c.BodyParser(&req); err != nil {
			return jsonError(c, fiber.StatusBadRequest, "invalid request body")
		}

		resp, err := auth.InitiatePairing(req)
		if err != nil {
			switch err {
			case service.ErrDeviceNotFound:
				return jsonError(c, fiber.StatusNotFound, err.Error())
			default:
				return jsonError(c, fiber.StatusInternalServerError, err.Error())
			}
		}

		return c.Status(fiber.StatusCreated).JSON(resp)
	}
}

// handlePairConfirm returns a handler for confirming pairing with PIN.
func handlePairConfirm(auth *service.AuthService) fiber.Handler {
	return func(c *fiber.Ctx) error {
		var req service.ConfirmPairRequest
		if err := c.BodyParser(&req); err != nil {
			return jsonError(c, fiber.StatusBadRequest, "invalid request body")
		}

		err := auth.ConfirmPairing(req)
		if err != nil {
			switch err {
			case service.ErrPairingNotFound:
				return jsonError(c, fiber.StatusNotFound, err.Error())
			case service.ErrPairingExpired:
				return jsonError(c, fiber.StatusGone, err.Error())
			case service.ErrInvalidPIN:
				return jsonError(c, fiber.StatusForbidden, err.Error())
			case service.ErrAlreadyPaired:
				return jsonError(c, fiber.StatusConflict, err.Error())
			default:
				return jsonError(c, fiber.StatusInternalServerError, err.Error())
			}
		}

		return c.JSON(fiber.Map{"status": "paired"})
	}
}
