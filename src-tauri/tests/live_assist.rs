//! Live MiMo check for the Assist prompts (no UI). Env-gated:
//!   $env:MIMO_API_KEY=...; build.ps1 test cuda -- --ignored live_assist --nocapture

use meetingus_tauri_lib::assist::{self, Action};
use meetingus_tauri_lib::llm::{self, StreamEvent};
use std::time::Instant;

#[tokio::test]
#[ignore = "calls the live MiMo API (needs MIMO_API_KEY)"]
async fn live_assist_streams_answer_from_transcript() {
    let Ok(key) = std::env::var("MIMO_API_KEY") else {
        eprintln!("SKIP: MIMO_API_KEY not set");
        return;
    };
    let transcript = vec![
        ("them".to_string(), "Can you walk me through the Q4 budget?".to_string()),
        ("me".to_string(), "Sure, give me a second.".to_string()),
    ];
    let user = assist::build_user_message(Action::WhatToSay, "", &transcript);
    let client = llm::MiMoClient::new(llm::DEFAULT_BASE_URL, key);
    let started = Instant::now();
    let mut first_token_ms: Option<u128> = None;
    let mut text = String::new();
    client
        .stream_chat_opts(llm::MODEL_FLASH, assist::SYSTEM, &user, true, |ev| {
            if let StreamEvent::Text(t) = ev {
                if first_token_ms.is_none() {
                    first_token_ms = Some(started.elapsed().as_millis());
                }
                text.push_str(&t);
            }
        })
        .await
        .expect("live stream");
    assert!(!text.trim().is_empty(), "expected an answer");
    let ttft = first_token_ms.expect("no text delta measured");
    eprintln!(
        "TTFT={ttft}ms total={}ms answer={text:?}",
        started.elapsed().as_millis()
    );
    assert!(ttft < 5000, "D16: first token p95 budget is 5s, got {ttft}ms");
}
