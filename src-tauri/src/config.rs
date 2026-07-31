use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderConfig,
    pub cloud: CloudConfig,
    pub local: LocalConfig,
    pub audio: AudioConfig,
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub beta: BetaConfig,
    #[serde(default)]
    pub meeting: MeetingConfig,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: String,
}

fn default_overlay_position() -> String {
    "top_center".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    pub account_id: String,
    pub api_token: String,
    pub stt_model: String,
    pub formatter_model: String,
    pub worker_url: String,
    pub token: String,
}

// Per-field defaults: a config written by a build with a different local
// block (e.g. the on-device whisper branch) must still parse — a parse
// failure here falls through to load()'s create-default path, which
// overwrites the user's config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    #[serde(default = "default_server_url")]
    pub server_url: String,
    #[serde(default)]
    pub token: String,
}

fn default_server_url() -> String {
    "http://192.168.1.183:9090".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub record: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaConfig {
    pub streaming_stt: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeetingConfig {
    /// Empty means `~/Documents/VoiceBox Meetings`.
    #[serde(default)]
    pub save_dir: String,
}

pub fn default_config() -> Config {
    Config {
        provider: ProviderConfig {
            mode: "cloud".into(),
        },
        cloud: CloudConfig {
            account_id: String::new(),
            api_token: String::new(),
            stt_model: "@cf/openai/whisper-large-v3-turbo".into(),
            formatter_model: "@cf/ibm-granite/granite-4.0-h-micro".into(),
            worker_url: String::new(),
            token: String::new(),
        },
        local: LocalConfig {
            server_url: default_server_url(),
            token: String::new(),
        },
        audio: AudioConfig {
            sample_rate: 16000,
            channels: 1,
            chunk_size: 4096,
        },
        hotkey: HotkeyConfig {
            record: "ctrl+cmd".into(),
        },
        beta: BetaConfig::default(),
        meeting: MeetingConfig::default(),
        overlay_position: default_overlay_position(),
    }
}

fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("VOICEBOX_CONFIG") {
        paths.push(PathBuf::from(path));
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".config").join("voicebox").join("voicebox.json"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("voicebox.json"));
        }
    }
    paths.push(PathBuf::from("voicebox.json"));
    paths
}

fn toml_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".config").join("voicebox").join("voicebox.toml"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("voicebox.toml"));
        }
    }
    paths.push(PathBuf::from("voicebox.toml"));
    paths
}

pub fn load() -> (Config, PathBuf) {
    // Try JSON first
    for path in config_search_paths() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&data) {
                log::info!("Loaded config from {}", path.display());
                return (cfg, path);
            }
        }
    }

    // Try TOML migration
    for path in toml_search_paths() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(cfg) = toml::from_str::<Config>(&data) {
                log::info!("Migrating TOML config from {}", path.display());
                let json_path = path.with_extension("json");
                if save(&json_path, &cfg).is_ok() {
                    log::info!("Migrated config to {}", json_path.display());
                }
                return (cfg, json_path);
            }
        }
    }

    // Create default
    let cfg = default_config();
    let default_path = dirs::home_dir()
        .map(|h| h.join(".config").join("voicebox").join("voicebox.json"))
        .unwrap_or_else(|| PathBuf::from("voicebox.json"));

    if let Some(parent) = default_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = save(&default_path, &cfg);
    log::info!("Created default config at {}", default_path.display());

    (cfg, default_path)
}

pub fn save(path: &Path, cfg: &Config) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config written by the on-device whisper branch has a different
    /// `local` block. It must still parse — failing here means load() wipes
    /// the user's config with defaults.
    #[test]
    fn foreign_local_block_parses_and_preserves_cloud_credentials() {
        let json = r#"{
          "provider": { "mode": "local" },
          "cloud": {
            "account_id": "", "api_token": "",
            "stt_model": "@cf/openai/whisper-large-v3-turbo",
            "formatter_model": "@cf/ibm-granite/granite-4.0-h-micro",
            "worker_url": "wss://voicebox.example.workers.dev",
            "token": "secret-worker-token"
          },
          "local": {
            "model": "small.en", "language": "en", "formatter": "none",
            "ollama_url": "http://localhost:11434", "ollama_model": "qwen3:4b"
          },
          "audio": { "sample_rate": 16000, "channels": 1, "chunk_size": 4096 },
          "hotkey": { "record": "ctrl+cmd" },
          "beta": { "streaming_stt": true },
          "overlay_position": "bottom_center"
        }"#;

        let cfg: Config = serde_json::from_str(json)
            .expect("foreign local block must not fail the whole config");
        assert_eq!(cfg.cloud.token, "secret-worker-token");
        assert_eq!(cfg.local.server_url, default_server_url());
        assert_eq!(cfg.meeting.save_dir, "");
    }

    #[test]
    fn meeting_section_roundtrips() {
        let mut cfg = default_config();
        cfg.meeting.save_dir = "/tmp/meetings".into();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.meeting.save_dir, "/tmp/meetings");
    }
}
