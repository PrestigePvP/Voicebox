package config

import (
	"os"

	"github.com/BurntSushi/toml"
)

type Mode string

const (
	ModeCloud Mode = "cloud"
	ModeLocal Mode = "local"
)

type Config struct {
	Provider ProviderConfig `toml:"provider"`
	Cloud    CloudConfig    `toml:"cloud"`
	Local    LocalConfig    `toml:"local"`
	Audio    AudioConfig    `toml:"audio"`
	Hotkey   HotkeyConfig   `toml:"hotkey"`
}

type ProviderConfig struct {
	Mode Mode `toml:"mode"`
}

type CloudConfig struct {
	AccountID      string `toml:"account_id"`
	APIToken       string `toml:"api_token"`
	STTModel       string `toml:"stt_model"`
	FormatterModel string `toml:"formatter_model"`
	WorkerURL      string `toml:"worker_url"`
	Token          string `toml:"token"`
}

type LocalConfig struct {
	STTEndpoint       string `toml:"stt_endpoint"`
	FormatterEndpoint string `toml:"formatter_endpoint"`
	FormatterModel    string `toml:"formatter_model"`
}

type AudioConfig struct {
	SampleRate int `toml:"sample_rate"`
	Channels   int `toml:"channels"`
	ChunkSize  int `toml:"chunk_size"`
}

type HotkeyConfig struct {
	Record string `toml:"record"`
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
		Hotkey: HotkeyConfig{Record: "ctrl+shift+r"},
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
