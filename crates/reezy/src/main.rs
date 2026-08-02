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
    /// Extract chapters from an EPUB into normalized text files
    Extract {
        /// Path to the .epub file
        epub: std::path::PathBuf,
        /// Output directory
        #[arg(short, long, default_value = "./out")]
        out: std::path::PathBuf,
    },
}

fn main() -> eyre::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Extract { epub, out } => {
            println!("extract: {} -> {}", epub.display(), out.display());
            Ok(())
        }
    }
}
