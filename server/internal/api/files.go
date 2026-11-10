package api

import (
	"github.com/gofiber/fiber/v2"

	"github.com/zima-connect/server/internal/service"
)

// registerFileRoutes adds file upload/download endpoints to the router.
func registerFileRoutes(router fiber.Router, staging *service.FileStaging) {
	g := router.Group("/files")

	// POST /api/v1/files/upload — Upload a file for delivery to another device.
	g.Post("/upload", handleFileUpload(staging))

	// GET /api/v1/files/:ref/download — Download a staged file.
	g.Get("/:ref/download", handleFileDownload(staging))

	// GET /api/v1/files/:ref/status — Get file metadata.
	g.Get("/:ref/status", handleFileStatus(staging))
}

// handleFileUpload returns a handler for file uploads (multipart form).
func handleFileUpload(staging *service.FileStaging) fiber.Handler {
	return func(c *fiber.Ctx) error {
		fromDevice := c.Locals("device_id").(string)
		toDevice := c.FormValue("to_device")
		if toDevice == "" {
			return jsonError(c, fiber.StatusBadRequest, "to_device is required")
		}

		fileHeader, err := c.FormFile("file")
		if err != nil {
			return jsonError(c, fiber.StatusBadRequest, "file is required")
		}

		file, err := fileHeader.Open()
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, "failed to open uploaded file")
		}
		defer file.Close()

		result, err := staging.StoreFile(fromDevice, toDevice, fileHeader.Filename, file)
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}

		return c.Status(fiber.StatusCreated).JSON(result)
	}
}

// handleFileDownload returns a handler for downloading a staged file.
func handleFileDownload(staging *service.FileStaging) fiber.Handler {
	return func(c *fiber.Ctx) error {
		ref := c.Params("ref")
		if ref == "" {
			return jsonError(c, fiber.StatusBadRequest, "file reference required")
		}

		sf, err := staging.GetFilePath(ref)
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		if sf == nil {
			return jsonError(c, fiber.StatusNotFound, "file not found")
		}

		// Verify the requester is the intended recipient.
		deviceID := c.Locals("device_id").(string)
		if sf.ToDevice != deviceID {
			return jsonError(c, fiber.StatusForbidden, "file is not addressed to this device")
		}

		// Mark as downloaded.
		if err := staging.MarkDownloaded(ref); err != nil {
			// Log but don't fail the download.
			_ = err
		}

		c.Set("Content-Disposition", "attachment; filename=\""+sf.Filename+"\"")
		c.Set("X-Checksum", sf.Checksum)
		return c.SendFile(sf.Path)
	}
}

// handleFileStatus returns a handler that gets file metadata.
func handleFileStatus(staging *service.FileStaging) fiber.Handler {
	return func(c *fiber.Ctx) error {
		ref := c.Params("ref")
		if ref == "" {
			return jsonError(c, fiber.StatusBadRequest, "file reference required")
		}

		sf, err := staging.GetFilePath(ref)
		if err != nil {
			return jsonError(c, fiber.StatusInternalServerError, err.Error())
		}
		if sf == nil {
			return jsonError(c, fiber.StatusNotFound, "file not found")
		}

		return c.JSON(sf)
	}
}
