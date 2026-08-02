use clap::{Parser, Subcommand};

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
        /// Path to the .epub file
        epub: std::path::PathBuf,
    },
}

fn main() -> eyre::Result<()> {
    // Piping into `head` closes stdout early; a CLI should exit quietly rather
    // than panicking with "failed printing to stdout: Broken pipe".
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let cli = Cli::parse();
    match cli.command {
        Command::Info { epub } => {
            let book = reezy_core::epub::open(&epub)?;
            println!("title    : {}", book.title);
            println!(
                "author   : {}",
                book.author.as_deref().unwrap_or("(unknown)")
            );
            println!(
                "language : {}",
                book.language.as_deref().unwrap_or("(undeclared)")
            );
            println!("chapters : {}", book.chapters.len());
            println!();
            let total: usize = book.chapters.iter().map(|c| c.text.len()).sum();
            for c in &book.chapters {
                println!("  [{:>3}] {:>8} chars  {}", c.index, c.text.len(), c.title);
            }
            println!();
            println!("total text: {total} chars");
            Ok(())
        }
    }
}
