package config

import (
	"os"
	"testing"
	"time"
)

func TestLoadDefaults(t *testing.T) {
	cfg := Load()
	if cfg.Port != 9120 {
		t.Errorf("expected default port 9120, got %d", cfg.Port)
	}
	if cfg.DashboardPort != 9121 {
		t.Errorf("expected default dashboard port 9121, got %d", cfg.DashboardPort)
	}
	if cfg.ServerName != "Zima Connect" {
		t.Errorf("expected default server name 'Zima Connect', got %s", cfg.ServerName)
	}
}

func TestLoadCustomEnv(t *testing.T) {
	os.Setenv("ZC_PORT", "9999")
	os.Setenv("ZC_SERVER_NAME", "Custom Hub")
	defer func() {
		os.Unsetenv("ZC_PORT")
		os.Unsetenv("ZC_SERVER_NAME")
	}()

	cfg := Load()
	if cfg.Port != 9999 {
		t.Errorf("expected port 9999, got %d", cfg.Port)
	}
	if cfg.ServerName != "Custom Hub" {
		t.Errorf("expected server name 'Custom Hub', got %s", cfg.ServerName)
	}
}
