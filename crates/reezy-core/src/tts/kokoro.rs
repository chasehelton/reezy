//! Kokoro-82M via sherpa-onnx.
//!
//! Measured at 3.0x faster than realtime on a Ryzen 5 PRO 7530U, CPU only.
//! See BENCHMARKS.md.

use super::{Pcm, TtsEngine};
use eyre::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where models live by default.
pub fn default_model_dir() -> PathBuf {
    dirs_home()
        .join(".local/share/abook/models")
        .join(DEFAULT_MODEL)
}

/// The English-only bundle.
///
/// **Do not swap this for `kokoro-multi-lang-v1_1`.** That model's token set
/// omits the rhotic schwa (U+025A), so espeak-ng emits a phoneme sherpa then
/// silently drops -- audibly clipping the r in "winter", "harbor", "leather".
/// Verified: `grep -c "ɚ" tokens.txt` is 1 here and 0 there.
pub const DEFAULT_MODEL: &str = "kokoro-en-v0_19";

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub struct KokoroEngine {
    /// sherpa's `create` takes `&mut self`, so serialize access. Chapters are
    /// synthesized in parallel by giving each worker its own engine.
    tts: Mutex<sherpa_rs::tts::KokoroTts>,
    sample_rate: u32,
    speaker_id: i32,
    speed: f32,
}

impl KokoroEngine {
    /// Load from a model directory containing `model.onnx`, `voices.bin`,
    /// `tokens.txt`, and `espeak-ng-data/`.
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();
        for f in ["model.onnx", "voices.bin", "tokens.txt"] {
            let p = dir.join(f);
            eyre::ensure!(p.exists(), "missing {} in {}", f, dir.display());
        }
        let s = |p: &str| dir.join(p).to_string_lossy().into_owned();

        let threads = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1).max(1))
            .unwrap_or(1) as i32;

        let tts = sherpa_rs::tts::KokoroTts::new(sherpa_rs::tts::KokoroTtsConfig {
            model: s("model.onnx"),
            voices: s("voices.bin"),
            tokens: s("tokens.txt"),
            data_dir: s("espeak-ng-data"),
            // en-v0_19 needs neither a dict nor a lexicon; passing paths that do
            // not exist makes sherpa log errors and fall back.
            dict_dir: String::new(),
            lexicon: String::new(),
            lang: "en-us".into(),
            length_scale: 1.0,
            onnx_config: sherpa_rs::OnnxConfig {
                num_threads: threads,
                provider: "cpu".into(),
                debug: false,
            },
            ..Default::default()
        });

        Ok(Self {
            tts: Mutex::new(tts),
            sample_rate: 24_000,
            speaker_id: 0,
            speed: 1.0,
        })
    }

    /// Load from the default location.
    pub fn from_default_dir() -> Result<Self> {
        let dir = default_model_dir();
        Self::new(&dir).with_context(|| {
            format!(
                "Kokoro model not found at {}.\nDownload {DEFAULT_MODEL} from \
                 https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models \
                 and extract it there.",
                dir.display()
            )
        })
    }

    /// Voice index within `voices.bin`.
    pub fn with_speaker(mut self, id: i32) -> Self {
        self.speaker_id = id;
        self
    }

    /// 1.0 is normal; higher is faster.
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }
}

impl TtsEngine for KokoroEngine {
    fn synth(&self, text: &str) -> Result<Pcm> {
        if text.trim().is_empty() {
            return Ok(Pcm {
                samples: Vec::new(),
                sample_rate: self.sample_rate,
            });
        }
        let mut tts = self
            .tts
            .lock()
            .map_err(|_| eyre::eyre!("Kokoro engine lock poisoned"))?;
        let audio = tts
            .create(text, self.speaker_id, self.speed)
            .map_err(|e| eyre::eyre!("Kokoro synthesis failed: {e}"))?;
        Ok(Pcm {
            samples: audio.samples,
            sample_rate: audio.sample_rate,
        })
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn name(&self) -> &str {
        "kokoro"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_present() -> bool {
        default_model_dir().join("model.onnx").exists()
    }

    #[test]
    fn missing_model_dir_is_a_clear_error() {
        let msg = match KokoroEngine::new("/nonexistent/model/dir") {
            Ok(_) => panic!("expected an error for a missing model dir"),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains("missing model.onnx"), "got: {msg}");
    }

    #[test]
    fn synthesizes_real_audio() {
        if !model_present() {
            eprintln!("skipping: Kokoro model not installed");
            return;
        }
        let engine = KokoroEngine::from_default_dir().expect("load Kokoro");
        let pcm = engine
            .synth("The Salinas Valley is in Northern California.")
            .unwrap();

        assert_eq!(pcm.sample_rate, 24_000);
        assert!(pcm.duration_secs() > 1.0, "got {}s", pcm.duration_secs());
        // Real speech, not silence.
        let peak = pcm.samples.iter().fold(0f32, |a, s| a.max(s.abs()));
        assert!(peak > 0.05, "output looks like silence (peak {peak})");
    }

    #[test]
    fn empty_text_yields_empty_audio_without_calling_the_model() {
        if !model_present() {
            eprintln!("skipping: Kokoro model not installed");
            return;
        }
        let engine = KokoroEngine::from_default_dir().unwrap();
        assert!(engine.synth("   ").unwrap().samples.is_empty());
    }
}
