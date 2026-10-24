//! Minimal WAV reader (PCM 16-bit and 32-bit float) for the replay tool and tests.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct Wav {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved samples in -1..1.
    pub samples: Vec<f32>,
}

pub fn read(path: &Path) -> Result<Wav, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Wav, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // (format, channels, rate, bits)
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body_start = pos + 8;
        let body_end = (body_start + size).min(bytes.len());
        match id {
            b"fmt " if size >= 16 => {
                fmt = Some((u16_at(body_start), u16_at(body_start + 2), u32_at(body_start + 4), u16_at(body_start + 14)));
            }
            b"data" => data = Some(&bytes[body_start..body_end]),
            _ => {}
        }
        pos = body_start + size + (size & 1);
    }
    let (format, channels, sample_rate, bits) = fmt.ok_or("missing fmt chunk")?;
    let data = data.ok_or("missing data chunk")?;
    let samples = match (format, bits) {
        (1, 16) | (65534, 16) => data.as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes(*c) as f32 / 32768.0).collect(),
        (3, 32) => data.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect(),
        _ => return Err(format!("unsupported WAV format {format} / {bits} bit")),
    };
    Ok(Wav { sample_rate, channels, samples })
}

/// Writes 16-bit PCM. Used by the take recorder (phase 1.5) and tests.
pub fn write_pcm16(path: &Path, sample_rate: u32, channels: u16, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_pcm16() {
        let dir = std::env::temp_dir().join(format!("pp-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.wav");
        let samples = vec![0.0, 0.5, -0.5, 0.999];
        write_pcm16(&p, 48000, 2, &samples).unwrap();
        let w = read(&p).unwrap();
        assert_eq!(w.sample_rate, 48000);
        assert_eq!(w.channels, 2);
        assert_eq!(w.samples.len(), 4);
        assert!((w.samples[1] - 0.5).abs() < 1e-3);
        std::fs::remove_dir_all(dir).ok();
    }
}
