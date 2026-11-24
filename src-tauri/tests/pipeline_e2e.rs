//! Full audio-processing slice, headless: 30 s fixture WAV -> VAD -> whisper ->
//! echo dedup -> SQLite. Proves every stage except the device capture itself.
//! Run explicitly (needs the model and the fixture):
//!   build.ps1 test cuda -- --ignored wav_runs_through

use meetingus_tauri_lib::{asr, audio::AudioChunk, audio::Role, db, session, vad};
use std::path::Path;

/// Minimal RIFF reader: returns the `data` chunk of a 16-bit PCM WAV.
fn read_wav_samples(path: &Path) -> Option<Vec<i16>> {
    let bytes = std::fs::read(path).ok()?;
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = bytes.get(pos + 8..pos + 8 + size)?;
        if id == b"data" {
            data = Some(body);
            break;
        }
        pos += 8 + size + (size % 2); // chunks are word-aligned
    }
    let data = data?;
    Some(
        data.chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect(),
    )
}

#[test]
#[ignore = "needs models/ggml-small.bin, replica/spike/e2e-30s.wav and a whisper backend"]
fn wav_runs_through_vad_asr_dedup_into_sqlite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let model = root.join("models/ggml-small.bin");
    let wav = root.join("replica/spike/e2e-30s.wav");
    if !model.exists() || !wav.exists() {
        eprintln!("SKIP: fixture missing ({} or {})", model.display(), wav.display());
        return;
    }
    let samples = read_wav_samples(&wav).expect("parse fixture wav");
    assert!(samples.len() >= 160_000, "need at least 10 s of audio");

    let conn = db::open(Path::new(":memory:")).expect("open memory db");
    let session_id = db::create_session(&conn, "manual", None).expect("create session");
    let transcriber = asr::Transcriber::new(&model, "en", 6).expect("load model");

    let mut vad = vad::VadProcessor::new();
    let mut memory = session::TranscriptMemory::new(8);
    let mut stored = 0usize;
    let mut texts: Vec<String> = Vec::new();

    // 100 ms chunks, exactly what loopback delivers
    for (i, chunk) in samples.chunks(1600).enumerate() {
        let chunk = AudioChunk {
            role: Role::Them,
            samples: chunk.to_vec(),
        };
        let Some(segment) = vad.push(chunk) else { continue };
        let text = transcriber
            .transcribe(&segment.samples, "en")
            .expect("transcribe segment");
        let at = (i * 100) as i64;
        if session::accept_segment(
            &conn,
            &session_id,
            Role::Them,
            &text,
            &mut memory,
            at,
            at,
            Some("en"),
        )
        .expect("store segment")
        {
            stored += 1;
            texts.push(text);
        }
    }

    assert!(stored >= 1, "expected at least one stored segment, got {stored}");
    let joined = texts.join(" ").to_lowercase();
    assert!(
        joined.contains("quarter"),
        "expected fixture speech in transcript, got: {joined}"
    );
    eprintln!("OK: {stored} segments -> {texts:?}");
}
