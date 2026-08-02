//! Assemble rendered chapters into a single tagged M4B audiobook.
//!
//! M4B is an MP4/AAC container with chapter atoms -- Apple's native audiobook
//! format, and what BookPlayer, Apple Books, and Audiobookshelf expect. A
//! single file with chapter markers is far nicer on a phone than a pile of
//! MP3s: one item in the library, working chapter list, and resume across the
//! whole book.
//!
//! Encoding goes through ffmpeg. A pure-Rust AAC encoder would remove the
//! dependency, but ffmpeg is already required for playback tooling and its
//! chapter-atom handling is correct; this is not where the interesting work is.

use crate::tts::Pcm;
use eyre::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// One chapter's position in the finished file.
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterMark {
    pub title: String,
    pub start_secs: f64,
    pub end_secs: f64,
}

/// Everything needed to tag the output.
#[derive(Debug, Clone, Default)]
pub struct Tags {
    pub title: String,
    pub author: Option<String>,
    pub narrator: Option<String>,
    /// Cover image bytes (JPEG or PNG).
    pub cover: Option<Vec<u8>>,
}

/// Build an ffmpeg metadata file describing title, author, and chapters.
///
/// ffmetadata timebase is set to 1/1000 so chapter offsets are in
/// milliseconds; using seconds loses sub-second placement on long books.
pub fn ffmetadata(tags: &Tags, marks: &[ChapterMark]) -> String {
    let mut s = String::from(";FFMETADATA1\n");
    s.push_str(&format!("title={}\n", escape(&tags.title)));
    if let Some(a) = &tags.author {
        s.push_str(&format!("artist={}\n", escape(a)));
        s.push_str(&format!("album_artist={}\n", escape(a)));
    }
    s.push_str(&format!("album={}\n", escape(&tags.title)));
    if let Some(n) = &tags.narrator {
        s.push_str(&format!("composer={}\n", escape(n)));
    }
    s.push_str("genre=Audiobook\n");

    for m in marks {
        s.push_str("\n[CHAPTER]\nTIMEBASE=1/1000\n");
        s.push_str(&format!(
            "START={}\n",
            (m.start_secs * 1000.0).round() as i64
        ));
        s.push_str(&format!("END={}\n", (m.end_secs * 1000.0).round() as i64));
        s.push_str(&format!("title={}\n", escape(&m.title)));
    }
    s
}

/// ffmetadata treats `=`, `;`, `#`, `\` and newlines as special.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '=' | ';' | '#' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// Compute chapter boundaries from rendered durations.
pub fn marks_from(titles: &[String], durations: &[f32]) -> Vec<ChapterMark> {
    let mut marks = Vec::with_capacity(titles.len());
    let mut cursor = 0.0f64;
    for (title, &dur) in titles.iter().zip(durations) {
        let end = cursor + dur as f64;
        marks.push(ChapterMark {
            title: title.clone(),
            start_secs: cursor,
            end_secs: end,
        });
        cursor = end;
    }
    marks
}

/// Encode joined PCM into a tagged `.m4b`.
///
/// Samples are streamed to ffmpeg's stdin as raw f32le rather than staged as a
/// WAV, which avoids writing a multi-gigabyte intermediate for a 20-hour book.
pub fn write_m4b(
    out: &Path,
    pcm: &Pcm,
    tags: &Tags,
    marks: &[ChapterMark],
    bitrate_kbps: u32,
) -> Result<()> {
    let dir = out.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).ok();

    let meta_path = dir.join(".reezy-ffmetadata.txt");
    std::fs::write(&meta_path, ffmetadata(tags, marks))
        .with_context(|| format!("writing {}", meta_path.display()))?;

    let cover_path = tags.cover.as_ref().map(|bytes| {
        let p = dir.join(".reezy-cover.img");
        let _ = std::fs::write(&p, bytes);
        p
    });

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
        // input 0: raw samples on stdin
        .args([
            "-f",
            "f32le",
            "-ar",
            &pcm.sample_rate.to_string(),
            "-ac",
            "1",
            "-i",
            "-",
        ])
        // input 1: metadata + chapters
        .args(["-i"])
        .arg(&meta_path);

    if let Some(c) = &cover_path {
        cmd.arg("-i").arg(c);
    }

    cmd.args(["-map_metadata", "1", "-map", "0:a"]);
    if cover_path.is_some() {
        cmd.args(["-map", "2:v", "-disposition:v", "attached_pic"]);
    }
    cmd.args(["-c:a", "aac", "-b:a", &format!("{bitrate_kbps}k")]);
    if cover_path.is_some() {
        cmd.args(["-c:v", "copy"]);
    }
    // `ipod` is the muxer that writes real m4b chapter atoms.
    cmd.args(["-f", "ipod"]).arg(out);

    let mut child = cmd
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run ffmpeg -- is it installed and on PATH?")?;

    {
        let mut stdin = child.stdin.take().expect("stdin was piped");
        let mut buf = Vec::with_capacity(64 * 1024);
        for chunk in pcm.samples.chunks(16_384) {
            buf.clear();
            for &s in chunk {
                buf.extend_from_slice(&s.clamp(-1.0, 1.0).to_le_bytes());
            }
            stdin.write_all(&buf).context("writing samples to ffmpeg")?;
        }
    } // drop closes stdin so ffmpeg finishes

    let output = child.wait_with_output().context("waiting for ffmpeg")?;
    let _ = std::fs::remove_file(&meta_path);
    if let Some(c) = cover_path {
        let _ = std::fs::remove_file(c);
    }

    eyre::ensure!(
        output.status.success(),
        "ffmpeg failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_contiguous_and_cumulative() {
        let titles = vec!["One".to_string(), "Two".to_string(), "Three".to_string()];
        let marks = marks_from(&titles, &[10.0, 20.0, 5.0]);
        assert_eq!(marks[0].start_secs, 0.0);
        assert_eq!(marks[0].end_secs, 10.0);
        assert_eq!(marks[1].start_secs, 10.0);
        assert_eq!(marks[2].end_secs, 35.0);
        // No gaps: each chapter starts where the previous ended.
        for w in marks.windows(2) {
            assert_eq!(w[0].end_secs, w[1].start_secs);
        }
    }

    #[test]
    fn ffmetadata_uses_millisecond_timebase() {
        let marks = marks_from(&["A".into()], &[1.5]);
        let s = ffmetadata(
            &Tags {
                title: "T".into(),
                ..Default::default()
            },
            &marks,
        );
        assert!(s.contains("TIMEBASE=1/1000"), "{s}");
        assert!(s.contains("START=0"), "{s}");
        assert!(s.contains("END=1500"), "{s}");
    }

    #[test]
    fn ffmetadata_escapes_special_characters() {
        let tags = Tags {
            title: "Cause = Effect; #1".into(),
            author: Some("A\nB".into()),
            ..Default::default()
        };
        let s = ffmetadata(&tags, &[]);
        assert!(s.contains(r"Cause \= Effect\; \#1"), "{s}");
        // Newlines in a value would corrupt the file.
        assert!(s.contains("artist=A B"), "{s}");
    }

    #[test]
    fn chapter_titles_with_equals_do_not_break_parsing() {
        let marks = marks_from(&["Chapter 1 = The Start".into()], &[1.0]);
        let s = ffmetadata(&Tags::default(), &marks);
        assert!(s.contains(r"title=Chapter 1 \= The Start"), "{s}");
    }
}
