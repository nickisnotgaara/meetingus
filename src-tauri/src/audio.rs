//! Audio capture: microphone (cpal) + system loopback (WASAPI, "them" role),
//! both normalized to 16 kHz mono. Ported from legacy/src/audio.rs — behavior
//! is a regression contract (watchdog timings live in session code).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Me,
    Them,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Me => "me",
            Role::Them => "them",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub role: Role,
    pub samples: Vec<i16>,
}

trait ToI16 {
    fn to_i16_sample(self) -> i16;
}

impl ToI16 for f32 {
    fn to_i16_sample(self) -> i16 {
        (self.clamp(-1.0, 1.0) * 32767.0) as i16
    }
}

impl ToI16 for i16 {
    fn to_i16_sample(self) -> i16 {
        self
    }
}

impl ToI16 for u16 {
    fn to_i16_sample(self) -> i16 {
        (self as i32 - 32768) as i16
    }
}

/// Linear-interpolation resampler to 16 kHz (whisper input rate).
pub struct Resampler {
    step: f64,
    next_out: f64,
    previous: i16,
    produced: u64,
}

impl Resampler {
    pub fn new(source_rate: u32) -> Self {
        Self {
            step: source_rate as f64 / 16000.0,
            next_out: 0.0,
            previous: 0,
            produced: 0,
        }
    }

    pub fn push(&mut self, sample: i16) -> Option<i16> {
        let position = self.produced as f64;
        self.produced += 1;
        let mut result = None;
        while self.next_out <= position {
            let fraction = (self.next_out - (position - 1.0)).clamp(0.0, 1.0);
            let value = self.previous as f64 * (1.0 - fraction) + sample as f64 * fraction;
            result = Some(value as i16);
            self.next_out += self.step;
        }
        self.previous = sample;
        result
    }
}

fn build_mic_stream<T: cpal::Sample + cpal::SizedSample + ToI16>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: Sender<AudioChunk>,
    channels: usize,
    rate: u32,
) -> anyhow::Result<cpal::Stream> {
    let mut resampler = Resampler::new(rate);
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut mono = Vec::with_capacity(data.len() / channels + 8);
            for frame in data.chunks(channels) {
                let sum: i32 = frame.iter().map(|&sample| sample.to_i16_sample() as i32).sum();
                let value = (sum / channels as i32) as i16;
                if let Some(out) = resampler.push(value) {
                    mono.push(out);
                }
            }
            if !mono.is_empty() {
                let _ = tx.send(AudioChunk {
                    role: Role::Me,
                    samples: mono,
                });
            }
        },
        |err| tracing::error!("microphone stream error: {err}"),
        None,
    )?;
    Ok(stream)
}

pub fn start_mic(tx: Sender<AudioChunk>) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no default microphone found"))?;
    let native = device.default_input_config()?;
    let format = native.sample_format();
    let channels = native.channels() as usize;
    let rate = native.sample_rate().0;
    let config: cpal::StreamConfig = native.into();

    let stream = match format {
        cpal::SampleFormat::F32 => build_mic_stream::<f32>(&device, &config, tx, channels, rate)?,
        cpal::SampleFormat::I16 => build_mic_stream::<i16>(&device, &config, tx, channels, rate)?,
        cpal::SampleFormat::U16 => build_mic_stream::<u16>(&device, &config, tx, channels, rate)?,
        other => anyhow::bail!("unsupported microphone sample format: {other:?}"),
    };
    stream.play()?;
    tracing::info!("microphone capture started (native {rate} Hz, {channels} ch -> 16 kHz mono)");
    Ok(stream)
}

pub fn start_system_loopback(
    tx: Sender<AudioChunk>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("system-audio".to_string())
        .spawn(move || {
            if let Err(err) = loopback_thread(tx, running) {
                tracing::error!("system audio capture failed: {err:#}");
            }
        })?;
    Ok(())
}

fn loopback_thread(
    tx: Sender<AudioChunk>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    use wasapi::{
        initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat,
    };

    let _ = initialize_mta();
    let device = DeviceEnumerator::new()?.get_default_device(Direction::Render)?;
    let mut client = device.get_iaudioclient()?;
    let format = WaveFormat::new(32, 32, SampleType::Float, 16000, 1, None);
    let (_default, minimum) = client.get_device_period()?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: minimum,
    };
    client.initialize_client(&format, Direction::Capture, mode, None)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    let mut queue: VecDeque<u8> = VecDeque::new();
    let mut batch = Vec::with_capacity(4096);

    client.start_stream()?;
    tracing::info!("system loopback capture started at 16 kHz mono");
    while running.load(std::sync::atomic::Ordering::Relaxed) {
        capture.read_from_device_to_deque(&mut queue)?;        while queue.len() >= 4 {
            let bytes = [
                queue.pop_front().unwrap(),
                queue.pop_front().unwrap(),
                queue.pop_front().unwrap(),
                queue.pop_front().unwrap(),
            ];
            let value = f32::from_le_bytes(bytes);
            batch.push((value.clamp(-1.0, 1.0) * 32767.0) as i16);
        }
        if !batch.is_empty() {
            let samples = std::mem::take(&mut batch);
            if tx
                .send(AudioChunk {
                    role: Role::Them,
                    samples,
                })
                .is_err()
            {
                break;
            }
        }
        if event.wait_for_event(1_000_000).is_err() {
            break;
        }
    }
    let _ = client.stop_stream();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_at_16k_is_identity() {
        let mut r = Resampler::new(16000);
        for i in 0..1000 {
            assert_eq!(r.push(i), Some(i), "16 kHz passes every sample through");
        }
    }

    #[test]
    fn resampler_48k_to_16k_produces_exact_third() {
        let mut r = Resampler::new(48000);
        let mut out = 0;
        for i in 0u16..48_000 {
            if r.push(i as i16).is_some() {
                out += 1;
            }
        }
        assert_eq!(out, 16_000, "48 kHz in -> exactly 16 kHz out");
    }

    #[test]
    fn sample_conversion_clamps_and_maps() {
        assert_eq!(1.5f32.to_i16_sample(), 32767);
        assert_eq!((-2.0f32).to_i16_sample(), -32767);
        assert_eq!(0u16.to_i16_sample(), -32768);
        assert_eq!(65535u16.to_i16_sample(), 32767);
        assert_eq!(42i16.to_i16_sample(), 42);
    }
}
