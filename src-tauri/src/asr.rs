//! Streaming ASR on whisper.cpp. Model is preloaded once (T12); VAD segments
//! are transcribed as they finish. Language resolution is the only pure logic
//! unit-testable without a model file; the fixture test is env-gated.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Resolve a language setting to whisper's Option: "auto"/empty -> None.
pub fn resolve_language(setting: &str) -> Option<String> {
    let s = setting.trim();
    if s.is_empty() || s == "auto" {
        None
    } else {
        Some(s.to_string())
    }
}

pub struct Transcriber {
    ctx: Mutex<WhisperContext>,
    language: Option<String>,
    threads: i32,
}

impl Transcriber {
    pub fn new(model_path: &Path, language: &str, threads: i32) -> Result<Self> {
        let path = model_path.to_string_lossy().to_string();
        let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
            .with_context(|| format!("failed to load whisper model at {path}"))?;
        Ok(Self {
            ctx: Mutex::new(ctx),
            language: resolve_language(language),
            threads,
        })
    }

    /// Transcribe 16 kHz mono PCM; `language` overrides the default when set.
    pub fn transcribe(&self, samples: &[i16], language: &str) -> Result<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }
        let language = resolve_language(language).or_else(|| self.language.clone());
        let pcm: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();
        let ctx = self
            .ctx
            .lock()
            .map_err(|_| anyhow::anyhow!("transcriber lock poisoned"))?;
        let mut state = ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.threads);
        params.set_language(language.as_deref());
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        state.full(params, &pcm)?;
        let segments = state.full_n_segments();
        let mut text = String::new();
        for index in 0..segments {
            if let Some(segment) = state.get_segment(index) {
                if let Ok(segment_text) = segment.to_str() {
                    text.push_str(segment_text);
                }
            }
        }
        Ok(text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_and_empty_resolve_to_none() {
        assert_eq!(resolve_language("auto"), None);
        assert_eq!(resolve_language(""), None);
    }

    #[test]
    fn explicit_languages_pass_through() {
        assert_eq!(resolve_language("ru"), Some("ru".to_string()));
        assert_eq!(resolve_language("en"), Some("en".to_string()));
    }

    /// Env-gated end-to-end proof: needs models/ggml-base.bin + the spike wav.
    /// CPU-only build, first 10 s -> a few seconds. Run explicitly:
    /// `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn transcribe_fixture_first_10_seconds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let model = root.join("models/ggml-base.bin");
        let wav = root.join("replica/spike/spike-en-16k.wav");
        if !model.exists() || !wav.exists() {
            eprintln!("SKIP: fixture missing (models/ggml-base.bin or spike wav)");
            return;
        }
        let data = std::fs::read(&wav).expect("read fixture");
        let samples: Vec<i16> = data[44..44 + 160_000 * 2]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        let t = Transcriber::new(&model, "en", 6).expect("load model");
        let text = t.transcribe(&samples, "en").expect("transcribe");
        assert!(
            text.to_lowercase().contains("quarterly"),
            "expected fixture speech, got: {text}"
        );
    }
}
