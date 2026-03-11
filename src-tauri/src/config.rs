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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    pub server_url: String,
    pub token: String,
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
            server_url: "http://192.168.1.183:9090".into(),
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
        overlay_position: default_overlay_position(),
    }
}

fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
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
