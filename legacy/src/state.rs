use crate::sessions::SessionData;
use std::time::Instant;

#[derive(Clone, Debug)]
pub enum UiCommand {
    Chat(String),
    AnswerLast,
    SetAutoAnswer(bool),
    SetListen(bool),
    SetLanguage(String),
    Screenshot,
    ToggleVisibility,
    MoveWindow(i32, i32),
    CenterWindow,
    Recap,
    FollowUps,
    FinishSession,
    SetView(String),
    OpenSession(String),
    CloseSessionDetail,
    AskAboutSession(String, String),
    SetNotes(String),
}

pub struct UiState {
    pub transcript: Vec<(String, String)>,
    pub chat: Vec<ChatMessage>,
    pub input: String,
    pub status: String,
    pub invisible: bool,
    pub auto_answer: bool,
    pub listen_them: bool,
    pub language: String,
    pub session_idle: bool,
    pub me_voice_at: Option<Instant>,
    pub them_voice_at: Option<Instant>,
    pub screenshot_busy: bool,
    pub view: String,
    pub session_id: String,
    pub session_started: u64,
    pub sessions: Vec<SessionData>,
    pub open_session: Option<SessionData>,
    pub session_input: String,
}

pub struct ChatMessage {
    pub role: String,
    pub text: String,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            transcript: Vec::new(),
            chat: Vec::new(),
            input: String::new(),
            status: "Listening...".to_string(),
            invisible: true,
            auto_answer: true,
            listen_them: true,
            language: "auto".to_string(),
            session_idle: false,
            me_voice_at: None,
            them_voice_at: None,
            screenshot_busy: false,
            view: "chat".to_string(),
            session_id: crate::sessions::new_id(),
            session_started: crate::sessions::now_secs(),
            sessions: crate::sessions::list(),
            open_session: None,
            session_input: String::new(),
        }
    }

    pub fn me_voice_active(&self) -> bool {
        self.me_voice_at
            .is_some_and(|at| at.elapsed().as_millis() < 1200)
    }

    pub fn them_voice_active(&self) -> bool {
        self.them_voice_at
            .is_some_and(|at| at.elapsed().as_millis() < 1200)
    }
}
