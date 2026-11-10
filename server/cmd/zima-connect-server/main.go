// Zima Connect Server — Peer-to-Server-to-Peer connectivity hub
package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/rs/zerolog"

	"github.com/zima-connect/server/internal/api"
	"github.com/zima-connect/server/internal/config"
	"github.com/zima-connect/server/internal/service"
	"github.com/zima-connect/server/internal/store"
)

func main() {
	// 1. Configuration
	cfg := config.Load()

	// 2. Logging setup
	logLevel, err := zerolog.ParseLevel(cfg.LogLevel)
	if err != nil {
		logLevel = zerolog.InfoLevel
	}
	zerolog.SetGlobalLevel(logLevel)

	output := zerolog.ConsoleWriter{Out: os.Stdout, TimeFormat: time.RFC3339}
	log := zerolog.New(output).With().Timestamp().Logger()

	log.Info().
		Str("server_name", cfg.ServerName).
		Int("port", cfg.Port).
		Int("dashboard_port", cfg.DashboardPort).
		Str("data_dir", cfg.DataDir).
		Msg("starting Zima Connect server")

	// 3. Database Store
	dbStore, err := store.New(cfg.DatabasePath(), log)
	if err != nil {
		log.Fatal().Err(err).Msg("failed to initialize SQLite database")
	}
	defer dbStore.Close()

	// Reset any stale online flags on server startup
	if err := dbStore.ResetAllOnline(); err != nil {
		log.Warn().Err(err).Msg("failed to reset device online states")
	}

	// 4. Service layer initialization
	authSvc := service.NewAuthService(dbStore, cfg, log)
	deviceReg := service.NewDeviceRegistry(dbStore, log)
	msgRouter := service.NewMessageRouter(dbStore, log)
	fileStaging := service.NewFileStaging(dbStore, cfg, log)
	queueSvc := service.NewQueue(dbStore, log)

	if err := fileStaging.EnsureStagingDir(); err != nil {
		log.Fatal().Err(err).Msg("failed to create file staging directory")
	}

	services := &api.Services{
		Store:          dbStore,
		Auth:           authSvc,
		DeviceRegistry: deviceReg,
		MessageRouter:  msgRouter,
		FileStaging:    fileStaging,
		Queue:          queueSvc,
	}

	// 5. Periodic cleanup ticker (every 10 minutes)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	go func() {
		ticker := time.NewTicker(10 * time.Minute)
		defer ticker.Stop()

		for {
			select {
			case <-ticker.C:
				if _, err := queueSvc.CleanupExpired(); err != nil {
					log.Error().Err(err).Msg("error cleaning up expired messages")
				}
				if _, err := fileStaging.CleanupExpired(); err != nil {
					log.Error().Err(err).Msg("error cleaning up expired files")
				}
			case <-ctx.Done():
				return
			}
		}
	}()

	// 6. Setup Fiber Apps
	app := api.NewApp(cfg, services, log)
	dashApp := api.NewDashboardApp(cfg, services, log)

	// 7. Start Dashboard server in background
	go func() {
		addr := fmt.Sprintf(":%d", cfg.DashboardPort)
		log.Info().Str("addr", addr).Msg("dashboard HTTP server listening")
		if err := dashApp.Listen(addr); err != nil {
			log.Info().Err(err).Msg("dashboard server stopped")
		}
	}()

	// 8. Start Main API server in background
	go func() {
		addr := fmt.Sprintf(":%d", cfg.Port)
		log.Info().Str("addr", addr).Msg("main API & WebSocket server listening")
		if err := app.Listen(addr); err != nil {
			log.Info().Err(err).Msg("main server stopped")
		}
	}()

	// 9. Graceful shutdown
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, os.Interrupt, syscall.SIGTERM)
	<-sigChan

	log.Info().Msg("shutdown signal received, closing gracefully...")

	cancel()
	_ = app.Shutdown()
	_ = dashApp.Shutdown()

	log.Info().Msg("Zima Connect server stopped")
}
