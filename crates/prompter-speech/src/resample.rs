//! Downmix + downsample any device format to 16 kHz mono i16 for the engine.
//!
//! Windowed-sinc low-pass at the input rate followed by linear interpolation.
//! Plenty for speech recognition, dependency-free, and stateful across chunks.

pub struct Downsampler {
    in_rate: u32,
    out_rate: u32,
    channels: usize,
    taps: Vec<f32>,
    /// Last `taps.len() - 1` mono input samples (filter history).
    hist: Vec<f32>,
    /// Last low-passed sample, for interpolation across chunk borders.
    last_y: f32,
    /// Fractional read position relative to `last_y`.
    pos: f64,
    mono: Vec<f32>,
    filtered: Vec<f32>,
}

impl Downsampler {
    pub fn new(in_rate: u32, channels: u16, out_rate: u32) -> Self {
        let taps = if in_rate == out_rate { Vec::new() } else { lowpass_taps(in_rate, out_rate, 49) };
        Downsampler {
            in_rate,
            out_rate,
            channels: channels.max(1) as usize,
            hist: vec![0.0; taps.len().saturating_sub(1)],
            taps,
            last_y: 0.0,
            pos: 1.0,
            mono: Vec::new(),
            filtered: Vec::new(),
        }
    }

    pub fn in_rate(&self) -> u32 {
        self.in_rate
    }

    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|h| *h = 0.0);
        self.last_y = 0.0;
        self.pos = 1.0;
    }

    /// `interleaved` holds `channels` samples per frame. Output is appended to `out`.
    pub fn process(&mut self, interleaved: &[f32], out: &mut Vec<i16>) {
        self.mono.clear();
        for frame in interleaved.chunks_exact(self.channels) {
            self.mono.push(frame.iter().sum::<f32>() / self.channels as f32);
        }
        if self.in_rate == self.out_rate {
            out.extend(self.mono.iter().map(|&s| to_i16(s)));
            return;
        }

        // Low-pass: y[i] = sum_k taps[k] * x[i - k], with history prepended.
        let n = self.taps.len();
        let mut x = std::mem::take(&mut self.hist);
        x.extend_from_slice(&self.mono);
        self.filtered.clear();
        for i in 0..self.mono.len() {
            let mut acc = 0.0f32;
            let base = i + n - 1;
            for (k, t) in self.taps.iter().enumerate() {
                acc += t * x[base - k];
            }
            self.filtered.push(acc);
        }
        // Keep the tail as history for the next chunk.
        let keep = n - 1;
        let start = x.len().saturating_sub(keep);
        x.drain(..start);
        self.hist = x;

        // Linear interpolation over [last_y, filtered...].
        let step = self.in_rate as f64 / self.out_rate as f64;
        let len = self.filtered.len() + 1; // virtual index 0 is last_y
        let at = |i: usize, s: &Self| if i == 0 { s.last_y } else { s.filtered[i - 1] };
        while self.pos + 1.0 < len as f64 {
            let i = self.pos.floor() as usize;
            let f = (self.pos - i as f64) as f32;
            let v = at(i, self) * (1.0 - f) + at(i + 1, self) * f;
            out.push(to_i16(v));
            self.pos += step;
        }
        self.pos -= (len - 1) as f64;
        if let Some(&y) = self.filtered.last() {
            self.last_y = y;
        }
    }
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0) as i16
}

/// Hamming-windowed sinc low-pass with cutoff at 90 % of the output Nyquist.
fn lowpass_taps(in_rate: u32, out_rate: u32, n: usize) -> Vec<f32> {
    let fc = 0.45 * out_rate as f64 / in_rate as f64; // cycles per input sample
    let m = (n - 1) as f64 / 2.0;
    let mut taps: Vec<f64> = (0..n)
        .map(|k| {
            let x = k as f64 - m;
            let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * x) };
            let w = 0.54 - 0.46 * (2.0 * std::f64::consts::PI * k as f64 / (n - 1) as f64).cos();
            sinc * w
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    taps.iter_mut().for_each(|t| *t /= sum);
    taps.into_iter().map(|t| t as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, secs: f32, channels: usize) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        let mut v = Vec::with_capacity(n * channels);
        for i in 0..n {
            let s = (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.8;
            for _ in 0..channels {
                v.push(s);
            }
        }
        v
    }

    fn run(rate: u32, hz: f32, channels: u16, chunk: usize) -> Vec<i16> {
        let input = sine(rate, hz, 1.0, channels as usize);
        let mut ds = Downsampler::new(rate, channels, 16000);
        let mut out = Vec::new();
        for c in input.chunks(chunk) {
            ds.process(c, &mut out);
        }
        out
    }

    fn peak(v: &[i16]) -> f32 {
        v.iter().map(|s| (*s as f32).abs()).fold(0.0, f32::max) / 32767.0
    }

    #[test]
    fn output_length_matches_ratio_regardless_of_chunking() {
        for chunk in [7, 480, 4096] {
            let out = run(48000, 1000.0, 2, chunk * 2);
            assert!((out.len() as i64 - 16000).abs() < 4, "chunk {chunk}: {}", out.len());
        }
    }

    #[test]
    fn keeps_a_1khz_tone_and_kills_a_15khz_tone() {
        let ok = run(48000, 1000.0, 1, 512);
        let p = peak(&ok[8000..]);
        assert!(p > 0.72 && p < 0.86, "1 kHz peak {p}");
        // 1 kHz at 16 kHz sample rate crosses zero 2000 times/s.
        let crossings = ok[8000..].windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
        assert!((crossings as i64 - 1000).abs() < 40, "crossings {crossings}");

        let bad = run(48000, 15000.0, 1, 512);
        let p = peak(&bad[8000..]);
        assert!(p < 0.05, "15 kHz peak {p}");
    }

    #[test]
    fn same_rate_is_a_plain_downmix() {
        let mut ds = Downsampler::new(16000, 2, 16000);
        let mut out = Vec::new();
        ds.process(&[0.5, -0.5, 1.0, 1.0], &mut out);
        assert_eq!(out, vec![0, 32767]);
    }

    #[test]
    fn handles_44100() {
        let out = run(44100, 440.0, 1, 1000);
        assert!((out.len() as i64 - 16000).abs() < 4, "{}", out.len());
        assert!(peak(&out[8000..]) > 0.72);
    }
}
