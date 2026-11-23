use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub api_key: String,
    pub llm_base: String,
    pub llm_model: String,
    pub vision_model: String,
    pub conversation_languages: String,
    pub language: String,
    pub stt_model_path: PathBuf,
    pub stt_threads: i32,
    pub persona: String,
    pub overlay_title: String,
    pub system_audio: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            api_key: env_or("OPENAI_API_KEY", "ollama"),
            llm_base: env_or("LLM_BASE_URL", "http://127.0.0.1:11434/v1"),
            llm_model: env_or("LLM_MODEL", "qwen3:8b"),
            vision_model: env_or("LLM_VISION_MODEL", "qwen2.5vl:3b"),
            conversation_languages: env_or(
                "LANGUAGES",
                "English, Russian and Uzbek",
            ),
            language: env_or("LANGUAGE", "auto"),
            stt_model_path: PathBuf::from(env_or(
                "STT_MODEL_PATH",
                "models/ggml-large-v3-turbo-q5_0.bin",
            )),
            stt_threads: env::var("STT_THREADS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
            persona: env_or(
                "PERSONA",
                "You are a helpful assistant in a live conversation. Answer in the language of the question.",
            ),
            overlay_title: env_or("OVERLAY_TITLE", "MeetingCopilotOverlay"),
            system_audio: env_or("SYSTEM_AUDIO", "1") != "0",
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
