use std::time::Instant;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: cpu-bench <model.gguf> <wav> [threads]");
        std::process::exit(2);
    }
    let threads: i32 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(6);

    let wav = std::fs::read(&args[2])?;
    let data_off = wav
        .windows(4)
        .position(|w| w == b"data")
        .ok_or("no data chunk in wav")?
        + 8;
    let samples: Vec<f32> = wav[data_off..]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect();
    let audio_secs = samples.len() as f64 / 16000.0;

    let t = Instant::now();
    let ctx = WhisperContext::new_with_params(&args[1], WhisperContextParameters::default())?;
    let load_ms = t.elapsed().as_millis();
    let mut state = ctx.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads);
    params.set_language(Some("en"));
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    let t = Instant::now();
    state.full(params, &samples)?;
    let decode_s = t.elapsed().as_secs_f64();

    println!(
        "model={} threads={} audio_s={:.1} load_ms={} decode_s={:.3} RTFx={:.1}",
        args[1],
        threads,
        audio_secs,
        load_ms,
        decode_s,
        audio_secs / decode_s
    );
    Ok(())
}
