use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
}

/// Sizes and hashes are the Hugging Face LFS values for
/// `ggerganov/whisper.cpp` at `main`, verified against locally computed
/// SHA-256 digests. `tiny.en` and `base.en` return unpunctuated text, which is
/// why the default is `small.en` — see `note`.
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "tiny.en",
        name: "Tiny (English)",
        note: "Fastest. No punctuation — pair with a formatter.",
        size_bytes: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
    },
    CatalogEntry {
        id: "base.en",
        name: "Base (English)",
        note: "Fast. Light punctuation.",
        size_bytes: 147_964_211,
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    },
    CatalogEntry {
        id: "small.en",
        name: "Small (English)",
        note: "Recommended. Punctuates correctly, ~24x realtime.",
        size_bytes: 487_614_201,
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
    },
    CatalogEntry {
        id: "large-v3-turbo",
        name: "Large v3 Turbo",
        note: "Most accurate. Multilingual. Slower, 1.6 GB.",
        size_bytes: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    },
];

#[derive(Serialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub note: String,
    pub size_bytes: u64,
    pub downloaded: bool,
}

#[derive(Serialize, Clone)]
struct DownloadProgress {
    id: String,
    downloaded: u64,
    total: u64,
}

pub fn find(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

pub fn models_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config").join("voicebox").join("models"))
        .unwrap_or_else(|| PathBuf::from("models"))
}

/// Resolves only ids present in `CATALOG`, so a caller-supplied id can never
/// escape the models directory.
pub fn model_path(id: &str) -> Option<PathBuf> {
    find(id).map(|e| models_dir().join(format!("ggml-{}.bin", e.id)))
}

pub fn is_downloaded(id: &str) -> bool {
    model_path(id).is_some_and(|p| p.is_file())
}

pub fn list() -> Vec<ModelInfo> {
    CATALOG
        .iter()
        .map(|e| ModelInfo {
            id: e.id.into(),
            name: e.name.into(),
            note: e.note.into(),
            size_bytes: e.size_bytes,
            downloaded: is_downloaded(e.id),
        })
        .collect()
}

pub fn delete(id: &str) -> Result<(), String> {
    let path = model_path(id).ok_or_else(|| format!("Unknown model: {}", id))?;
    if !path.is_file() {
        return Ok(());
    }
    std::fs::remove_file(&path).map_err(|e| format!("Failed to delete {}: {}", path.display(), e))
}

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

pub async fn download(app: AppHandle, id: String) -> Result<(), String> {
    let entry = find(&id).ok_or_else(|| format!("Unknown model: {}", id))?;
    let final_path = models_dir().join(format!("ggml-{}.bin", entry.id));

    if final_path.is_file() {
        let _ = app.emit("voicebox:model_download_complete", &id);
        return Ok(());
    }

    let result = download_inner(&app, entry, &final_path).await;

    match &result {
        Ok(()) => {
            log::info!("[models] downloaded {}", entry.id);
            let _ = app.emit("voicebox:model_download_complete", &id);
        }
        Err(e) => {
            log::error!("[models] download {} failed: {}", entry.id, e);
            let _ = app.emit(
                "voicebox:model_download_error",
                serde_json::json!({ "id": id, "message": e }),
            );
        }
    }

    result
}

async fn download_inner(
    app: &AppHandle,
    entry: &CatalogEntry,
    final_path: &std::path::Path,
) -> Result<(), String> {
    let dir = models_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Creating {}: {}", dir.display(), e))?;

    let partial_path = dir.join(format!("ggml-{}.bin.partial", entry.id));
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        entry.id
    );

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Requesting {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!("Download returned HTTP {}", response.status()));
    }

    let total = response.content_length().unwrap_or(entry.size_bytes);

    let mut file = std::fs::File::create(&partial_path)
        .map_err(|e| format!("Creating {}: {}", partial_path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                cleanup(&partial_path);
                return Err(format!("Stream error: {}", e));
            }
        };

        if let Err(e) = file.write_all(&chunk) {
            cleanup(&partial_path);
            return Err(format!("Writing {}: {}", partial_path.display(), e));
        }
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;

        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            let _ = app.emit(
                "voicebox:model_download_progress",
                DownloadProgress {
                    id: entry.id.into(),
                    downloaded,
                    total,
                },
            );
        }
    }

    if let Err(e) = file.flush() {
        cleanup(&partial_path);
        return Err(format!("Flushing {}: {}", partial_path.display(), e));
    }
    drop(file);

    // Always emit the final byte count so the UI can't stall short of 100%.
    let _ = app.emit(
        "voicebox:model_download_progress",
        DownloadProgress {
            id: entry.id.into(),
            downloaded,
            total,
        },
    );

    let digest = format!("{:x}", hasher.finalize());
    if digest != entry.sha256 {
        cleanup(&partial_path);
        return Err(format!(
            "Checksum mismatch for {} (expected {}, got {})",
            entry.id, entry.sha256, digest
        ));
    }

    std::fs::rename(&partial_path, final_path).map_err(|e| {
        cleanup(&partial_path);
        format!("Moving into place: {}", e)
    })
}

fn cleanup(partial_path: &std::path::Path) {
    let _ = std::fs::remove_file(partial_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique() {
        let mut ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate model id in catalog");
    }

    #[test]
    fn catalog_hashes_are_well_formed() {
        for entry in CATALOG {
            assert_eq!(entry.sha256.len(), 64, "{}: sha256 wrong length", entry.id);
            assert!(
                entry.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}: sha256 not hex",
                entry.id
            );
            assert!(entry.size_bytes > 0, "{}: zero size", entry.id);
        }
    }

    #[test]
    fn default_config_model_exists_in_catalog() {
        let default_model = crate::config::default_config().local.model;
        assert!(
            find(&default_model).is_some(),
            "default model {:?} is not in the catalog",
            default_model
        );
    }

    #[test]
    fn unknown_id_resolves_to_no_path() {
        assert!(model_path("../../etc/passwd").is_none());
        assert!(model_path("nonexistent").is_none());
    }

    #[test]
    fn known_id_resolves_inside_models_dir() {
        let path = model_path("small.en").expect("small.en is in the catalog");
        assert!(path.starts_with(models_dir()));
        assert!(path.ends_with("ggml-small.en.bin"));
    }
}
