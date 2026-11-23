use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: Mutex<WhisperContext>,
    language: Option<String>,
    threads: i32,
}

impl Transcriber {
    pub fn new(model_path: &Path, language: &str, threads: i32) -> anyhow::Result<Self> {
        let path = model_path.to_string_lossy().to_string();
        let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
            .with_context(|| format!("failed to load whisper model at {path}"))?;
        let language = if language.is_empty() || language == "auto" {
            None
        } else {
            Some(language.to_string())
        };
        Ok(Self {
            ctx: Mutex::new(ctx),
            language,
            threads,
        })
    }

    pub fn transcribe(&self, samples: &[i16], language: &str) -> anyhow::Result<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }
        let language = if language.is_empty() || language == "auto" {
            None
        } else {
            Some(language.to_string())
        };
        let language = language.as_deref().or(self.language.as_deref());
        let pcm: Vec<f32> = samples.iter().map(|&sample| sample as f32 / 32768.0).collect();
        let ctx = self
            .ctx
            .lock()
            .map_err(|_| anyhow::anyhow!("transcriber lock poisoned"))?;
        let mut state = ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.threads);
        params.set_language(language);
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
