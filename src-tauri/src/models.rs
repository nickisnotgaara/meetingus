//! Whisper model catalog, hardware-based default and downloader (D13).
//! Models live in the app data dir; dev builds may reuse repo `models/`.
//! The catalog is deliberately small: base for CPU, small/turbo for GPUs.

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub struct ModelSpec {
    pub id: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub label: &'static str,
    pub note: &'static str,
}

/// `bytes` is the upstream size, used for the progress bar before the server
/// reports content-length. base/small match the files shipped in `models/`.
pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "base",
        file: "ggml-base.bin",
        bytes: 147_951_465,
        label: "Base",
        note: "Fastest. Best on CPU-only machines.",
    },
    ModelSpec {
        id: "small",
        file: "ggml-small.bin",
        bytes: 487_601_967,
        label: "Small",
        note: "Good accuracy, still fast on a GPU.",
    },
    ModelSpec {
        id: "turbo",
        file: "ggml-large-v3-turbo.bin",
        bytes: 1_624_555_275,
        label: "Turbo",
        note: "Best accuracy. Needs a GPU with 12 GB+ VRAM.",
    },
];

pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    CATALOG.iter().find(|m| m.id == id)
}

/// Recommended model: CPU builds stay on base; CUDA builds scale with VRAM.
/// 8 GB (RTX 4060) -> small per replica/ai-stack.md.
pub fn recommended_id(cuda: bool, vram_mb: Option<u64>) -> &'static str {
    if !cuda {
        return "base";
    }
    match vram_mb {
        Some(v) if v >= 12_288 => "turbo",
        Some(v) if v >= 6_144 => "small",
        _ => "base",
    }
}

/// Parse `nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits`.
/// Returns the largest GPU's VRAM in MiB.
pub fn parse_vram_mb(stdout: &str) -> Option<u64> {
    stdout
        .lines()
        .filter_map(|l| l.trim().parse::<u64>().ok())
        .max()
}

/// Probe the GPU once at startup. Missing nvidia-smi simply means "no CUDA".
pub fn detect_vram_mb() -> Option<u64> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_vram_mb(&String::from_utf8_lossy(&out.stdout))
}

pub fn models_dir(app_data: &Path) -> PathBuf {
    app_data.join("models")
}

pub fn model_file(app_data: &Path, id: &str) -> Option<PathBuf> {
    spec(id).map(|m| models_dir(app_data).join(m.file))
}

/// Files that look complete (the downloader writes `.part` first).
pub fn is_downloaded(app_data: &Path, id: &str) -> bool {
    model_file(app_data, id)
        .map(|p| p.is_file() && p.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false))
        .unwrap_or(false)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub bytes: u64,
    pub note: &'static str,
    pub downloaded: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub models: Vec<ModelInfo>,
    pub recommended: &'static str,
    pub selected: Option<String>,
    pub cuda: bool,
    pub vram_mb: Option<u64>,
}

pub fn catalog(app_data: &Path, selected: Option<String>) -> Catalog {
    let cuda = cfg!(feature = "cuda");
    let vram_mb = if cuda { detect_vram_mb() } else { None };
    Catalog {
        models: CATALOG
            .iter()
            .map(|m| ModelInfo {
                id: m.id,
                label: m.label,
                bytes: m.bytes,
                note: m.note,
                downloaded: is_downloaded(app_data, m.id),
            })
            .collect(),
        recommended: recommended_id(cuda, vram_mb),
        selected,
        cuda,
        vram_mb,
    }
}

/// Stream a model into `<dest>.part`, then rename it into place. Emits
/// `model://progress` roughly every 200 ms. Cancellation removes the part.
pub async fn download(
    app: &AppHandle,
    id: &str,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<u64> {
    let Some(spec) = spec(id) else {
        bail!("unknown model: {id}");
    };
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let url = format!("{HF_BASE}/{}", spec.file);
    let response = reqwest::get(&url)
        .await
        .with_context(|| format!("download {url}"))?
        .error_for_status()
        .with_context(|| format!("download {url}"))?;
    let total = response.content_length().unwrap_or(spec.bytes);
    let part = dest.with_extension("bin.part");
    let mut file = std::fs::File::create(&part)
        .with_context(|| format!("create {}", part.display()))?;
    let mut stream = response.bytes_stream();
    let mut done: u64 = 0;
    let mut last_emit = Instant::now();
    let _ = app.emit(
        "model://progress",
        serde_json::json!({ "id": id, "downloaded": 0, "total": total }),
    );
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            bail!("download cancelled");
        }
        let chunk = chunk.context("read model body")?;
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(200) {
            last_emit = Instant::now();
            let _ = app.emit(
                "model://progress",
                serde_json::json!({ "id": id, "downloaded": done, "total": total }),
            );
        }
    }
    file.flush()?;
    drop(file);
    if done < 1_000_000 {
        let _ = std::fs::remove_file(&part);
        bail!("download too small ({done} bytes)");
    }
    std::fs::rename(&part, dest)
        .with_context(|| format!("move {} into place", part.display()))?;
    let _ = app.emit(
        "model://progress",
        serde_json::json!({ "id": id, "downloaded": done, "total": total, "done": true }),
    );
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_resolvable() {
        let mut ids: Vec<_> = CATALOG.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        for m in CATALOG {
            assert_eq!(spec(m.id).map(|s| s.file), Some(m.file));
        }
        assert!(spec("huge").is_none());
    }

    #[test]
    fn cpu_builds_always_get_base() {
        assert_eq!(recommended_id(false, None), "base");
        assert_eq!(recommended_id(false, Some(24_576)), "base");
    }

    #[test]
    fn gpu_recommendation_scales_with_vram() {
        assert_eq!(recommended_id(true, Some(4_096)), "base");
        assert_eq!(recommended_id(true, Some(8_192)), "small");
        assert_eq!(recommended_id(true, Some(12_288)), "turbo");
        assert_eq!(recommended_id(true, None), "base");
    }

    #[test]
    fn vram_parser_takes_the_largest_gpu() {
        assert_eq!(parse_vram_mb("8192\n24576\n"), Some(24_576));
        assert_eq!(parse_vram_mb("  6144 \r\n"), Some(6_144));
        assert_eq!(parse_vram_mb("N/A\n"), None);
        assert_eq!(parse_vram_mb(""), None);
    }

    #[test]
    fn model_file_lives_under_app_data_models() {
        let dir = std::path::Path::new("C:/data");
        assert_eq!(
            model_file(dir, "small").unwrap(),
            std::path::Path::new("C:/data/models/ggml-small.bin")
        );
        assert!(model_file(dir, "nope").is_none());
        assert!(!is_downloaded(dir, "small"));
    }
}
