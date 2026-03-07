package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDefaultConfig(t *testing.T) {
	cfg := DefaultConfig()

	if cfg.Provider.Mode != ModeCloud {
		t.Errorf("expected mode %q, got %q", ModeCloud, cfg.Provider.Mode)
	}
	if cfg.Audio.SampleRate != 16000 {
		t.Errorf("expected sample rate 16000, got %d", cfg.Audio.SampleRate)
	}
	if cfg.Audio.Channels != 1 {
		t.Errorf("expected channels 1, got %d", cfg.Audio.Channels)
	}
	if cfg.Audio.ChunkSize != 4096 {
		t.Errorf("expected chunk size 4096, got %d", cfg.Audio.ChunkSize)
	}
	if cfg.Hotkey.Record != "ctrl+shift+r" {
		t.Errorf("expected hotkey %q, got %q", "ctrl+shift+r", cfg.Hotkey.Record)
	}
	if cfg.Cloud.STTModel != "@cf/openai/whisper-large-v3-turbo" {
		t.Errorf("expected stt model %q, got %q", "@cf/openai/whisper-large-v3-turbo", cfg.Cloud.STTModel)
	}
	if cfg.Cloud.FormatterModel != "@cf/ibm-granite/granite-4.0-h-micro" {
		t.Errorf("expected formatter model %q, got %q", "@cf/ibm-granite/granite-4.0-h-micro", cfg.Cloud.FormatterModel)
	}
}

func TestLoad(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "voicebox.toml")

	content := `
[provider]
mode = "local"

[cloud]
account_id = "abc123"
api_token = "tok_secret"
worker_url = "wss://voicebox.example.com/transcribe"
token = "my-token"

[audio]
sample_rate = 44100
channels = 2
chunk_size = 8192

[hotkey]
record = "ctrl+alt+r"
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	cfg, err := Load(path)
	if err != nil {
		t.Fatal(err)
	}

	if cfg.Provider.Mode != ModeLocal {
		t.Errorf("expected mode %q, got %q", ModeLocal, cfg.Provider.Mode)
	}
	if cfg.Cloud.AccountID != "abc123" {
		t.Errorf("expected account_id %q, got %q", "abc123", cfg.Cloud.AccountID)
	}
	if cfg.Cloud.WorkerURL != "wss://voicebox.example.com/transcribe" {
		t.Errorf("expected worker_url %q, got %q", "wss://voicebox.example.com/transcribe", cfg.Cloud.WorkerURL)
	}
	if cfg.Cloud.Token != "my-token" {
		t.Errorf("expected token %q, got %q", "my-token", cfg.Cloud.Token)
	}
	if cfg.Audio.SampleRate != 44100 {
		t.Errorf("expected sample rate 44100, got %d", cfg.Audio.SampleRate)
	}
	if cfg.Audio.Channels != 2 {
		t.Errorf("expected channels 2, got %d", cfg.Audio.Channels)
	}
	if cfg.Audio.ChunkSize != 8192 {
		t.Errorf("expected chunk size 8192, got %d", cfg.Audio.ChunkSize)
	}
	if cfg.Hotkey.Record != "ctrl+alt+r" {
		t.Errorf("expected hotkey %q, got %q", "ctrl+alt+r", cfg.Hotkey.Record)
	}
}

func TestLoadPartialConfig(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "voicebox.toml")

	content := `
[cloud]
worker_url = "wss://example.com/transcribe"
`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	cfg, err := Load(path)
	if err != nil {
		t.Fatal(err)
	}

	if cfg.Provider.Mode != ModeCloud {
		t.Errorf("expected default mode %q, got %q", ModeCloud, cfg.Provider.Mode)
	}
	if cfg.Audio.SampleRate != 16000 {
		t.Errorf("expected default sample rate 16000, got %d", cfg.Audio.SampleRate)
	}
	if cfg.Cloud.WorkerURL != "wss://example.com/transcribe" {
		t.Errorf("expected worker_url %q, got %q", "wss://example.com/transcribe", cfg.Cloud.WorkerURL)
	}
	if cfg.Cloud.STTModel != "@cf/openai/whisper-large-v3-turbo" {
		t.Errorf("expected default stt model preserved, got %q", cfg.Cloud.STTModel)
	}
}
