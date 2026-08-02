
//! Long-form A/B: render the same ~2 minutes of real Steinbeck with the
//! shortlisted voices. Short clips flatter a voice; fatigue shows up over
//! minutes, not seconds.
use reezy_core::{classify, epub, render, tts::kokoro::KokoroEngine};

const SHORTLIST: &[(i32, &str)] = &[
    (0, "af"),
    (5, "am_adam"),
    (7, "bf_emma"),
    (9, "bm_george"),
];

const MAX_CHARS: usize = 2000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let book = epub::open("/home/chase/Repos/reezy/fixtures/local/east-of-eden.epub")?;
    let body: Vec<_> = book
        .chapters
        .iter()
        .filter(|c| classify::classify(c) == classify::Kind::Body)
        .collect();

    // Chapter 1 proper (index 1 in the narratable list: 0 is the Covici letter).
    let mut ch = body[1].clone();
    let cut = ch
        .text
        .char_indices()
        .take_while(|(i, _)| *i < MAX_CHARS)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    ch.text.truncate(cut);
    println!("passage: {:?} ({} chars)\n", ch.title, ch.text.len());

    std::fs::create_dir_all("/tmp/kokoro-longform")?;
    for (id, name) in SHORTLIST {
        let engine = KokoroEngine::from_default_dir()?.with_speaker(*id);
        let t = std::time::Instant::now();
        let pcm = render::render_chapter(&engine, &ch)?;
        let wall = t.elapsed().as_secs_f32();
        let path = format!("/tmp/kokoro-longform/{name}.wav");
        render::write_wav(&path, &pcm)?;
        let secs = pcm.duration_secs();
        println!(
            "{name:<11} {:>5.1}s audio  {:>5.1}s wall  {:.2}x realtime  \
             -> full book {:>4.1}h audio / {:>4.1}h render",
            secs,
            wall,
            secs / wall,
            secs as f64 / ch.text.len() as f64 * 1_260_792.0 / 3600.0,
            wall as f64 / ch.text.len() as f64 * 1_260_792.0 / 3600.0,
        );
    }
    Ok(())
}
