//! Assist family: clean-room prompt construction for the four widget actions and
//! free-form questions. Pure text logic, unit-tested without a network.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Assist,
    WhatToSay,
    FollowUp,
    Recap,
    Custom,
}

impl Action {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "assist" => Some(Self::Assist),
            "what_to_say" => Some(Self::WhatToSay),
            "followup" => Some(Self::FollowUp),
            "recap" => Some(Self::Recap),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assist => "assist",
            Self::WhatToSay => "what_to_say",
            Self::FollowUp => "followup",
            Self::Recap => "recap",
            Self::Custom => "custom",
        }
    }
}

/// Persona + guardrails. Short on purpose: flash TTFT is the latency budget.
pub const SYSTEM: &str = "You are MeetingUS, a live copilot that helps the user during real meetings and calls.\n\
You receive a transcript of the conversation (Me = the user's microphone, Them = everyone else) and one task.\n\n\
Rules:\n\
- Reply in the language the conversation is mostly in.\n\
- Be concrete and brief: 1-3 short sentences; use at most 4 one-line bullets for lists.\n\
- No preamble, no restating the task, no markdown headings.\n\
- Never invent facts that are not in the transcript or the task. If something is missing, say what to ask instead.\n\
- When asked what to say, write the exact words the user can read aloud, first person, without quotes.\n\
- Never wrap the whole answer in quotes.\n\
- Stay neutral and professional; do not mention being an AI unless asked.";

fn instruction(action: Action, question: &str) -> String {
    match action {
        Action::Assist => "Help the user right now: answer the open question in the conversation, or give the most useful next point to make. If nothing is pending, say where things stand in one line.".into(),
        Action::WhatToSay => "Write what the user should say next: one or two sentences they can read aloud verbatim.".into(),
        Action::FollowUp => "Give 3 sharp follow-up questions the user could ask next, one per line, numbered 1-3.".into(),
        Action::Recap => "Summarize the conversation for the user: decisions, open questions and next steps. At most 4 bullets.".into(),
        Action::Custom => {
            let q = question.trim();
            if q.is_empty() {
                "Answer the last question in the conversation.".into()
            } else {
                q.to_string()
            }
        }
    }
}

/// Render the transcript as role-labelled lines (oldest first) plus the task.
pub fn build_user_message(action: Action, question: &str, transcript: &[(String, String)]) -> String {
    let mut out = String::new();
    if transcript.is_empty() {
        out.push_str("No transcript yet — this is the start of the conversation.\n\n");
    } else {
        out.push_str("Conversation transcript (oldest first):\n");
        for (role, text) in transcript {
            let who = if role == "me" { "Me" } else { "Them" };
            out.push_str(who);
            out.push_str(": ");
            out.push_str(text.trim());
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("Task: ");
    out.push_str(&instruction(action, question));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Vec<(String, String)> {
        vec![
            ("them".into(), "Can you walk me through the Q4 budget?".into()),
            ("me".into(), "Sure, give me a second.".into()),
        ]
    }

    #[test]
    fn action_roundtrips_wire_names() {
        for a in [
            Action::Assist,
            Action::WhatToSay,
            Action::FollowUp,
            Action::Recap,
            Action::Custom,
        ] {
            assert_eq!(Action::parse(a.as_str()), Some(a));
        }
        assert_eq!(Action::parse("nope"), None);
    }

    #[test]
    fn message_contains_roles_in_order_and_task() {
        let msg = build_user_message(Action::Assist, "", &lines());
        let them = msg.find("Them: Can you walk").expect("them line");
        let me = msg.find("Me: Sure").expect("me line");
        assert!(them < me, "oldest first");
        assert!(msg.contains("Task: Help the user right now"));
    }

    #[test]
    fn empty_transcript_is_explicit_not_blank() {
        let msg = build_user_message(Action::Recap, "", &[]);
        assert!(msg.starts_with("No transcript yet"));
        assert!(msg.contains("Task: Summarize the conversation"));
    }

    #[test]
    fn custom_uses_the_typed_question() {
        let msg = build_user_message(Action::Custom, "What is our churn number?", &lines());
        assert!(msg.trim_end().ends_with("What is our churn number?"));
    }

    #[test]
    fn what_to_say_asks_for_speakable_words() {
        let msg = build_user_message(Action::WhatToSay, "", &[]);
        assert!(msg.contains("read aloud verbatim"));
    }
}
