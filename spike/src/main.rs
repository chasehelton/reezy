use sherpa_rs::tts::{KokoroTts, KokoroTtsConfig};
use std::time::Instant;

const MODEL_DIR: &str = "/home/chase/.local/share/abook/models/kokoro-en-v0_19";

/// A/B pairs: (label, raw-text, normalized-text)
/// Proves whether the Phase 1.5 normalization layer actually fixes the
/// artifacts heard in the first spike run.
const CASES: &[(&str, &str, &str)] = &[
    (
        "abbrev",
        "In the winter of 1904, Dr. Aldous Renwick arrived at the harbor.",
        "In the winter of nineteen oh four, Doctor Aldous Renwick arrived at the harbor.",
    ),
    (
        "titles",
        "Mr. Vane and St. John met Mrs. Ash at 4 p.m. on Elm St. that day.",
        "Mister Vane and Saint John met Missus Ash at four PM on Elm Street that day.",
    ),
    (
        "numbers",
        "By 1987 the 3rd volume cost $4.50, up 12% from 1902.",
        "By nineteen eighty seven the third volume cost four dollars and fifty cents, up twelve percent from nineteen oh two.",
    ),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let threads = std::thread::available_parallelism()?.get() as i32 - 1;
    let mut tts = KokoroTts::new(KokoroTtsConfig {
        model: format!("{MODEL_DIR}/model.onnx"),
        voices: format!("{MODEL_DIR}/voices.bin"),
        tokens: format!("{MODEL_DIR}/tokens.txt"),
        data_dir: format!("{MODEL_DIR}/espeak-ng-data"),
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

    for (label, raw, norm) in CASES {
        for (variant, text) in [("raw", raw), ("norm", norm)] {
            let t = Instant::now();
            let audio = tts.create(text, 0, 1.0)?;
            let secs = audio.samples.len() as f32 / audio.sample_rate as f32;
            let path = format!("ab-{label}-{variant}.wav");
            write_wav(&path, &audio.samples, audio.sample_rate)?;
            println!(
                "{path:28} {secs:5.2}s audio  {:5.2}s wall",
                t.elapsed().as_secs_f32()
            );
        }
    }
    println!("\nCompare each pair; 'norm' should have no mid-sentence pause.");
    Ok(())
}

fn write_wav(path: &str, samples: &[f32], rate: u32) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let n = samples.len() as u32;
    let (bytes, block, bits) = (n * 2, 2u16, 16u16);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + bytes).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&rate.to_le_bytes())?;
    f.write_all(&(rate * block as u32).to_le_bytes())?;
    f.write_all(&block.to_le_bytes())?;
    f.write_all(&bits.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&bytes.to_le_bytes())?;
    for &s in samples {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    Ok(())
}
