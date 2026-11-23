use crate::config::Config;
use futures::StreamExt;

fn split_tail(text: &str, keep: usize) -> (&str, &str) {
    let boundary = text
        .char_indices()
        .rev()
        .take(keep)
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0);
    text.split_at(boundary)
}

pub fn language_name(code: &str) -> &'static str {
    match code {
        "en" => "English",
        "ru" => "Russian",
        "uz" => "Uzbek (Latin script)",
        _ => "",
    }
}

pub async fn stream_answer<F>(
    question: &str,
    history: &[String],
    config: &Config,
    language: &str,
    screenshot_png: Option<&[u8]>,
    mut on_token: F,
) -> anyhow::Result<String>
where
    F: FnMut(&str),
{
    let url = format!("{}/chat/completions", config.llm_base);
    let context = history
        .iter()
        .rev()
        .take(12)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let language_instruction = if language.is_empty() || language == "auto" {
        format!(
            "The conversation may be in {}. Always answer in the language of the question.",
            config.conversation_languages
        )
    } else {
        format!(
            "Conduct the conversation in {}. Answer in {} unless the user explicitly switches language.",
            language_name(language),
            language_name(language)
        )
    };
    let system = format!(
        "{}\n\n{}\n\nYou are in a live conversation. Answer the latest question concisely and naturally. Output only the words the user should say.",
        config.persona, language_instruction
    );
    let user = if context.is_empty() {
        question.to_string()
    } else {
        format!("Recent conversation:\n{context}\n\nLatest question: {question}")
    };
    let user_content = match screenshot_png {
        Some(png) => {
            let encoded = crate::screenshot::base64_encode(png);
            serde_json::json!([
                { "type": "text", "text": user },
                { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{encoded}") } }
            ])
        }
        None => serde_json::json!(user),
    };
    let model = if screenshot_png.is_some() {
        &config.vision_model
    } else {
        &config.llm_model
    };
    let body = serde_json::json!({
        "model": model,
        "stream": true,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user_content }
        ]
    });

    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(&config.api_key)
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await?;
        anyhow::bail!("LLM request failed ({status}): {text}");
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut answer = String::new();
    let mut thinking = false;
    let mut tagbuf = String::new();

    while let Some(item) = stream.next().await {
        let item = item?;
        buffer.push_str(&String::from_utf8_lossy(&item));
        while let Some(position) = buffer.find('\n') {
            let line = buffer[..position].trim().to_string();
            buffer = buffer[position + 1..].to_string();
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                if !thinking && !tagbuf.is_empty() {
                    answer.push_str(&tagbuf);
                    on_token(&tagbuf);
                }
                return Ok(answer);
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                let Some(delta) = value["choices"][0]["delta"]["content"].as_str() else {
                    continue;
                };
                let mut rest = std::mem::take(&mut tagbuf);
                rest.push_str(delta);
                let mut emit = String::new();
                loop {
                    if thinking {
                        match rest.find("</think>") {
                            Some(pos) => {
                                rest = rest[pos + 8..].to_string();
                                thinking = false;
                            }
                            None => {
                                let (_kept, held) = split_tail(&rest, 7);
                                tagbuf = held.to_string();
                                break;
                            }
                        }
                    } else {
                        match rest.find("<think>") {
                            Some(pos) => {
                                emit.push_str(&rest[..pos]);
                                rest = rest[pos + 7..].to_string();
                                thinking = true;
                            }
                            None => {
                                let (kept, held) = split_tail(&rest, 6);
                                emit.push_str(kept);
                                tagbuf = held.to_string();
                                break;
                            }
                        }
                    }
                }
                if !emit.is_empty() {
                    answer.push_str(&emit);
                    on_token(&emit);
                }
            }
        }
    }
    if !thinking && !tagbuf.is_empty() {
        answer.push_str(&tagbuf);
        on_token(&tagbuf);
    }
    Ok(answer)
}
