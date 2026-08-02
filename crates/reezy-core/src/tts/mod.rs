//! Text-to-speech backends.
//!
//! Engines sit behind [`TtsEngine`] so Kokoro (default), Piper (fast drafts),
//! and a remote GPU worker are interchangeable. sherpa-onnx exposes all of
//! these through one ONNX Runtime interface, so adding a backend is mostly
//! config.

pub mod chunk;
pub mod kokoro;
pub mod voice;

use eyre::Result;

/// Mono audio samples in `[-1.0, 1.0]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Pcm {
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    /// Append another clip, inserting `gap_secs` of silence between them.
    pub fn append(&mut self, other: &Pcm, gap_secs: f32) {
        debug_assert_eq!(self.sample_rate, other.sample_rate);
        if !self.samples.is_empty() && gap_secs > 0.0 {
            let n = (self.sample_rate as f32 * gap_secs) as usize;
            self.samples.extend(std::iter::repeat_n(0.0, n));
        }
        self.samples.extend_from_slice(&other.samples);
    }
}

/// A speech synthesizer.
pub trait TtsEngine: Send + Sync {
    /// Synthesize one chunk of already-normalized text.
    fn synth(&self, text: &str) -> Result<Pcm>;
    fn sample_rate(&self) -> u32;
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_matches_sample_count() {
        let pcm = Pcm {
            samples: vec![0.0; 24_000],
            sample_rate: 24_000,
        };
        assert!((pcm.duration_secs() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn append_inserts_a_gap_between_clips_but_not_before_the_first() {
        let mut a = Pcm {
            samples: vec![],
            sample_rate: 100,
        };
        let b = Pcm {
            samples: vec![1.0; 100],
            sample_rate: 100,
        };
        a.append(&b, 0.5); // no leading silence on an empty buffer
        assert_eq!(a.samples.len(), 100);
        a.append(&b, 0.5); // 50 samples of gap, then 100 of audio
        assert_eq!(a.samples.len(), 250);
        assert_eq!(a.samples[100..150], [0.0; 50]);
    }
}
