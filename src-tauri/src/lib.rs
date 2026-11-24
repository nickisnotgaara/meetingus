// meetingus — Cluely-clone core. Units: db, audio, vad, asr, llm, dedup,
// state, session, assist. This file wires them: app setup, session commands,
// the capture->VAD->ASR->DB pipeline thread, Assist streaming, global
// shortcuts, health watchdog, UI events.

pub mod asr;
pub mod assist;
pub mod audio;
pub mod db;
pub mod dedup;
pub mod llm;
pub mod models;
pub mod session;
pub mod state;
pub mod vad;

use assist::Action;
use state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState};

const KEYRING_SERVICE: &str = "meetingus";
const KEYRING_ACCOUNT: &str = "mimo";

fn model_path(app: &AppHandle, state: &AppState) -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("STT_MODEL_PATH") {
        return Some(p.into());
    }
    let root = models_root(app)?;
    if let Some(id) = selected_model(state) {
        if let Some(p) = models::model_file(&root, &id) {
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // dev builds compile with cwd=src-tauri; reuse the repo models/ dir so a
    // checkout works before onboarding has downloaded anything.
    let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../models/ggml-small.bin");
    if dev.exists() {
        return Some(dev);
    }
    let small = models::model_file(&root, "small")?;
    small.is_file().then_some(small)
}

/// Where model files live: an explicit override (dev/tests), else app data.
fn models_root(app: &AppHandle) -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("MEETINGUS_MODELS_DIR") {
        return Some(p.into());
    }
    app.path().app_data_dir().ok()
}

fn selected_model(state: &AppState) -> Option<String> {
    let conn = state.db.lock().ok()?;
    db::get_setting(&conn, "stt_model").ok().flatten()
}

fn stt_threads() -> i32 {
    std::env::var("STT_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn base_url() -> String {
    std::env::var("MIMO_BASE_URL").unwrap_or_else(|_| llm::DEFAULT_BASE_URL.to_string())
}

/// MiMo key: env override (dev/CI) first, then the OS keyring (D6).
fn api_key() -> Result<(String, &'static str), String> {
    if let Ok(key) = std::env::var("MIMO_API_KEY") {
        let key = key.trim().to_string();
        if !key.is_empty() {
            return Ok((key, "env"));
        }
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| format!("keyring: {e}"))?;
    match entry.get_password() {
        Ok(key) if !key.trim().is_empty() => Ok((key.trim().to_string(), "keyring")),
        _ => Err("MiMo API key is not set. Paste it in the app or set MIMO_API_KEY.".into()),
    }
}

/// Load whisper once at startup (487 MB on CUDA) so session_start is instant.
/// T12: model is preloaded; the pipeline waits for this slot.
fn preload_transcriber(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("whisper-preload".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let Some(path) = model_path(&app, &state) else {
                tracing::info!("no STT model yet — onboarding will download one");
                state.set_transcriber_error(
                    "No speech model yet — finish setup to download one.".into(),
                );
                return;
            };
            tracing::info!("loading whisper model at {}", path.display());
            match asr::Transcriber::new(&path, "auto", stt_threads()) {
                Ok(t) => {
                    state.set_transcriber(Arc::new(t));
                    tracing::info!("whisper model ready");
                }
                Err(err) => {
                    let message = format!("STT model: {err:#}");
                    state.set_transcriber_error(message.clone());
                    let _ = app.emit("app://error", serde_json::json!({ "message": message }));
                }
            }
        });
}

/// Capture -> VAD -> ASR -> dedup -> SQLite -> UI events. Runs until every
/// capture sender drops (session_end clears the running flag).
fn spawn_pipeline(app: AppHandle, session_id: String, running: Arc<std::sync::atomic::AtomicBool>) {
    let (tx, rx) = std::sync::mpsc::channel::<audio::AudioChunk>();

    // mic thread owns its cpal Stream (created and dropped on this thread)
    {
        let app = app.clone();
        let running = running.clone();
        let tx = tx.clone();
        let _ = std::thread::Builder::new()
            .name("mic-capture".into())
            .spawn(move || match audio::start_mic(tx) {
                Ok(stream) => {
                    while running.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                    drop(stream);
                }
                Err(err) => {
                    let _ = app.emit(
                        "app://error",
                        serde_json::json!({ "message": format!("microphone: {err:#}") }),
                    );
                }
            });
    }

    // system loopback ("them") exits on send failure or running=false
    if let Err(err) = audio::start_system_loopback(tx, running.clone()) {
        let _ = app.emit(
            "app://error",
            serde_json::json!({ "message": format!("system audio: {err:#}") }),
        );
        return;
    }

    // pipeline thread: VAD -> transcribe -> dedup/store -> emit
    let _ = std::thread::Builder::new()
        .name("transcribe".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            // preloaded at startup; wait for it instead of re-reading 487 MB
            let transcriber = loop {
                if let Some(t) = state.transcriber() {
                    break Some(t);
                }
                if let Some(err) = state.transcriber_error() {
                    let _ = app.emit(
                        "app://error",
                        serde_json::json!({ "message": format!("STT model: {err}") }),
                    );
                    break None;
                }
                if !running.load(Ordering::Relaxed) {
                    break None;
                }
                std::thread::sleep(Duration::from_millis(150));
            };
            let Some(transcriber) = transcriber else { return };
            let mut vad = vad::VadProcessor::new();
            let mut memory = session::TranscriptMemory::new(8);
            let started = Instant::now();

            for chunk in rx {
                state.health.touch(chunk.role, now_secs());
                let Some(segment) = vad.push(chunk) else { continue };
                let end_ms = started.elapsed().as_millis() as i64;
                let duration = (segment.samples.len() as i64 * 1000) / 16_000;
                let start_ms = (end_ms - duration).max(0);
                let text = match transcriber.transcribe(&segment.samples, "auto") {
                    Ok(t) => t,
                    Err(err) => {
                        let _ = app.emit(
                            "app://error",
                            serde_json::json!({ "message": format!("transcribe: {err:#}") }),
                        );
                        continue;
                    }
                };
                let stored = {
                    let conn = state.db.lock().expect("db lock");
                    session::accept_segment(
                        &conn,
                        &session_id,
                        segment.role,
                        &text,
                        &mut memory,
                        start_ms,
                        end_ms,
                        None,
                    )
                };
                match stored {
                    Ok(true) => {
                        let _ = app.emit(
                            "transcript://segment",
                            serde_json::json!({
                                "role": segment.role.as_str(),
                                "text": text,
                                "start_ms": start_ms,
                                "end_ms": end_ms,
                            }),
                        );
                    }
                    Ok(false) => {} // echo dropped (D11)
                    Err(err) => {
                        let _ = app.emit(
                            "app://error",
                            serde_json::json!({ "message": format!("db: {err:#}") }),
                        );
                    }
                }
            }
        });
}

#[tauri::command]
fn session_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    start_session_core(&app, &state)
}

/// Shared by the IPC command and the MEETINGUS_AUTOSTART dev/test flag.
fn start_session_core(app: &AppHandle, state: &AppState) -> Result<String, String> {
    if state.audio_running.load(Ordering::Relaxed) {
        if let Some(id) = state.current_session.lock().expect("lock").clone() {
            return Ok(id); // idempotent
        }
    }
    // No model at all: fail fast instead of waiting forever in the pipeline.
    // Still loading (no error yet) is fine — the pipeline waits for the slot.
    if state.transcriber().is_none() {
        if let Some(err) = state.transcriber_error() {
            return Err(err);
        }
    }
    let id = {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        db::create_session(&conn, "manual", None).map_err(|e| e.to_string())?
    };
    // convention: both roles touched at start (see state::Health)
    state.health.touch(audio::Role::Me, now_secs());
    state.health.touch(audio::Role::Them, now_secs());
    state.audio_running.store(true, Ordering::Relaxed);
    spawn_pipeline(app.clone(), id.clone(), state.audio_running.clone());
    *state.current_session.lock().map_err(|_| "lock poisoned")? = Some(id.clone());
    tracing::info!("session_start id={id}");
    let _ = app.emit(
        "session://state",
        serde_json::json!({ "running": true, "session_id": id }),
    );
    Ok(id)
}

#[tauri::command]
fn session_end(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.audio_running.store(false, Ordering::Relaxed);
    let ended = {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        let current = state.current_session.lock().map_err(|_| "lock poisoned")?.clone();
        match current {
            Some(id) => {
                db::end_session(&conn, &id).map_err(|e| e.to_string())?;
                Some(id)
            }
            None => None,
        }
    };
    *state.current_session.lock().map_err(|_| "lock poisoned")? = None;
    tracing::info!("session_end id={ended:?}");
    let _ = app.emit(
        "session://state",
        serde_json::json!({ "running": false, "session_id": ended }),
    );
    Ok(())
}

#[tauri::command]
fn session_state(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let running = state.audio_running.load(Ordering::Relaxed);
    let session_id = state
        .current_session
        .lock()
        .map_err(|_| "lock poisoned")?
        .clone();
    Ok(serde_json::json!({ "running": running, "session_id": session_id }))
}

#[derive(serde::Serialize)]
struct ApiKeyStatus {
    configured: bool,
    source: Option<&'static str>,
}

#[tauri::command]
fn api_key_status() -> ApiKeyStatus {
    match api_key() {
        Ok((_, source)) => ApiKeyStatus { configured: true, source: Some(source) },
        Err(_) => ApiKeyStatus { configured: false, source: None },
    }
}

#[tauri::command]
fn set_api_key(key: String) -> Result<ApiKeyStatus, String> {
    let key = key.trim();
    if key.len() < 8 {
        return Err("That key looks too short.".into());
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| format!("keyring: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("could not save the key: {e}"))?;
    Ok(ApiKeyStatus { configured: true, source: Some("keyring") })
}

/// One Assist request. Streams deltas on `assist://delta`, reasoning on
/// `assist://thinking`, and finishes with `assist://done` or `assist://error`.
/// `request_id` is generated by the UI so no delta is lost before invoke returns.
#[tauri::command]
fn assist_ask(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    action: String,
    question: Option<String>,
    smart: bool,
) -> Result<serde_json::Value, String> {
    let request_id = request_id.trim().to_string();
    if request_id.is_empty() || request_id.len() > 64 {
        return Err("bad request id".into());
    }
    let action = Action::parse(&action).ok_or_else(|| "unknown action".to_string())?;
    let question = question.unwrap_or_default();
    if action == Action::Custom && question.trim().is_empty() {
        return Err("Type a question first.".into());
    }
    let (key, _source) = api_key()?;

    let session_id = state
        .current_session
        .lock()
        .map_err(|_| "lock poisoned")?
        .clone();
    let (system, user, active_session) = {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        // After End session the id is cleared but the transcript stays on
        // screen; Assist keeps working against the last conversation.
        let active_session = match session_id {
            Some(id) => Some(id),
            None => db::latest_session_id(&conn).map_err(|e| e.to_string())?,
        };
        let transcript = match &active_session {
            Some(id) => db::recent_segments(&conn, id, 40).map_err(|e| e.to_string())?,
            None => Vec::new(),
        };
        if transcript.is_empty() && action != Action::Custom {
            return Err("No conversation yet — start a session or type a question.".into());
        }
        let request_label = if action == Action::Custom {
            question.trim().to_string()
        } else {
            action.as_str().to_string()
        };
        db::insert_chat_message(
            &conn,
            active_session.as_deref(),
            "user",
            &request_label,
            None,
            smart,
            Some(action.as_str()),
        )
        .map_err(|e| e.to_string())?;
        (
            assist::SYSTEM.to_string(),
            assist::build_user_message(action, &question, &transcript),
            active_session,
        )
    };

    // Cancel any previous stream and mark this generation.
    let my_gen = state.assist_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let generation = state.assist_gen.clone();
    let model = if smart { llm::MODEL_PRO } else { llm::MODEL_FLASH };
    let base = base_url();
    let rid = request_id.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let client = llm::MiMoClient::new(base, key);
        let mut text = String::new();
        let mut thinking = String::new();
        let started = Instant::now();

        let generation_emit = generation.clone();
        let app_emit = app_task.clone();
        let rid_emit = rid.clone();
        let result = client
            .stream_chat_opts(model, &system, &user, !smart, |ev| {
                if generation_emit.load(Ordering::SeqCst) != my_gen {
                    return;
                }
                match ev {
                    llm::StreamEvent::Text(t) => {
                        text.push_str(&t);
                        let _ = app_emit.emit(
                            "assist://delta",
                            serde_json::json!({ "request_id": rid_emit, "text": t }),
                        );
                    }
                    llm::StreamEvent::Thinking(t) => {
                        thinking.push_str(&t);
                        let _ = app_emit.emit(
                            "assist://thinking",
                            serde_json::json!({ "request_id": rid_emit, "chars": thinking.chars().count() }),
                        );
                    }
                    llm::StreamEvent::Done => {}
                }
            })
            .await;

        if generation.load(Ordering::SeqCst) != my_gen {
            return; // superseded or cancelled: drop silently
        }
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(()) if !text.trim().is_empty() => {
                let st = app_task.state::<AppState>();
                if let Ok(conn) = st.db.lock() {
                    let _ = db::insert_chat_message(
                        &conn,
                        active_session.as_deref(),
                        "assistant",
                        text.trim(),
                        if thinking.is_empty() { None } else { Some(thinking.as_str()) },
                        smart,
                        Some(action.as_str()),
                    );
                }
                let _ = app_task.emit(
                    "assist://done",
                    serde_json::json!({
                        "request_id": rid,
                        "text": text,
                        "elapsed_ms": elapsed_ms,
                        "model": model,
                    }),
                );
            }
            Ok(()) => {
                let _ = app_task.emit(
                    "assist://error",
                    serde_json::json!({
                        "request_id": rid,
                        "message": "The model returned an empty answer. Try again.",
                    }),
                );
            }
            Err(err) => {
                let _ = app_task.emit(
                    "assist://error",
                    serde_json::json!({
                        "request_id": rid,
                        "message": format!("{err:#}"),
                    }),
                );
            }
        }
    });

    Ok(serde_json::json!({ "request_id": request_id, "model": model }))
}

#[tauri::command]
fn assist_cancel(state: State<'_, AppState>) {
    state.assist_gen.fetch_add(1, Ordering::SeqCst);
}

// ---------------------------------------------------------------- onboarding

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OnboardingStatus {
    needed: bool,
    model_ready: bool,
    model_error: Option<String>,
    key_configured: bool,
}

#[tauri::command]
fn onboarding_status(state: State<'_, AppState>) -> OnboardingStatus {
    let onboarded = {
        let conn = state.db.lock().expect("db lock");
        db::get_setting(&conn, "onboarded").ok().flatten()
    };
    OnboardingStatus {
        needed: onboarded.as_deref() != Some("1"),
        model_ready: state.transcriber().is_some(),
        model_error: state.transcriber_error(),
        key_configured: api_key().is_ok(),
    }
}

#[tauri::command]
fn onboarding_complete(state: State<'_, AppState>) -> Result<(), String> {
    if api_key().is_err() {
        return Err("Add your MiMo API key before finishing setup.".into());
    }
    if state.transcriber().is_none() {
        return Err(state
            .transcriber_error()
            .unwrap_or_else(|| "The speech model is still loading — retry shortly.".into()));
    }
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    db::set_setting(&conn, "onboarded", "1").map_err(|e| e.to_string())
}

#[tauri::command]
fn model_catalog(app: AppHandle, state: State<'_, AppState>) -> Result<models::Catalog, String> {
    let root = models_root(&app).ok_or("no app data dir")?;
    Ok(models::catalog(&root, selected_model(&state)))
}

#[tauri::command]
async fn model_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<u64, String> {
    let root = models_root(&app).ok_or("no app data dir")?;
    let dest = models::model_file(&root, &id).ok_or_else(|| format!("unknown model: {id}"))?;
    if dest.is_file() {
        return Ok(dest.metadata().map(|m| m.len()).unwrap_or(0));
    }
    state.model_cancel.store(false, Ordering::SeqCst);
    models::download(&app, &id, &dest, &state.model_cancel)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn model_download_cancel(state: State<'_, AppState>) {
    state.model_cancel.store(true, Ordering::SeqCst);
}

/// Save the chosen model and load it in the background (no app restart).
#[tauri::command]
fn model_activate(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    if state.audio_running.load(Ordering::Relaxed) {
        return Err("End the session before switching models.".into());
    }
    let root = models_root(&app).ok_or("no app data dir")?;
    let path = models::model_file(&root, &id).ok_or_else(|| format!("unknown model: {id}"))?;
    if !path.is_file() {
        let label = models::spec(&id).map(|s| s.label).unwrap_or(id.as_str());
        return Err(format!("{label} is not downloaded yet."));
    }
    {
        let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
        db::set_setting(&conn, "stt_model", &id).map_err(|e| e.to_string())?;
    }
    let app = app.clone();
    std::thread::Builder::new()
        .name("model-load".into())
        .spawn(move || {
            let st = app.state::<AppState>();
            let _ = app.emit(
                "model://state",
                serde_json::json!({ "status": "loading", "id": id.as_str() }),
            );
            tracing::info!("loading model {id} at {}", path.display());
            match asr::Transcriber::new(&path, "auto", stt_threads()) {
                Ok(t) => {
                    st.set_transcriber(Arc::new(t));
                    let _ = app.emit(
                        "model://state",
                        serde_json::json!({ "status": "ready", "id": id.as_str() }),
                    );
                }
                Err(err) => {
                    let message = format!("STT model: {err:#}");
                    st.set_transcriber_error(message.clone());
                    let _ = app.emit(
                        "model://state",
                        serde_json::json!({ "status": "error", "id": id.as_str(), "message": message }),
                    );
                }
            }
        })
        .map_err(|e| format!("spawn model loader: {e}"))?;
    Ok(())
}

/// Onboarding mic meter: emits `mic://level {level: 0..1}` every ~200 ms while
/// the default microphone captures. A bumped generation stops the monitor.
#[tauri::command]
fn mic_check_start(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let generation = state.mic_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let stop = state.mic_gen.clone();
    std::thread::Builder::new()
        .name("mic-check".into())
        .spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel::<audio::AudioChunk>();
            let _ = app.emit("mic://state", serde_json::json!({ "running": true }));
            let stream = match audio::start_mic(tx) {
                Ok(s) => s,
                Err(err) => {
                    let _ = app.emit(
                        "mic://level",
                        serde_json::json!({ "error": format!("microphone: {err:#}") }),
                    );
                    return;
                }
            };
            let mut buf: Vec<i16> = Vec::new();
            while stop.load(Ordering::SeqCst) == generation {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(chunk) => {
                        buf.extend_from_slice(&chunk.samples);
                        if buf.len() >= 1600 {
                            let _ = app.emit(
                                "mic://level",
                                serde_json::json!({ "level": audio::level(&buf) }),
                            );
                            buf.clear();
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            drop(stream);
            let _ = app.emit("mic://state", serde_json::json!({ "running": false }));
        })
        .map_err(|e| format!("spawn mic check: {e}"))?;
    Ok(())
}

#[tauri::command]
fn mic_check_stop(state: State<'_, AppState>) {
    state.mic_gen.fetch_add(1, Ordering::SeqCst);
}

fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let assist = Shortcut::new(Some(Modifiers::CONTROL), Code::Enter);
    if shortcut == &assist {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = app.emit("assist://hotkey", serde_json::json!({}));
        return;
    }
    let toggle = Shortcut::new(Some(Modifiers::CONTROL), Code::Backslash);
    if shortcut == &toggle {
        if let Some(window) = app.get_webview_window("main") {
            if window.is_visible().unwrap_or(true) {
                let _ = window.hide();
            } else {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
}

/// Legacy watchdog contract: warn when a role is silent >10s during a session.
fn spawn_watchdog(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("audio-watchdog".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            let state = app.state::<AppState>();
            if !state.audio_running.load(Ordering::Relaxed) {
                continue;
            }
            let now = now_secs();
            for (role, name) in [(audio::Role::Me, "mic"), (audio::Role::Them, "system")] {
                if state.health.stalled(role, now, 10) {
                    let _ = app.emit(
                        "audio://health",
                        serde_json::json!({ "role": name, "stalled": true }),
                    );
                }
            }
        });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(on_shortcut)
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let state = AppState::open_at(&data_dir.join("meetingus.db"))?;
            app.manage(state);
            preload_transcriber(app.handle().clone());
            spawn_watchdog(app.handle().clone());

            // Global: Ctrl+Enter = Assist, Ctrl+\ = hide/show the widget.
            let gs = app.global_shortcut();
            for shortcut in [
                Shortcut::new(Some(Modifiers::CONTROL), Code::Enter),
                Shortcut::new(Some(Modifiers::CONTROL), Code::Backslash),
            ] {
                if let Err(err) = gs.register(shortcut) {
                    tracing::warn!("global shortcut not registered: {err}");
                }
            }

            // dev/test seam: start a session without a UI click (E2E checks)
            if std::env::var("MEETINGUS_AUTOSTART").as_deref() == Ok("1") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    let st = h.state::<AppState>();
                    if let Err(e) = start_session_core(&h, &st) {
                        let _ = h.emit(
                            "app://error",
                            serde_json::json!({ "message": format!("autostart: {e}") }),
                        );
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_start,
            session_end,
            session_state,
            assist_ask,
            assist_cancel,
            api_key_status,
            set_api_key,
            onboarding_status,
            onboarding_complete,
            model_catalog,
            model_download,
            model_download_cancel,
            model_activate,
            mic_check_start,
            mic_check_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
