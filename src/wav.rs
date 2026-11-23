use std::path::Path;

pub struct WavData {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

fn find_chunk<'a>(data: &'a [u8], id: &[u8; 4]) -> Option<&'a [u8]> {
    let mut position = 12usize;
    while position + 8 <= data.len() {
        let chunk_id = &data[position..position + 4];
        let size = u32::from_le_bytes([
            data[position + 4],
            data[position + 5],
            data[position + 6],
            data[position + 7],
        ]) as usize;
        let body_start = position + 8;
        let body_end = (body_start + size).min(data.len());
        if chunk_id == id {
            return Some(&data[body_start..body_end]);
        }
        position = body_start + size + (size & 1);
    }
    None
}

pub fn read_pcm16(path: &Path) -> anyhow::Result<WavData> {
    let data = std::fs::read(path)?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        anyhow::bail!("not a RIFF/WAVE file: {}", path.display());
    }
    let fmt = find_chunk(&data, b"fmt ").ok_or_else(|| anyhow::anyhow!("missing fmt chunk"))?;
    if fmt.len() < 16 {
        anyhow::bail!("short fmt chunk");
    }
    let audio_format = u16::from_le_bytes([fmt[0], fmt[1]]);
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
    let sample_rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
    let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
    if audio_format != 1 || bits != 16 {
        anyhow::bail!("only PCM 16-bit WAV is supported (format={audio_format}, bits={bits})");
    }
    let payload = find_chunk(&data, b"data").ok_or_else(|| anyhow::anyhow!("missing data chunk"))?;
    let samples = payload
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Ok(WavData {
        samples,
        sample_rate,
        channels: channels.max(1),
    })
}
