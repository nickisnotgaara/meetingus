//! Shared application state: the single DB writer (ub-mode: one writer),
//! the current session id, audio health timestamps for the watchdog, the
//! preloaded whisper model (T12) and the Assist generation counter (cancels
//! stale streams without aborting the HTTP request).

use crate::asr::Transcriber;
use crate::audio::Role;
use crate::db;
use anyhow::Result;
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub current_session: Mutex<Option<String>>,
    pub health: Health,
    /// Shared with capture/pipeline threads: cleared by session_end.
    pub audio_running: Arc<AtomicBool>,
    /// Bumped by every assist_ask; older streams stop emitting.
    pub assist_gen: Arc<AtomicU64>,
    /// Loaded once at startup so session_start is instant (487 MB model).
    pub transcriber: Mutex<Option<Arc<Transcriber>>>,
    pub transcriber_error: Mutex<Option<String>>,
    /// Bumped to stop the onboarding mic-level monitor.
    pub mic_gen: Arc<AtomicU64>,
    /// Set by model_download_cancel; checked by the streaming downloader.
    pub model_cancel: Arc<AtomicBool>,
}

/// Last-seen timestamps (epoch seconds) per audio role; powers the 10s/30s
/// legacy watchdog timings (regression contract).
pub struct Health {
    mic: AtomicU64,
    them: AtomicU64,
}

impl Health {
    pub fn new() -> Self {
        Self {
            mic: AtomicU64::new(0),
            them: AtomicU64::new(0),
        }
    }

    pub fn touch(&self, role: Role, now_secs: u64) {
        let slot = match role {
            Role::Me => &self.mic,
            Role::Them => &self.them,
        };
        slot.store(now_secs, Ordering::Relaxed);
    }

    /// True when this role has been silent longer than `threshold_secs`.
    /// Convention: the pipeline touches BOTH roles at session start, so a
    /// never-fed role (last==0, before any session) is not considered stalled.
    pub fn stalled(&self, role: Role, now_secs: u64, threshold_secs: u64) -> bool {
        let slot = match role {
            Role::Me => &self.mic,
            Role::Them => &self.them,
        };
        let last = slot.load(Ordering::Relaxed);
        last != 0 && now_secs.saturating_sub(last) > threshold_secs
    }
}

impl AppState {
    pub fn new(conn: Connection) -> Result<Self> {
        Ok(Self {
            db: Mutex::new(conn),
            current_session: Mutex::new(None),
            health: Health::new(),
            audio_running: Arc::new(AtomicBool::new(false)),
            assist_gen: Arc::new(AtomicU64::new(0)),
            transcriber: Mutex::new(None),
            transcriber_error: Mutex::new(None),
            mic_gen: Arc::new(AtomicU64::new(0)),
            model_cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Open the on-disk database (app data dir) and bootstrap state.
    pub fn open_at(path: &std::path::Path) -> Result<Self> {
        Self::new(db::open(path)?)
    }

    pub fn transcriber(&self) -> Option<Arc<Transcriber>> {
        self.transcriber.lock().expect("transcriber lock").clone()
    }

    pub fn set_transcriber(&self, t: Arc<Transcriber>) {
        *self.transcriber.lock().expect("transcriber lock") = Some(t);
        *self.transcriber_error.lock().expect("transcriber lock") = None;
    }

    pub fn set_transcriber_error(&self, message: String) {
        *self.transcriber_error.lock().expect("transcriber lock") = Some(message);
    }

    pub fn transcriber_error(&self) -> Option<String> {
        self.transcriber_error.lock().expect("transcriber lock").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_stall_never_fires_before_first_chunk() {
        let h = Health::new();
        assert!(!h.stalled(Role::Me, 1_000, 10), "zero last-seen = not stalled");
    }

    #[test]
    fn health_stall_follows_10s_threshold() {
        let h = Health::new();
        h.touch(Role::Them, 100);
        assert!(!h.stalled(Role::Them, 109, 10), "9s silent is fine");
        assert!(h.stalled(Role::Them, 111, 10), "11s silent exceeds 10s");
        h.touch(Role::Them, 112);
        assert!(!h.stalled(Role::Them, 113, 10), "fresh chunk resets");
    }

    #[test]
    fn health_roles_are_independent() {
        let h = Health::new();
        h.touch(Role::Me, 100);
        h.touch(Role::Them, 100); // convention: both touched at session start
        assert!(h.stalled(Role::Them, 200, 10), "them silent 100s -> stalled");
        assert!(!h.stalled(Role::Me, 105, 10), "me still feeding -> fine");
    }

    #[test]
    fn app_state_opens_db_and_starts_empty() {
        let state = AppState::open_at(std::path::Path::new(":memory:")).expect("open");
        assert!(state.current_session.lock().unwrap().is_none());
        let guard = state.db.lock().unwrap();
        let n: i64 = guard
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .expect("schema applied");
        assert_eq!(n, 0);
    }

    #[test]
    fn transcriber_slot_starts_empty_and_reports_errors() {
        let state = AppState::open_at(std::path::Path::new(":memory:")).unwrap();
        assert!(state.transcriber().is_none());
        assert!(state.transcriber_error().is_none());
        state.set_transcriber_error("boom".into());
        assert_eq!(state.transcriber_error().as_deref(), Some("boom"));
    }
}
