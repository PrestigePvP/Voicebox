use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderConfig,
    pub cloud: CloudConfig,
    #[serde(default)]
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
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_formatter")]
    pub formatter: String,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
}

fn default_model() -> String {
    "small.en".into()
}

fn default_language() -> String {
    "en".into()
}

fn default_formatter() -> String {
    "none".into()
}

fn default_ollama_url() -> String {
    "http://localhost:11434".into()
}

fn default_ollama_model() -> String {
    "qwen3:4b".into()
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            language: default_language(),
            formatter: default_formatter(),
            ollama_url: default_ollama_url(),
            ollama_model: default_ollama_model(),
        }
    }
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
        local: LocalConfig::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    const PRE_ONDEVICE_CONFIG: &str = r#"{
      "provider": { "mode": "local" },
      "cloud": {
        "account_id": "acct-123",
        "api_token": "secret-api-token",
        "stt_model": "@cf/openai/whisper-large-v3-turbo",
        "formatter_model": "@cf/ibm-granite/granite-4.0-h-micro",
        "worker_url": "https://voicebox.example.workers.dev",
        "token": "secret-worker-token"
      },
      "local": { "server_url": "http://10.0.0.5:9090", "token": "lan-token" },
      "audio": { "sample_rate": 16000, "channels": 1, "chunk_size": 4096 },
      "hotkey": { "record": "ctrl+cmd" },
      "overlay_position": "bottom_right"
    }"#;

    #[test]
    fn old_config_parses_and_preserves_cloud_credentials() {
        let cfg: Config = serde_json::from_str(PRE_ONDEVICE_CONFIG)
            .expect("pre-on-device config must still deserialize; failing here wipes user settings");

        assert_eq!(cfg.cloud.api_token, "secret-api-token");
        assert_eq!(cfg.cloud.token, "secret-worker-token");
        assert_eq!(cfg.hotkey.record, "ctrl+cmd");
        assert_eq!(cfg.overlay_position, "bottom_right");
    }

    #[test]
    fn old_local_block_falls_back_to_ondevice_defaults() {
        let cfg: Config = serde_json::from_str(PRE_ONDEVICE_CONFIG).unwrap();

        assert_eq!(cfg.provider.mode, "local");
        assert_eq!(cfg.local.model, "small.en");
        assert_eq!(cfg.local.formatter, "none");
        assert_eq!(cfg.local.language, "en");
    }

    #[test]
    fn missing_local_block_uses_defaults() {
        let json = r#"{
          "provider": { "mode": "cloud" },
          "cloud": {
            "account_id": "", "api_token": "", "stt_model": "m",
            "formatter_model": "f", "worker_url": "", "token": ""
          },
          "audio": { "sample_rate": 16000, "channels": 1, "chunk_size": 4096 },
          "hotkey": { "record": "ctrl+cmd" }
        }"#;

        let cfg: Config = serde_json::from_str(json).expect("absent local block must default");
        assert_eq!(cfg.local.model, "small.en");
        assert_eq!(cfg.local.ollama_url, "http://localhost:11434");
    }

    /// Parses a real config file when `VOICEBOX_TEST_CONFIG` points at one.
    /// The synthetic fixtures above cover the shape; this covers an actual
    /// on-disk file, which is what a failed migration would destroy.
    #[test]
    fn real_config_file_still_parses() {
        let Ok(path) = std::env::var("VOICEBOX_TEST_CONFIG") else {
            eprintln!("skipping: set VOICEBOX_TEST_CONFIG to a config path to run");
            return;
        };
        let data = fs::read_to_string(&path).expect("read config");
        let cfg: Config = serde_json::from_str(&data)
            .unwrap_or_else(|e| panic!("real config at {} failed to parse: {}", path, e));

        let original: serde_json::Value = serde_json::from_str(&data).unwrap();
        let cloud = &original["cloud"];
        assert_eq!(cfg.cloud.token, cloud["token"].as_str().unwrap_or_default());
        assert_eq!(
            cfg.cloud.worker_url,
            cloud["worker_url"].as_str().unwrap_or_default()
        );
        assert_eq!(
            cfg.hotkey.record,
            original["hotkey"]["record"].as_str().unwrap_or_default()
        );
        assert!(!cfg.local.model.is_empty(), "local model must default");
    }

    #[test]
    fn roundtrip_preserves_local_settings() {
        let mut cfg = default_config();
        cfg.local.model = "large-v3-turbo".into();
        cfg.local.formatter = "ollama".into();

        let json = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(back.local.model, "large-v3-turbo");
        assert_eq!(back.local.formatter, "ollama");
    }
}
