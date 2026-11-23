mod audio;
mod config;
mod overlay;
mod pipeline;
mod screenshot;
mod sessions;
mod state;
mod stealth;
mod stt;
mod vad;
mod wav;

use audio::{start_mic, start_system_loopback, Role};
use config::Config;
use state::{ChatMessage, UiCommand, UiState};
use std::io::BufRead;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use stt::Transcriber;
use tokio::sync::mpsc;
use vad::{SpeechSegment, VadProcessor};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let _ = dotenvy::dotenv();
    let config = Config::from_env()?;

    tracing::info!(
        "loading whisper model: {}",
        config.stt_model_path.display()
    );
    let transcriber = Arc::new(Transcriber::new(
        &config.stt_model_path,
        &config.language,
        config.stt_threads,
    )?);
    tracing::info!("whisper model loaded");

    if let Some(position) = std::env::args().position(|arg| arg == "--transcribe-wav") {
        let path = std::env::args()
            .nth(position + 1)
            .ok_or_else(|| anyhow::anyhow!("usage: --transcribe-wav <file.wav>"))?;
        let wav = wav::read_pcm16(std::path::Path::new(&path))?;
        let channels = wav.channels as usize;
        let mut resampler = audio::Resampler::new(wav.sample_rate);
        let mut mono = Vec::with_capacity(wav.samples.len() / channels + 8);
        for frame in wav.samples.chunks(channels) {
            let sum: i32 = frame.iter().map(|&sample| sample as i32).sum();
            let value = (sum / channels as i32) as i16;
            if let Some(out) = resampler.push(value) {
                mono.push(out);
            }
        }
        tracing::info!(
            "wav: {} Hz, {} ch, {} samples -> {} samples @16k",
            wav.sample_rate,
            wav.channels,
            wav.samples.len(),
            mono.len()
        );
        let started = Instant::now();
        let text = transcriber.transcribe(&mono, &config.language)?;
        println!("{text}");
        eprintln!("elapsed: {:?}", started.elapsed());
        return Ok(());
    }

    let state = Arc::new(Mutex::new(UiState::new()));
    load_persisted(&state);
    let (audio_tx, audio_rx) = std::sync::mpsc::channel();
    let (segment_tx, mut segment_rx) = mpsc::unbounded_channel::<SpeechSegment>();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<UiCommand>();

    let health = Arc::new(Mutex::new(Health::default()));
    let _mic_stream = start_mic(audio_tx.clone())?;
    if config.system_audio {
        start_system_loopback(audio_tx.clone())?;
    }

    let vad_state = state.clone();
    let vad_health = health.clone();
    let _vad_thread = std::thread::Builder::new()
        .name("vad".to_string())
        .spawn(move || {
            let mut vad = VadProcessor::new();
            while let Ok(chunk) = audio_rx.recv() {
                let role = chunk.role;
                {
                    let mut h = vad_health.lock().unwrap();
                    match role {
                        Role::Me => h.me_last = Some(Instant::now()),
                        Role::Them => h.them_last = Some(Instant::now()),
                    }
                }
                if let Some(segment) = vad.push(chunk) {
                    if segment_tx.send(segment).is_err() {
                        break;
                    }
                }
                if vad.is_active(role) {
                    let mut ui = vad_state.lock().unwrap();
                    match role {
                        Role::Me => ui.me_voice_at = Some(Instant::now()),
                        Role::Them => ui.them_voice_at = Some(Instant::now()),
                    }
                }
            }
        })?;

    let last_activity = Arc::new(Mutex::new(Instant::now()));

    let processor = tokio::spawn({
        let config = config.clone();
        let state = state.clone();
        let transcriber = transcriber.clone();
        let last_activity = last_activity.clone();
        async move {
            loop {
                tokio::select! {
                    incoming = segment_rx.recv() => {
                        let Some(segment) = incoming else { break };
                        handle_segment(segment, &config, &state, &transcriber, &last_activity).await;
                    }
                    incoming = command_rx.recv() => {
                        let Some(command) = incoming else { break };
                        handle_command(command, &config, &state, &last_activity).await;
                    }
                }
            }
        }
    });

    tokio::spawn(watchdog(
        state.clone(),
        health,
        last_activity,
        config.overlay_title.clone(),
    ));

    let control_title = config.overlay_title.clone();
    let control_state = state.clone();
    let control_commands = command_tx.clone();
    let control = tokio::task::spawn_blocking(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line.unwrap_or_default();
            let trimmed = line.trim();
            match trimmed {
                "h" => {
                    let _ = control_commands.send(UiCommand::ToggleVisibility);
                }
                "q" => std::process::exit(0),
                "a" => {
                    let _ = control_commands.send(UiCommand::AnswerLast);
                }
                "s" => {
                    let _ = control_commands.send(UiCommand::Screenshot);
                }
                "l" => {
                    let listen = !control_state.lock().unwrap().listen_them;
                    let _ = control_commands.send(UiCommand::SetListen(listen));
                }
                "f" => {
                    let _ = control_commands.send(UiCommand::FinishSession);
                }
                "r" => {
                    let _ = control_commands.send(UiCommand::Recap);
                }
                "u" => {
                    let _ = control_commands.send(UiCommand::FollowUps);
                }
                "c" => {
                    let _ = control_commands.send(UiCommand::CenterWindow);
                }
                "" => {}
                other => {
                    let _ = control_commands.send(UiCommand::Chat(other.to_string()));
                }
            }
        }
        let _ = &control_title;
    });

    spawn_win32_events(command_tx.clone(), state.clone());

    let startup_title = config.overlay_title.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let invisible = true;
        stealth::ensure_visible(&startup_title);
        stealth::apply(&startup_title, invisible);
    });

    let title = config.overlay_title.clone();
    if let Err(error) = overlay::run(title, state.clone(), command_tx.clone()) {
        anyhow::bail!("overlay failed: {error:?}");
    }

    processor.abort();
    control.abort();
    Ok(())
}

struct Health {
    me_last: Option<Instant>,
    them_last: Option<Instant>,
    start: Instant,
    no_audio_warned: bool,
    stall_warned: bool,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            me_last: None,
            them_last: None,
            start: Instant::now(),
            no_audio_warned: false,
            stall_warned: false,
        }
    }
}

async fn watchdog(
    state: Arc<Mutex<UiState>>,
    health: Arc<Mutex<Health>>,
    last_activity: Arc<Mutex<Instant>>,
    overlay_title: String,
) {
    const SESSION_IDLE: std::time::Duration = std::time::Duration::from_secs(600);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        stealth::ensure_visible(&overlay_title);
        {
            let mut h = health.lock().unwrap();
            if !h.no_audio_warned && h.start.elapsed() > std::time::Duration::from_secs(10) && h.me_last.is_none() && h.them_last.is_none() {
                h.no_audio_warned = true;
                tracing::warn!("no audio chunks after 10s - check microphone permissions");
            }
            let stale = |last: Option<Instant>| {
                last.is_some_and(|at| at.elapsed() > std::time::Duration::from_secs(30))
            };
            if stale(h.me_last) || stale(h.them_last) {
                if !h.stall_warned {
                    h.stall_warned = true;
                    tracing::warn!("audio stream stalled (>30s without chunks)");
                }
            } else if h.me_last.is_some() || h.them_last.is_some() {
                h.stall_warned = false;
            }
        }
        let idle_for = last_activity.lock().unwrap().elapsed();
        if idle_for > SESSION_IDLE {
            let mut ui = state.lock().unwrap();
            if !ui.session_idle {
                ui.session_idle = true;
                ui.status = "Session idle — say something or type to continue".to_string();
                tracing::info!("session idle for 10 minutes");
            }
        }
    }
}

async fn handle_segment(
    segment: SpeechSegment,
    config: &Config,
    state: &Arc<Mutex<UiState>>,
    transcriber: &Arc<Transcriber>,
    last_activity: &Arc<Mutex<Instant>>,
) {
    if segment.role == Role::Them && !state.lock().unwrap().listen_them {
        return;
    }
    *last_activity.lock().unwrap() = Instant::now();
    {
        let mut ui = state.lock().unwrap();
        ui.session_idle = false;
    }
    let role_name = if segment.role == Role::Me { "You" } else { "Them" };
    let samples = segment.samples.clone();
    let language = state.lock().unwrap().language.clone();
    let transcriber = transcriber.clone();
    let text = match tokio::task::spawn_blocking(move || transcriber.transcribe(&samples, &language))
        .await
    {
        Ok(Ok(text)) => text,
        Ok(Err(error)) => {
            tracing::warn!("transcription failed: {error:#}");
            return;
        }
        Err(error) => {
            tracing::warn!("transcription task failed: {error}");
            return;
        }
    };
    if text.is_empty() {
        return;
    }
    tracing::info!("[{role_name}] {text}");
    {
        let mut ui = state.lock().unwrap();
        ui.transcript.push((role_name.to_string(), text.clone()));
        if ui.transcript.len() > 200 {
            ui.transcript.remove(0);
        }
    }
    let auto_answer = state.lock().unwrap().auto_answer;
    if segment.role == Role::Them && auto_answer {
        let history = transcript_history(state);
        answer_into_chat(text, history, config, state, last_activity, None).await;
    }
}

async fn handle_command(
    command: UiCommand,
    config: &Config,
    state: &Arc<Mutex<UiState>>,
    last_activity: &Arc<Mutex<Instant>>,
) {
    *last_activity.lock().unwrap() = Instant::now();
    {
        let mut ui = state.lock().unwrap();
        ui.session_idle = false;
    }
    match command {
        UiCommand::Chat(text) => {
            let history = chat_history(state);
            state.lock().unwrap().chat.push(ChatMessage {
                role: "You".to_string(),
                text: text.clone(),
            });
            answer_into_chat(text, history, config, state, last_activity, None).await;
        }
        UiCommand::AnswerLast => {
            let question = state
                .lock()
                .unwrap()
                .transcript
                .iter()
                .rev()
                .find(|(role, _)| role == "Them")
                .map(|(_, text)| text.clone());
            if let Some(question) = question {
                let history = transcript_history(state);
                answer_into_chat(question, history, config, state, last_activity, None).await;
            }
        }
        UiCommand::SetAutoAnswer(value) => {
            state.lock().unwrap().auto_answer = value;
            save_persisted(state);
        }
        UiCommand::SetListen(value) => {
            {
                let mut ui = state.lock().unwrap();
                ui.listen_them = value;
                ui.status = if value {
                    "Listening...".to_string()
                } else {
                    "Interlocutor muted".to_string()
                };
            }
            save_persisted(state);
            tracing::info!("listen interlocutor: {value}");
        }
        UiCommand::SetLanguage(value) => {
            {
                let mut ui = state.lock().unwrap();
                ui.language = value.clone();
                ui.status = format!(
                    "Language: {}",
                    if value == "auto" {
                        "auto-detect".to_string()
                    } else {
                        pipeline::language_name(&value).to_string()
                    }
                );
            }
            save_persisted(state);
            tracing::info!("language set: {value}");
        }
        UiCommand::ToggleVisibility => {
            let invisible = {
                let mut ui = state.lock().unwrap();
                ui.invisible = !ui.invisible;
                ui.invisible
            };
            stealth::apply(&config.overlay_title, invisible);
            if !invisible {
                stealth::ensure_visible(&config.overlay_title);
                stealth::bring_to_front(&config.overlay_title);
            }
            save_persisted(state);
        }
        UiCommand::CenterWindow => {
            stealth::center(&config.overlay_title);
            stealth::bring_to_front(&config.overlay_title);
        }
        UiCommand::MoveWindow(dx, dy) => {
            tracing::info!("move window by ({dx}, {dy})");
            stealth::move_by(&config.overlay_title, dx, dy);
        }
        UiCommand::Recap => {
            let history = transcript_history(state);
            if history.is_empty() {
                state.lock().unwrap().status = "Nothing to recap yet".to_string();
                return;
            }
            let question = "Give a brief recap of the conversation so far in 3-5 sentences.";
            answer_into_chat(question.to_string(), history, config, state, last_activity, None).await;
        }
        UiCommand::FollowUps => {
            let history = transcript_history(state);
            if history.is_empty() {
                state.lock().unwrap().status = "No conversation yet".to_string();
                return;
            }
            let question = "Suggest 3 short follow-up questions the user could ask next in this conversation. Reply with just the questions, one per line.";
            answer_into_chat(question.to_string(), history, config, state, last_activity, None).await;
        }
        UiCommand::FinishSession => {
            finish_session(config, state, last_activity).await;
        }
        UiCommand::SetView(view) => {
            let mut ui = state.lock().unwrap();
            ui.view = view;
            if ui.view != "detail" {
                ui.open_session = None;
            }
        }
        UiCommand::OpenSession(id) => {
            let mut ui = state.lock().unwrap();
            match sessions::load(&id) {
                Ok(session) => {
                    ui.open_session = Some(session);
                    ui.view = "detail".to_string();
                }
                Err(error) => {
                    tracing::warn!("failed to open session {id}: {error:#}");
                    ui.status = format!("Failed to open session: {error:#}");
                }
            }
        }
        UiCommand::CloseSessionDetail => {
            let mut ui = state.lock().unwrap();
            ui.open_session = None;
            ui.view = "sessions".to_string();
        }
        UiCommand::AskAboutSession(id, question) => {
            let Ok(session) = sessions::load(&id) else {
                return;
            };
            let mut history: Vec<String> = session
                .transcript
                .iter()
                .map(|(role, text)| format!("{role}: {text}"))
                .collect();
            if !session.summary.is_empty() {
                history.push(format!("Session summary: {}", session.summary));
            }
            {
                let mut ui = state.lock().unwrap();
                ui.view = "chat".to_string();
                ui.open_session = None;
                ui.chat.push(ChatMessage {
                    role: "You".to_string(),
                    text: question.clone(),
                });
            }
            answer_into_chat(question, history, config, state, last_activity, None).await;
        }
        UiCommand::SetNotes(text) => {
            let mut ui = state.lock().unwrap();
            if let Some(session) = ui.open_session.as_mut() {
                session.notes = text;
                if let Err(error) = sessions::save(session) {
                    tracing::warn!("failed to save notes: {error:#}");
                }
            }
        }
        UiCommand::Screenshot => {
            {
                let mut ui = state.lock().unwrap();
                if ui.screenshot_busy {
                    return;
                }
                ui.screenshot_busy = true;
                ui.status = "Capturing screen...".to_string();
            }
            stealth::apply(&config.overlay_title, true);
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let captured = tokio::task::spawn_blocking(screenshot::capture_screen_png).await;
            let restore_invisible = state.lock().unwrap().invisible;
            stealth::apply(&config.overlay_title, restore_invisible);
            state.lock().unwrap().screenshot_busy = false;
            match captured {
                Ok(Ok((_width, _height, png))) => {
                    tracing::info!("screenshot captured ({} bytes png)", png.len());
                    let question = state
                        .lock()
                        .unwrap()
                        .transcript
                        .iter()
                        .rev()
                        .find(|(role, _)| role == "Them")
                        .map(|(_, text)| text.clone())
                        .unwrap_or_else(|| {
                            "What is shown on my screen? Help me with it.".to_string()
                        });
                    let history = transcript_history(state);
                    answer_into_chat(question, history, config, state, last_activity, Some(png))
                        .await;
                }
                Ok(Err(error)) => {
                    tracing::error!("screenshot failed: {error:#}");
                    push_error_chat(state, &format!("screenshot failed: {error:#}"));
                }
                Err(error) => {
                    tracing::error!("screenshot task failed: {error}");
                    push_error_chat(state, &format!("screenshot failed: {error}"));
                }
            }
        }
    }
}

fn push_error_chat(state: &Arc<Mutex<UiState>>, message: &str) {
    if let Ok(mut ui) = state.lock() {
        ui.chat.push(ChatMessage {
            role: "AI".to_string(),
            text: format!("[error] {message}"),
        });
    }
}

async fn finish_session(
    config: &Config,
    state: &Arc<Mutex<UiState>>,
    last_activity: &Arc<Mutex<Instant>>,
) {
    let (id, started, transcript, chat) = {
        let mut ui = state.lock().unwrap();
        if ui.transcript.is_empty() && ui.chat.is_empty() {
            ui.status = "Nothing to save yet".to_string();
            return;
        }
        ui.status = "Generating notes...".to_string();
        (
            ui.session_id.clone(),
            ui.session_started,
            ui.transcript.clone(),
            ui.chat
                .iter()
                .map(|message| (message.role.clone(), message.text.clone()))
                .collect::<Vec<_>>(),
        )
    };
    let history: Vec<String> = transcript
        .iter()
        .map(|(role, text)| format!("{role}: {text}"))
        .collect();
    let question = "Create meeting notes in markdown with sections: Summary, Key points, Decisions, Action items. Be concise.";
    let summary = answer_into_chat(
        question.to_string(),
        history,
        config,
        state,
        last_activity,
        None,
    )
    .await
    .unwrap_or_default();
    let title = transcript
        .iter()
        .find(|(role, _)| role == "Them")
        .or_else(|| transcript.first())
        .or_else(|| chat.iter().find(|(role, _)| role == "You"))
        .or_else(|| chat.first())
        .map(|(_, text)| text.chars().take(60).collect::<String>())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "Untitled Session".to_string());
    let session = sessions::SessionData {
        id: id.clone(),
        title: title.clone(),
        started,
        finished: sessions::now_secs(),
        transcript,
        chat,
        summary,
        notes: String::new(),
    };
    match sessions::save(&session) {
        Ok(()) => {
            tracing::info!("session saved: {id} ({title})");
            let mut ui = state.lock().unwrap();
            ui.sessions = sessions::list();
            ui.transcript.clear();
            ui.session_id = sessions::new_id();
            ui.session_started = sessions::now_secs();
            ui.chat.push(ChatMessage {
                role: "System".to_string(),
                text: format!("Session saved: {title} (sessions tab)"),
            });
            ui.status = "Session saved. Listening...".to_string();
        }
        Err(error) => {
            tracing::error!("failed to save session: {error:#}");
            push_error_chat(state, &format!("failed to save session: {error:#}"));
        }
    }
}

async fn answer_into_chat(
    question: String,
    history: Vec<String>,
    config: &Config,
    state: &Arc<Mutex<UiState>>,
    last_activity: &Arc<Mutex<Instant>>,
    screenshot_png: Option<Vec<u8>>,
) -> Option<String> {
    let index = {
        let mut ui = state.lock().unwrap();
        ui.chat.push(ChatMessage {
            role: "AI".to_string(),
            text: String::new(),
        });
        ui.status = "Thinking...".to_string();
        ui.chat.len() - 1
    };
    let language = state.lock().unwrap().language.clone();
    let tokens_state = state.clone();
    let generated = pipeline::stream_answer(
        &question,
        &history,
        config,
        &language,
        screenshot_png.as_deref(),
        move |token| {
            if let Ok(mut ui) = tokens_state.lock() {
                if let Some(message) = ui.chat.get_mut(index) {
                    message.text.push_str(token);
                }
            }
        },
    )
    .await;
    match generated {
        Ok(answer) => {
            tracing::info!("[Answer] {answer}");
            *last_activity.lock().unwrap() = Instant::now();
            if let Ok(mut ui) = state.lock() {
                ui.status = if ui.listen_them {
                    "Listening...".to_string()
                } else {
                    "Interlocutor muted".to_string()
                };
            }
            return Some(answer);
        }
        Err(error) => {
            tracing::error!("answer generation failed: {error:#}");
            let mut message = format!("[error] {error:#}");
            if screenshot_png.is_some() && error.to_string().contains("ultimodal") {
                message.push_str(
                    "\n[hint] Screenshots need a vision model: ollama pull qwen2.5vl:3b (LLM_VISION_MODEL=qwen2.5vl:3b)",
                );
            }
            if let Ok(mut ui) = state.lock() {
                if let Some(chat) = ui.chat.get_mut(index) {
                    chat.text = message;
                }
            }
        }
    }
    *last_activity.lock().unwrap() = Instant::now();
    if let Ok(mut ui) = state.lock() {
        ui.status = if ui.listen_them {
            "Listening...".to_string()
        } else {
            "Interlocutor muted".to_string()
        };
    }
    None
}

fn transcript_history(state: &Arc<Mutex<UiState>>) -> Vec<String> {
    state
        .lock()
        .unwrap()
        .transcript
        .iter()
        .map(|(_, text)| text.clone())
        .collect()
}

fn chat_history(state: &Arc<Mutex<UiState>>) -> Vec<String> {
    state
        .lock()
        .unwrap()
        .chat
        .iter()
        .map(|message| format!("{}: {}", message.role, message.text))
        .collect()
}

fn load_persisted(state: &Arc<Mutex<UiState>>) {
    let Ok(text) = std::fs::read_to_string("shared-state.json") else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let mut ui = state.lock().unwrap();
    if let Some(flag) = value["invisible"].as_bool() {
        ui.invisible = flag;
    }
    if let Some(flag) = value["auto_answer"].as_bool() {
        ui.auto_answer = flag;
    }
    if let Some(flag) = value["listen_them"].as_bool() {
        ui.listen_them = flag;
    }
    if let Some(language) = value["language"].as_str() {
        ui.language = language.to_string();
    }
    tracing::info!(
        "persisted state loaded: invisible={}, auto_answer={}, listen_them={}, language={}",
        ui.invisible,
        ui.auto_answer,
        ui.listen_them,
        ui.language
    );
}

fn save_persisted(state: &Arc<Mutex<UiState>>) {
    let ui = state.lock().unwrap();
    let value = serde_json::json!({
        "invisible": ui.invisible,
        "auto_answer": ui.auto_answer,
        "listen_them": ui.listen_them,
        "language": ui.language,
    });
    if let Err(error) = std::fs::write("shared-state.json", value.to_string()) {
        tracing::warn!("failed to persist state: {error}");
    }
}

fn spawn_win32_events(commands: mpsc::UnboundedSender<UiCommand>, state: Arc<Mutex<UiState>>) {
    std::thread::Builder::new()
        .name("win32-events".to_string())
        .spawn(move || {
            use global_hotkey::hotkey::{Code, HotKey, Modifiers};
            use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
            use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem};
            use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
            };

            let manager = match GlobalHotKeyManager::new() {
                Ok(manager) => manager,
                Err(error) => {
                    tracing::warn!("global hotkeys unavailable: {error}");
                    return;
                }
            };
            let bindings = [
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyH),
                    UiCommand::ToggleVisibility,
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyA),
                    UiCommand::AnswerLast,
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyS),
                    UiCommand::Screenshot,
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyL),
                    UiCommand::SetListen(false),
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyC),
                    UiCommand::CenterWindow,
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowUp),
                    UiCommand::MoveWindow(0, -80),
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowDown),
                    UiCommand::MoveWindow(0, 80),
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowLeft),
                    UiCommand::MoveWindow(-80, 0),
                ),
                (
                    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowRight),
                    UiCommand::MoveWindow(80, 0),
                ),
            ];
            let mut registered: Vec<(HotKey, UiCommand)> = Vec::new();
            for (hotkey, command) in bindings {
                match manager.register(hotkey) {
                    Ok(()) => registered.push((hotkey, command)),
                    Err(error) => {
                        tracing::warn!("hotkey {hotkey:?} not registered: {error}");
                    }
                }
            }
            tracing::info!(
                "registered {} global hotkeys (Ctrl+Alt+H/A/S/L/Arrows)",
                registered.len()
            );

            let menu = Menu::new();
            let toggle_item = MenuItem::new("Show / Hide", true, None);
            let center_item = MenuItem::new("Center window", true, None);
            let listen_item = MenuItem::new("Toggle listening", true, None);
            let shot_item = MenuItem::new("Screenshot", true, None);
            let quit_item = MenuItem::new("Quit", true, None);
            let items: [&dyn IsMenuItem; 5] = [
                &toggle_item,
                &center_item,
                &listen_item,
                &shot_item,
                &quit_item,
            ];
            for item in items {
                if let Err(error) = menu.append(item) {
                    tracing::warn!("tray menu append failed: {error}");
                }
            }
            let mut rgba = vec![0u8; 16 * 16 * 4];
            for y in 0..16u32 {
                for x in 0..16u32 {
                    let dx = x as f32 - 7.5;
                    let dy = y as f32 - 7.5;
                    if (dx * dx + dy * dy).sqrt() < 7.0 {
                        let i = ((y * 16 + x) * 4) as usize;
                        rgba[i] = 46;
                        rgba[i + 1] = 160;
                        rgba[i + 2] = 67;
                        rgba[i + 3] = 255;
                    }
                }
            }
            let icon = Icon::from_rgba(rgba, 16, 16).expect("tray icon");
            let _tray = match TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Meeting Copilot")
                .with_icon(icon)
                .build()
            {
                Ok(tray) => tray,
                Err(error) => {
                    tracing::warn!("tray icon unavailable: {error}");
                    return;
                }
            };
            tracing::info!("tray icon ready");

            let menu_receiver = MenuEvent::receiver();
            let tray_receiver = TrayIconEvent::receiver();
            let hotkey_receiver = GlobalHotKeyEvent::receiver();
            loop {
                let mut message = MSG::default();
                unsafe {
                    while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                while let Ok(event) = hotkey_receiver.try_recv() {
                    if event.state != global_hotkey::HotKeyState::Pressed {
                        continue;
                    }
                    for (hotkey, command) in &registered {
                        if hotkey.id() == event.id {
                            let _ = commands.send(command.clone());
                            break;
                        }
                    }
                }
                while let Ok(event) = menu_receiver.try_recv() {
                    if event.id == toggle_item.id() {
                        let _ = commands.send(UiCommand::ToggleVisibility);
                    } else if event.id == center_item.id() {
                        let _ = commands.send(UiCommand::CenterWindow);
                    } else if event.id == listen_item.id() {
                        let listen = !state.lock().unwrap().listen_them;
                        let _ = commands.send(UiCommand::SetListen(listen));
                    } else if event.id == shot_item.id() {
                        let _ = commands.send(UiCommand::Screenshot);
                    } else if event.id == quit_item.id() {
                        std::process::exit(0);
                    }
                }
                while let Ok(event) = tray_receiver.try_recv() {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = commands.send(UiCommand::ToggleVisibility);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
        .expect("failed to spawn win32 events thread");
}
