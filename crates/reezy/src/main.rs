use clap::{Parser, Subcommand};
use eyre::Result;
use reezy_core::{
    assemble, epub, render,
    tts::{kokoro::KokoroEngine, voice},
};

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
        #[command(flatten)]
        voice: VoiceOpts,
    },
    /// Render a whole book to a chapter-tagged M4B audiobook
    Build {
        epub: std::path::PathBuf,
        /// Output .m4b (defaults to the book title)
        #[arg(short, long)]
        out: Option<std::path::PathBuf>,
        /// Render only the first N chapters
        #[arg(long)]
        limit: Option<usize>,
        /// Parallel render workers. Each loads its own ~350MB model copy.
        #[arg(short = 'j', long, default_value_t = 3)]
        workers: usize,
        /// AAC bitrate in kbps; 64 is ample for speech
        #[arg(long, default_value_t = 64)]
        bitrate: u32,
        #[command(flatten)]
        voice: VoiceOpts,
    },
    /// List the available narrator voices
    Voices,
}

#[derive(clap::Args)]
struct VoiceOpts {
    /// Narrator voice, by name or id (see `reezy voices`)
    #[arg(short, long, default_value = voice::DEFAULT_VOICE)]
    voice: String,
    /// Speaking rate; 1.0 is normal, higher is faster
    #[arg(long, default_value_t = 1.0)]
    speed: f32,
}

impl VoiceOpts {
    fn engine(&self) -> Result<KokoroEngine> {
        let id = voice::resolve(&self.voice)?;
        eyre::ensure!(
            (0.5..=2.0).contains(&self.speed),
            "speed {} out of range (0.5 to 2.0)",
            self.speed
        );
        Ok(KokoroEngine::from_default_dir()?
            .with_speaker(id)
            .with_speed(self.speed))
    }
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
            voice,
        } => cmd_sample(&epub, chapter, &out, max_chars, &voice),
        Command::Build {
            epub,
            out,
            limit,
            workers,
            bitrate,
            voice,
        } => cmd_build(&epub, out.as_deref(), limit, workers, bitrate, &voice),
        Command::Voices => {
            println!("{}", voice::describe_all());
            println!("\ndefault: {}", voice::DEFAULT_VOICE);
            Ok(())
        }
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

fn cmd_build(
    path: &std::path::Path,
    out: Option<&std::path::Path>,
    limit: Option<usize>,
    workers: usize,
    bitrate: u32,
    opts: &VoiceOpts,
) -> Result<()> {
    let speaker = voice::resolve(&opts.voice)?;
    eyre::ensure!(workers >= 1, "workers must be at least 1");

    let book = epub::open(path)?;
    let all = render::narratable(&book);
    let chapters: Vec<_> = match limit {
        Some(n) => all.into_iter().take(n).collect(),
        None => all,
    };
    eyre::ensure!(!chapters.is_empty(), "nothing to narrate in this book");

    let default_out = std::path::PathBuf::from(format!("{}.m4b", sanitize(&book.title)));
    let out = out.unwrap_or(&default_out);

    println!("book     : {}", book.title);
    println!(
        "author   : {}",
        book.author.as_deref().unwrap_or("(unknown)")
    );
    println!(
        "voice    : {} [{}] ({})",
        opts.voice,
        speaker,
        voice::describe(&opts.voice)
    );
    println!("chapters : {}", chapters.len());
    println!("workers  : {workers}");
    println!("output   : {}", out.display());
    println!();

    let started = std::time::Instant::now();
    let speed = opts.speed;
    let voice_name = opts.voice.clone();
    let pcms = render::render_all(
        &chapters,
        || {
            let id = voice::resolve(&voice_name)?;
            Ok(Box::new(
                KokoroEngine::from_default_dir()?
                    .with_speaker(id)
                    .with_speed(speed),
            ) as Box<dyn reezy_core::tts::TtsEngine>)
        },
        workers,
        |done, total| {
            let pct = done as f64 / total as f64 * 100.0;
            print!(
                "
  rendering {done}/{total} ({pct:.0}%)          "
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();
        },
    )?;
    println!();

    let joined = {
        let mut acc = reezy_core::tts::Pcm {
            samples: Vec::new(),
            sample_rate: pcms.first().map_or(24_000, |p| p.sample_rate),
        };
        for p in &pcms {
            acc.append(p, 0.0);
        }
        acc
    };

    let titles: Vec<String> = chapters.iter().map(|c| c.title.clone()).collect();
    let durations: Vec<f32> = pcms.iter().map(|p| p.duration_secs()).collect();
    let marks = assemble::marks_from(&titles, &durations);

    let tags = assemble::Tags {
        title: book.title.clone(),
        author: book.author.clone(),
        narrator: Some(format!("Kokoro ({})", opts.voice)),
        cover: book.cover.clone(),
    };

    println!("encoding to {} ...", out.display());
    assemble::write_m4b(out, &joined, &tags, &marks, bitrate)?;

    let wall = started.elapsed().as_secs_f64();
    let audio = joined.duration_secs() as f64;
    let size = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
    println!();
    println!("done in {:.1} min", wall / 60.0);
    println!(
        "  {:.1} h of audio, {} chapters, {:.1} MB",
        audio / 3600.0,
        marks.len(),
        size as f64 / 1e6
    );
    println!("  {:.2}x realtime", audio / wall);
    Ok(())
}

/// Make a title safe for a filename.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .replace(' ', "-")
}

fn cmd_sample(
    path: &std::path::Path,
    chapter: usize,
    out: &std::path::Path,
    max_chars: usize,
    opts: &VoiceOpts,
) -> Result<()> {
    // Validate cheap inputs before parsing a 600k-word book or loading a
    // 345MB model, so a typo fails instantly instead of after the banner.
    let speaker = voice::resolve(&opts.voice)?;

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
    println!(
        "voice   : {} [{}] ({})",
        opts.voice,
        speaker,
        voice::describe(&opts.voice)
    );
    println!("loading Kokoro...");

    let engine = opts.engine()?;
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
