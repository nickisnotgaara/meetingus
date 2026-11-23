use crate::audio::{AudioChunk, Role};
use webrtc_vad::{SampleRate, Vad, VadMode};

pub struct SpeechSegment {
    pub role: Role,
    pub samples: Vec<i16>,
}

struct ChannelState {
    vad: Vad,
    buffer: Vec<i16>,
    segment: Vec<i16>,
    active: bool,
    hangover: usize,
}

impl ChannelState {
    fn new() -> Self {
        Self {
            vad: Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::Aggressive),
            buffer: Vec::new(),
            segment: Vec::new(),
            active: false,
            hangover: 0,
        }
    }

    fn push(&mut self, samples: &[i16]) -> Option<Vec<i16>> {
        const FRAME: usize = 320;
        const HANGOVER: usize = 15;
        const MIN_FRAMES: usize = 5;

        self.buffer.extend_from_slice(samples);
        let mut finished = None;

        while self.buffer.len() >= FRAME {
            let frame: Vec<i16> = self.buffer.drain(..FRAME).collect();
            let voiced = self.vad.is_voice_segment(&frame).unwrap_or(false);
            if voiced {
                self.active = true;
                self.hangover = HANGOVER;
                self.segment.extend_from_slice(&frame);
            } else if self.active {
                self.hangover = self.hangover.saturating_sub(1);
                self.segment.extend_from_slice(&frame);
                if self.hangover == 0 {
                    self.active = false;
                    let segment = std::mem::take(&mut self.segment);
                    if segment.len() >= FRAME * MIN_FRAMES {
                        finished = Some(segment);
                    }
                }
            }
        }
        finished
    }
}

pub struct VadProcessor {
    me: ChannelState,
    them: ChannelState,
}

impl VadProcessor {
    pub fn new() -> Self {
        Self {
            me: ChannelState::new(),
            them: ChannelState::new(),
        }
    }

    pub fn is_active(&self, role: Role) -> bool {
        let channel = match role {
            Role::Me => &self.me,
            Role::Them => &self.them,
        };
        channel.active || channel.hangover > 0
    }

    pub fn push(&mut self, chunk: AudioChunk) -> Option<SpeechSegment> {
        let role = chunk.role;
        let segment = match role {
            Role::Me => self.me.push(&chunk.samples),
            Role::Them => self.them.push(&chunk.samples),
        };
        segment.map(|samples| SpeechSegment { role, samples })
    }
}
