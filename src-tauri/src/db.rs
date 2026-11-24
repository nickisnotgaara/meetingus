//! SQLite layer: schema bootstrap (user_version migrations), single-writer
//! session/segment/chat CRUD, FTS5 search over past meetings (D15).
//! All timestamps are UTC epoch milliseconds.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub const SCHEMA: &str = include_str!("../../replica/schema.sql");

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as i64
}

/// Open (creating if needed) the database at `path`, apply pending schema
/// versions. Idempotent: opening twice does not error.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    Ok(conn)
}

/// Start a session. Idempotent: if an `ongoing` session exists, return its id.
pub fn create_session(conn: &Connection, source: &str, detection_name: Option<&str>) -> Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM sessions WHERE status='ongoing' ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sessions (id, title, status, source, detection_name, started_at)
         VALUES (?1, 'Untitled Session', 'ongoing', ?2, ?3, ?4)",
        params![id, source, detection_name, now_ms()],
    )?;
    Ok(id)
}

/// End a session (sets status='ended', ended_at). No-op if already ended.
pub fn end_session(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET status='ended', ended_at=?1, updated_at=?1
         WHERE id=?2 AND status != 'ended'",
        params![now_ms(), id],
    )?;
    Ok(())
}

/// Append a transcript segment; returns its rowid.
pub fn insert_segment(
    conn: &Connection,
    session_id: &str,
    role: &str,
    text: &str,
    start_ms: i64,
    end_ms: i64,
    language: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO transcript_segments (session_id, role, text, start_ms, end_ms, language)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![session_id, role, text, start_ms, end_ms, language],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Most recently started session id (ended sessions still give Assist context).
pub fn latest_session_id(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM sessions ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

/// Read an app setting. The API key never lives here (D6: OS keyring).
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Last `limit` transcript segments of a session, oldest first (context for Assist).
pub fn recent_segments(
    conn: &Connection,
    session_id: &str,
    limit: usize,
) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT role, text FROM (
            SELECT id, role, text FROM transcript_segments
            WHERE session_id=?1 ORDER BY id DESC LIMIT ?2
         ) ORDER BY id ASC",
    )?;
    let rows = stmt
        .query_map(params![session_id, limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<Vec<(String, String)>>>()?;
    Ok(rows)
}

/// Append a chat turn (user request or assistant answer) for history.
pub fn insert_chat_message(
    conn: &Connection,
    session_id: Option<&str>,
    role: &str,
    content: &str,
    thinking: Option<&str>,
    smart: bool,
    action: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO chat_messages
           (session_id, role, content, thinking, smart_mode, action)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![session_id, role, content, thinking, smart as i64, action],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Keyword search over chunked past meetings (FTS5, D15).
/// Words are AND-ed as phrases; a word with no match yields no rows.
pub fn search_chunks(conn: &Connection, query: &str) -> Result<Vec<(String, String)>> {
    let match_expr = query
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    if match_expr.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT c.session_id, c.text FROM chunks_fts f
         JOIN chunks c ON c.id = f.rowid
         WHERE chunks_fts MATCH ?1 LIMIT 20",
    )?;
    let rows = stmt
        .query_map(params![match_expr], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<(String, String)>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        open(Path::new(":memory:")).expect("open :memory:")
    }

    #[test]
    fn open_is_idempotent_on_disk() {
        let path = std::env::temp_dir().join(format!("meetingus-test-{}.db", uuid::Uuid::new_v4()));
        let _a = open(&path).expect("first open migrates");
        let _b = open(&path).expect("second open must not error (user_version guard)");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn session_start_is_idempotent_while_ongoing() {
        let conn = mem();
        let a = create_session(&conn, "manual", None).expect("create");
        let b = create_session(&conn, "manual", None).expect("create again");
        assert_eq!(a, b, "second start returns the ongoing session");
    }

    #[test]
    fn end_session_sets_status_and_timestamp() {
        let conn = mem();
        let id = create_session(&conn, "manual", None).unwrap();
        end_session(&conn, &id).unwrap();
        let (status, ended): (String, Option<i64>) = conn
            .query_row(
                "SELECT status, ended_at FROM sessions WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "ended");
        assert!(ended.is_some(), "ended_at set");
        // ending twice is a no-op, not an error
        end_session(&conn, &id).unwrap();
    }

    #[test]
    fn segments_persist_with_roles_and_offsets() {
        let conn = mem();
        let id = create_session(&conn, "manual", None).unwrap();
        let row = insert_segment(&conn, &id, "me", "hello", 0, 1200, Some("en")).unwrap();
        assert!(row > 0);
        insert_segment(&conn, &id, "them", "hi there", 1300, 2500, Some("en")).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transcript_segments WHERE session_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
        // invalid role rejected by CHECK constraint
        assert!(insert_segment(&conn, &id, "bot", "x", 0, 1, None).is_err());
    }

    #[test]
    fn settings_roundtrip_and_overwrite() {
        let conn = mem();
        assert_eq!(get_setting(&conn, "stt_model").unwrap(), None);
        set_setting(&conn, "stt_model", "base").unwrap();
        set_setting(&conn, "stt_model", "small").unwrap();
        assert_eq!(
            get_setting(&conn, "stt_model").unwrap().as_deref(),
            Some("small")
        );
    }

    #[test]
    fn latest_session_id_survives_session_end() {
        let conn = mem();
        let id = create_session(&conn, "manual", None).unwrap();
        end_session(&conn, &id).unwrap();
        assert_eq!(latest_session_id(&conn).unwrap().as_deref(), Some(id.as_str()));
        assert_eq!(latest_session_id(&mem()).unwrap(), None);
    }

    #[test]
    fn recent_segments_returns_oldest_first_and_respects_limit() {
        let conn = mem();
        let id = create_session(&conn, "manual", None).unwrap();
        for i in 0..5 {
            insert_segment(&conn, &id, "them", &format!("line {i}"), i, i + 1, None).unwrap();
        }
        let rows = recent_segments(&conn, &id, 3).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].1, "line 2", "last 3, still chronological");
        assert_eq!(rows[2].1, "line 4");
    }

    #[test]
    fn chat_messages_persist_with_action_and_flags() {
        let conn = mem();
        let id = create_session(&conn, "manual", None).unwrap();
        insert_chat_message(&conn, Some(&id), "user", "what should I say?", None, true, Some("custom"))
            .unwrap();
        insert_chat_message(&conn, Some(&id), "assistant", "Ask about timing.", Some("think"), true, Some("custom"))
            .unwrap();
        let (role, content, smart, action): (String, String, i64, Option<String>) = conn
            .query_row(
                "SELECT role, content, smart_mode, action FROM chat_messages ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(role, "assistant");
        assert_eq!(content, "Ask about timing.");
        assert_eq!(smart, 1);
        assert_eq!(action.as_deref(), Some("custom"));
    }

    #[test]
    fn fts_search_finds_chunked_meeting_text() {
        let conn = mem();
        let id = create_session(&conn, "manual", None).unwrap();
        conn.execute(
            "INSERT INTO chunks (session_id, chunk_idx, text) VALUES (?1, 0, ?2)",
            rusqlite::params![id, "discussed the budget breakdown for q4"],
        )
        .unwrap();
        let hits = search_chunks(&conn, "budget").expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, id);
        assert!(search_chunks(&conn, "nonexistentword").unwrap().is_empty());
    }

    #[test]
    fn cascades_delete_notes_and_segments() {
        let conn = mem();
        let id = create_session(&conn, "manual", None).unwrap();
        insert_segment(&conn, &id, "me", "x", 0, 1, None).unwrap();
        conn.execute("DELETE FROM sessions WHERE id=?1", [&id]).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM transcript_segments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "segments cascade with session");
    }
}
