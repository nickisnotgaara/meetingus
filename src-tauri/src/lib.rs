// meetingus — Cluely-clone core. Units: db, audio, vad, asr, llm, dedup,
// state, session. This file wires them: app setup, session commands, the
// capture->VAD->ASR->DB pipeline thread, health watchdog, UI events.

pub mod asr;
pub mod audio;
pub mod db;
pub mod dedup;
pub mod llm;
pub mod session;
pub mod state;
pub mod vad;

use state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

fn model_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("STT_MODEL_PATH") {
        return p.into();
    }
    // dev builds run with cwd=src-tauri; resolve the repo models/ dir.
    let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/ggml-small.bin");
    if dev.exists() {
        return dev;
    }
    std::path::PathBuf::from("models/ggml-small.bin")
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
            let transcriber = match asr::Transcriber::new(&model_path(), "auto", stt_threads()) {
                Ok(t) => t,
                Err(err) => {
                    let _ = app.emit(
                        "app://error",
                        serde_json::json!({ "message": format!("STT model: {err:#}") }),
                    );
                    return;
                }
            };
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
    if state.audio_running.load(Ordering::Relaxed) {
        if let Some(id) = state.current_session.lock().expect("lock").clone() {
            return Ok(id); // idempotent
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
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let state = AppState::open_at(&data_dir.join("meetingus.db"))?;
            app.manage(state);
            spawn_watchdog(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_start,
            session_end,
            session_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
