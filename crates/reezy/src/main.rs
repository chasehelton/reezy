use clap::{Parser, Subcommand};
use eyre::Result;
use reezy_core::{epub, render, tts::kokoro::KokoroEngine};

#[derive(Parser)]
#[command(
    name = "reezy",
    version,
    about = "EPUB -> M4B audiobook with local neural TTS"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse an EPUB and report its chapters
    Info {
        epub: std::path::PathBuf,
        /// Include sections that will be skipped during narration
        #[arg(long)]
        all: bool,
    },
    /// Render one chapter to a WAV so you can hear it before a full run
    Sample {
        epub: std::path::PathBuf,
        /// Which narratable chapter (1-based)
        #[arg(short, long, default_value_t = 1)]
        chapter: usize,
        #[arg(short, long, default_value = "sample.wav")]
        out: std::path::PathBuf,
        /// Stop after roughly this many characters
        #[arg(long, default_value_t = 2000)]
        max_chars: usize,
    },
}

fn main() -> Result<()> {
    // Piping into `head` closes stdout early; exit quietly rather than
    // panicking with "failed printing to stdout: Broken pipe".
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    match Cli::parse().command {
        Command::Info { epub, all } => cmd_info(&epub, all),
        Command::Sample {
            epub,
            chapter,
            out,
            max_chars,
        } => cmd_sample(&epub, chapter, &out, max_chars),
    }
}

fn cmd_info(path: &std::path::Path, all: bool) -> Result<()> {
    let book = epub::open(path)?;
    let body = render::narratable(&book);

    println!("title    : {}", book.title);
    println!(
        "author   : {}",
        book.author.as_deref().unwrap_or("(unknown)")
    );
    println!(
        "language : {}",
        book.language.as_deref().unwrap_or("(undeclared)")
    );
    println!(
        "sections : {} total, {} to narrate",
        book.chapters.len(),
        body.len()
    );
    println!();

    if all {
        for c in &book.chapters {
            let kind = reezy_core::classify::classify(c);
            println!(
                "  [{:>3}] {:>8} chars  {:?}  {}",
                c.index,
                c.text.len(),
                kind,
                c.title
            );
        }
    } else {
        for (n, c) in body.iter().enumerate() {
            println!("  {:>3}. {:>8} chars  {}", n + 1, c.text.len(), c.title);
        }
    }

    let chars: usize = body.iter().map(|c| c.text.len()).sum();
    // ~950 chars of prose per minute of narration at Kokoro's default rate.
    let mins = chars as f64 / 950.0;
    println!();
    println!("narratable text : {chars} chars");
    println!("estimated audio : {:.1} hours", mins / 60.0);
    println!(
        "estimated render: {:.1} hours (at 3x realtime)",
        mins / 60.0 / 3.0
    );
    Ok(())
}

fn cmd_sample(
    path: &std::path::Path,
    chapter: usize,
    out: &std::path::Path,
    max_chars: usize,
) -> Result<()> {
    let book = epub::open(path)?;
    let body = render::narratable(&book);
    eyre::ensure!(
        (1..=body.len()).contains(&chapter),
        "chapter {chapter} out of range (book has {} narratable chapters)",
        body.len()
    );

    let mut ch = body[chapter - 1].clone();
    if ch.text.len() > max_chars {
        let cut = ch
            .text
            .char_indices()
            .take_while(|(i, _)| *i < max_chars)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        ch.text.truncate(cut);
    }

    println!("book    : {}", book.title);
    println!("chapter : {} ({} chars)", ch.title, ch.text.len());
    println!("loading Kokoro...");

    let engine = KokoroEngine::from_default_dir()?;
    let started = std::time::Instant::now();
    let pcm = render::render_chapter(&engine, &ch)?;
    let wall = started.elapsed().as_secs_f32();

    render::write_wav(out, &pcm)?;
    let secs = pcm.duration_secs();
    println!(
        "wrote {} -- {:.1}s of audio in {:.1}s ({:.2}x realtime)",
        out.display(),
        secs,
        wall,
        secs / wall
    );
    Ok(())
}
