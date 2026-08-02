//! Turn a parsed book into audio.

use crate::classify::{self, Kind};
use crate::epub::{Book, Chapter};
use crate::normalize;
use crate::tts::{Pcm, TtsEngine, chunk};
use eyre::Result;

/// Silence inserted between chunks within a chapter.
const CHUNK_GAP_SECS: f32 = 0.25;

/// The chapters worth narrating, in reading order.
pub fn narratable(book: &Book) -> Vec<&Chapter> {
    book.chapters
        .iter()
        .filter(|c| classify::classify(c) == Kind::Body)
        .collect()
}

/// Normalize, chunk, and synthesize one chapter.
pub fn render_chapter(engine: &dyn TtsEngine, chapter: &Chapter) -> Result<Pcm> {
    let text = normalize::normalize(&chapter.text);
    let mut out = Pcm {
        samples: Vec::new(),
        sample_rate: engine.sample_rate(),
    };
    for piece in chunk::split(&text) {
        let pcm = engine.synth(&piece)?;
        out.append(&pcm, CHUNK_GAP_SECS);
    }
    Ok(out)
}

/// Write mono f32 samples as a 16-bit PCM WAV.
pub fn write_wav(path: impl AsRef<std::path::Path>, pcm: &Pcm) -> Result<()> {
    use std::io::Write;

    let mut f = std::io::BufWriter::new(std::fs::File::create(path.as_ref())?);
    let n = pcm.samples.len() as u32;
    let data_bytes = n * 2;
    let (channels, bits) = (1u16, 16u16);
    let block_align = channels * bits / 8;
    let byte_rate = pcm.sample_rate * block_align as u32;

    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_bytes).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&pcm.sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&bits.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_bytes.to_le_bytes())?;
    for &s in &pcm.samples {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    f.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narratable_skips_dividers_and_front_matter() {
        let book = Book {
            title: "T".into(),
            author: None,
            language: None,
            chapters: vec![
                Chapter {
                    index: 0,
                    title: "Cover".into(),
                    text: "x".into(),
                },
                Chapter {
                    index: 1,
                    title: "Part One".into(),
                    text: "Part One".into(),
                },
                Chapter {
                    index: 2,
                    title: "Chapter 1".into(),
                    text: "The Salinas Valley is in Northern California. ".repeat(20),
                },
            ],
        };
        let got: Vec<&str> = narratable(&book).iter().map(|c| c.title.as_str()).collect();
        assert_eq!(got, ["Chapter 1"]);
    }

    #[test]
    fn wav_header_is_well_formed() {
        let dir = std::env::temp_dir().join("reezy-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.wav");
        let pcm = Pcm {
            samples: vec![0.5; 100],
            sample_rate: 24_000,
        };
        write_wav(&path, &pcm).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        // 44-byte header + 100 samples * 2 bytes
        assert_eq!(bytes.len(), 44 + 200);
        std::fs::remove_file(&path).ok();
    }
}
