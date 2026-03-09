package config

import (
	"bytes"
	"os"

	"github.com/BurntSushi/toml"
)

type Mode string

const (
	ModeCloud Mode = "cloud"
	ModeLocal Mode = "local"
)

type Config struct {
	Provider ProviderConfig `toml:"provider" json:"provider"`
	Cloud    CloudConfig    `toml:"cloud" json:"cloud"`
	Local    LocalConfig    `toml:"local" json:"local"`
	Audio    AudioConfig    `toml:"audio" json:"audio"`
	Hotkey   HotkeyConfig   `toml:"hotkey" json:"hotkey"`
}

type ProviderConfig struct {
	Mode Mode `toml:"mode" json:"mode"`
}

type CloudConfig struct {
	AccountID      string `toml:"account_id" json:"account_id"`
	APIToken       string `toml:"api_token" json:"api_token"`
	STTModel       string `toml:"stt_model" json:"stt_model"`
	FormatterModel string `toml:"formatter_model" json:"formatter_model"`
	WorkerURL      string `toml:"worker_url" json:"worker_url"`
	Token          string `toml:"token" json:"token"`
}

type LocalConfig struct {
	STTEndpoint       string `toml:"stt_endpoint" json:"stt_endpoint"`
	FormatterEndpoint string `toml:"formatter_endpoint" json:"formatter_endpoint"`
	FormatterModel    string `toml:"formatter_model" json:"formatter_model"`
}

type AudioConfig struct {
	SampleRate int `toml:"sample_rate" json:"sample_rate"`
	Channels   int `toml:"channels" json:"channels"`
	ChunkSize  int `toml:"chunk_size" json:"chunk_size"`
}

type HotkeyConfig struct {
	Record string `toml:"record" json:"record"`
}

func DefaultConfig() *Config {
	return &Config{
		Provider: ProviderConfig{Mode: ModeCloud},
		Cloud: CloudConfig{
			STTModel:       "@cf/openai/whisper-large-v3-turbo",
			FormatterModel: "@cf/ibm-granite/granite-4.0-h-micro",
		},
		Audio: AudioConfig{
			SampleRate: 16000,
			Channels:   1,
			ChunkSize:  4096,
		},
		Hotkey: HotkeyConfig{Record: "ctrl+cmd"},
	}
}

func Load(path string) (*Config, error) {
	cfg := DefaultConfig()
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if err := toml.Unmarshal(data, cfg); err != nil {
		return nil, err
	}
	return cfg, nil
}

func Save(path string, cfg *Config) error {
	var buf bytes.Buffer
	if err := toml.NewEncoder(&buf).Encode(cfg); err != nil {
		return err
	}
	return os.WriteFile(path, buf.Bytes(), 0o644)
}
