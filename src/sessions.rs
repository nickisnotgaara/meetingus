use anyhow::Context;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct SessionData {
    pub id: String,
    pub title: String,
    pub started: u64,
    pub finished: u64,
    pub transcript: Vec<(String, String)>,
    pub chat: Vec<(String, String)>,
    pub summary: String,
    pub notes: String,
}

impl SessionData {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "title": self.title,
            "started": self.started,
            "finished": self.finished,
            "transcript": self.transcript,
            "chat": self.chat,
            "summary": self.summary,
            "notes": self.notes,
        })
    }

    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        let pairs = |key: &str| -> Vec<(String, String)> {
            value[key]
                .as_array()
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|item| {
                            Some((item[0].as_str()?.to_string(), item[1].as_str()?.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        Some(Self {
            id: value["id"].as_str()?.to_string(),
            title: value["title"].as_str().unwrap_or("Untitled Session").to_string(),
            started: value["started"].as_u64().unwrap_or(0),
            finished: value["finished"].as_u64().unwrap_or(0),
            transcript: pairs("transcript"),
            chat: pairs("chat"),
            summary: value["summary"].as_str().unwrap_or("").to_string(),
            notes: value["notes"].as_str().unwrap_or("").to_string(),
        })
    }
}

pub fn new_id() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{millis:x}")
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sessions_dir() -> PathBuf {
    PathBuf::from("sessions")
}

pub fn save(session: &SessionData) -> anyhow::Result<()> {
    let dir = sessions_dir();
    std::fs::create_dir_all(&dir).context("create sessions dir")?;
    let path = dir.join(format!("{}.json", session.id));
    std::fs::write(&path, serde_json::to_string_pretty(&session.to_json())?)
        .context("write session file")?;
    Ok(())
}

pub fn load(id: &str) -> anyhow::Result<SessionData> {
    let path = sessions_dir().join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("read session {id}"))?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    SessionData::from_json(&value).ok_or_else(|| anyhow::anyhow!("bad session file {id}"))
}

pub fn list() -> Vec<SessionData> {
    let Ok(entries) = std::fs::read_dir(sessions_dir()) else {
        return Vec::new();
    };
    let mut sessions: Vec<SessionData> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let value: serde_json::Value = serde_json::from_str(&text).ok()?;
            SessionData::from_json(&value)
        })
        .collect();
    sessions.sort_by(|a, b| b.finished.cmp(&a.finished));
    sessions
}
