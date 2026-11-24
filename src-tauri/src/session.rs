//! Session pipeline core: routing a finished VAD segment through echo
//! dedup (D11) before it reaches the database, with a small per-role ring of
//! recent transcripts for the containment check. Thread/event wiring lives
//! with the Tauri commands; this part is the testable heart.

use crate::audio::Role;
use crate::db;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::VecDeque;

/// Recent transcripts per role; `cap` texts each (8 ≈ enough for a 10s window
/// of back-and-forth at conversational pace).
pub struct TranscriptMemory {
    me: VecDeque<String>,
    them: VecDeque<String>,
    cap: usize,
}

impl TranscriptMemory {
    pub fn new(cap: usize) -> Self {
        Self {
            me: VecDeque::new(),
            them: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    fn slot_mut(&mut self, role: Role) -> &mut VecDeque<String> {
        match role {
            Role::Me => &mut self.me,
            Role::Them => &mut self.them,
        }
    }

    pub fn push(&mut self, role: Role, text: impl Into<String>) {
        let cap = self.cap;
        let slot = self.slot_mut(role);
        slot.push_back(text.into());
        while slot.len() > cap {
            slot.pop_front();
        }
    }

    /// Joined recent texts of the role opposite to `role` (the echo source).
    pub fn recent_other(&self, role: Role) -> String {
        let other = match role {
            Role::Me => &self.them,
            Role::Them => &self.me,
        };
        other.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

/// Store a finished segment unless it is an echo of recent opposite-role
/// speech. Returns true when persisted, false when skipped (echo/empty).
pub fn accept_segment(
    conn: &Connection,
    session_id: &str,
    role: Role,
    text: &str,
    memory: &mut TranscriptMemory,
    start_ms: i64,
    end_ms: i64,
    language: Option<&str>,
) -> Result<bool> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }
    if crate::dedup::is_echo(trimmed, &memory.recent_other(role)) {
        return Ok(false);
    }
    db::insert_segment(conn, session_id, role.as_str(), trimmed, start_ms, end_ms, language)?;
    memory.push(role, trimmed.to_string());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_conn() -> (Connection, String) {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let id = db::create_session(&conn, "manual", None).unwrap();
        (conn, id)
    }

    fn count(conn: &Connection, id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM transcript_segments WHERE session_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn new_speech_is_stored_and_recorded() {
        let (conn, id) = mem_conn();
        let mut mem = TranscriptMemory::new(8);
        let ok = accept_segment(&conn, &id, Role::Me, "hello world", &mut mem, 0, 1000, Some("en"))
            .unwrap();
        assert!(ok, "fresh speech must be stored");
        assert_eq!(count(&conn, &id), 1);
    }

    #[test]
    fn echo_of_recent_other_role_is_dropped() {
        let (conn, id) = mem_conn();
        let mut mem = TranscriptMemory::new(8);
        // they say it (loopback, direct signal)
        accept_segment(&conn, &id, Role::Them, "revenue grew fourteen percent", &mut mem, 0, 1500, Some("en"))
            .unwrap();
        // mic picks the same words up from our speakers -> echo
        let stored = accept_segment(
            &conn,
            &id,
            Role::Me,
            "revenue grew fourteen percent.",
            &mut mem,
            100, 1600, Some("en"),
        )
        .unwrap();
        assert!(!stored, "echo must be dropped");
        assert_eq!(count(&conn, &id), 1, "only the direct copy is stored");
    }

    #[test]
    fn empty_text_is_skipped_without_error() {
        let (conn, id) = mem_conn();
        let mut mem = TranscriptMemory::new(8);
        assert!(!accept_segment(&conn, &id, Role::Me, "   ", &mut mem, 0, 1, None).unwrap());
        assert_eq!(count(&conn, &id), 0);
    }

    #[test]
    fn memory_ring_keeps_cap_and_isolates_roles() {
        let mut mem = TranscriptMemory::new(3);
        for i in 0..5 {
            mem.push(Role::Me, format!("m{i}"));
        }
        let other = mem.recent_other(Role::Them); // me-texts, seen from them
        assert!(other.contains("m4") && other.contains("m3"), "latest kept");
        assert!(!other.contains("m0"), "ring dropped the oldest");
        assert!(mem.recent_other(Role::Me).is_empty(), "them has no texts yet");
    }
}
