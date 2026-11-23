-- meetingus (Cluely clone) — local-first schema, SQLite (WAL, single writer)
-- Decisions: D15 (FTS5 for RAG), D6 (API key in OS keyring, NOT in DB), no auth/billing.
-- Timestamps: INTEGER UTC epoch milliseconds (SQLite "timestamptz" convention).

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Sessions (a listening session; manual start or D14 detection)
CREATE TABLE sessions (
  id            TEXT PRIMARY KEY,              -- uuid v4
  title         TEXT NOT NULL DEFAULT 'Untitled Session',
  status        TEXT NOT NULL DEFAULT 'ongoing'
                CHECK (status IN ('ongoing','ended','idle')),
  source        TEXT NOT NULL DEFAULT 'manual'
                CHECK (source IN ('manual','detection')),
  detection_name TEXT,                          -- e.g. "Zoom" when source='detection'
  started_at    INTEGER NOT NULL,               -- epoch ms, UTC
  ended_at      INTEGER,                        -- NULL while ongoing
  created_at    INTEGER NOT NULL DEFAULT (strftime('%s','now')*1000),
  updated_at    INTEGER NOT NULL DEFAULT (strftime('%s','now')*1000)
);
CREATE INDEX idx_sessions_status   ON sessions(status);
CREATE INDEX idx_sessions_started  ON sessions(started_at DESC);

-- Live transcript; role me = mic, them = system loopback (dedup applied upstream, D11)
CREATE TABLE transcript_segments (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  role        TEXT NOT NULL CHECK (role IN ('me','them')),
  text        TEXT NOT NULL,
  start_ms    INTEGER NOT NULL,                 -- offset within session
  end_ms      INTEGER NOT NULL,
  language    TEXT,                             -- detected/selected ISO code
  created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')*1000)
);
CREATE INDEX idx_segments_session ON transcript_segments(session_id, start_ms);

-- AI notes generated on session end (also survives crash: segments are persisted live, F6)
CREATE TABLE meeting_notes (
  session_id   TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
  summary      TEXT NOT NULL DEFAULT '',
  next_steps   TEXT NOT NULL DEFAULT '',
  key_insights TEXT NOT NULL DEFAULT '',
  email_draft  TEXT NOT NULL DEFAULT '',
  manual_notes TEXT NOT NULL DEFAULT '',        -- user's own notes (S02)
  generated_at INTEGER,                         -- NULL = still generating / failed
  updated_at   INTEGER NOT NULL DEFAULT (strftime('%s','now')*1000)
);

-- Chat with AI: during session (session_id set) or about a saved one (set too), or global
CREATE TABLE chat_messages (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id         TEXT REFERENCES sessions(id) ON DELETE CASCADE,
  role               TEXT NOT NULL CHECK (role IN ('user','assistant')),
  content            TEXT NOT NULL,
  thinking           TEXT,                      -- reasoning_content, filtered on render
  screenshot_attached INTEGER NOT NULL DEFAULT 0,
  smart_mode         INTEGER NOT NULL DEFAULT 0,
  action             TEXT,                      -- assist|what_to_say|followup|recap|custom
  created_at         INTEGER NOT NULL DEFAULT (strftime('%s','now')*1000)
);
CREATE INDEX idx_chat_session ON chat_messages(session_id, created_at);

-- Modes (D13/customization): instructions + email template; default mode used by Assist
CREATE TABLE modes (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  instructions   TEXT NOT NULL DEFAULT '',
  email_template TEXT NOT NULL DEFAULT '',
  is_default     INTEGER NOT NULL DEFAULT 0,
  position       INTEGER NOT NULL DEFAULT 0
);

-- Uploaded context files (unlimited locally; original gates at 3 on free plan)
CREATE TABLE files (
  id          TEXT PRIMARY KEY,
  mode_id     TEXT REFERENCES modes(id) ON DELETE SET NULL,
  name        TEXT NOT NULL,
  content     TEXT NOT NULL DEFAULT '',         -- extracted text
  processed   INTEGER NOT NULL DEFAULT 1,
  created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')*1000)
);

-- App settings: key/value (API key lives in OS keyring, D6 — never here)
CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- RAG over past meetings (D15): chunks + FTS5 external-content index
CREATE TABLE chunks (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  chunk_idx  INTEGER NOT NULL,
  text       TEXT NOT NULL
);
CREATE INDEX idx_chunks_session ON chunks(session_id, chunk_idx);

CREATE VIRTUAL TABLE chunks_fts USING fts5(
  text,
  content='chunks',
  content_rowid='id'
);

CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
  INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
  INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
  INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
