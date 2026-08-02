//! Kokoro v0.19 voice names.
//!
//! `voices.bin` is an array of style vectors indexed by position, so the model
//! itself has no notion of names. The order below is Kokoro v0.19's published
//! ordering (hexgrad/Kokoro-82M), verified against the 11-voice file shipped
//! with `kokoro-en-v0_19`:
//! 5,755,904 bytes / (511 tokens * 256 floats * 4 bytes) = 11 voices.

use eyre::{Result, eyre};

/// Voice order in `voices.bin`. Index is the sherpa speaker id.
///
/// Naming: `a`/`b` = American/British, `f`/`m` = female/male.
pub const VOICES: &[&str] = &[
    "af",          // 0  American female (default blend)
    "af_bella",    // 1
    "af_nicole",   // 2
    "af_sarah",    // 3
    "af_sky",      // 4
    "am_adam",     // 5  American male
    "am_michael",  // 6
    "bf_emma",     // 7  British female
    "bf_isabella", // 8
    "bm_george",   // 9  British male
    "bm_lewis",    // 10
];

/// Default narrator.
///
/// Chosen by listening to ~2 minutes of real prose from each shortlisted voice
/// rather than a short clip, since fatigue only shows up over minutes.
pub const DEFAULT_VOICE: &str = "bm_george";

/// Resolve a voice name to its speaker id.
///
/// A bare integer is accepted as an escape hatch for voices this table does not
/// know about, so a future model with more voices stays usable.
pub fn resolve(name: &str) -> Result<i32> {
    let name = name.trim();

    if let Ok(id) = name.parse::<i32>() {
        return if (0..VOICES.len() as i32).contains(&id) {
            Ok(id)
        } else {
            Err(eyre!(
                "speaker id {id} out of range (this model has {} voices, 0-{})",
                VOICES.len(),
                VOICES.len() - 1
            ))
        };
    }

    VOICES
        .iter()
        .position(|v| v.eq_ignore_ascii_case(name))
        .map(|i| i as i32)
        .ok_or_else(|| {
            eyre!(
                "unknown voice {name:?}\n\navailable voices:\n{}",
                describe_all()
            )
        })
}

/// Human-readable listing for `--help` and error messages.
pub fn describe_all() -> String {
    VOICES
        .iter()
        .enumerate()
        .map(|(i, v)| format!("  {i:>2}  {v:<12} {}", describe(v)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Accent and gender implied by the name prefix.
pub fn describe(name: &str) -> &'static str {
    match name.as_bytes() {
        [b'a', b'f', ..] => "American female",
        [b'a', b'm', ..] => "American male",
        [b'b', b'f', ..] => "British female",
        [b'b', b'm', ..] => "British male",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_names_to_ids() {
        assert_eq!(resolve("af").unwrap(), 0);
        assert_eq!(resolve("am_adam").unwrap(), 5);
        assert_eq!(resolve("bf_emma").unwrap(), 7);
        assert_eq!(resolve("bm_george").unwrap(), 9);
    }

    #[test]
    fn name_lookup_is_case_and_space_insensitive() {
        assert_eq!(resolve("  BM_George ").unwrap(), 9);
    }

    #[test]
    fn accepts_a_bare_id_as_an_escape_hatch() {
        assert_eq!(resolve("9").unwrap(), 9);
        assert!(resolve("11").is_err(), "out of range must fail");
        assert!(resolve("-1").is_err());
    }

    #[test]
    fn unknown_name_lists_the_alternatives() {
        let msg = resolve("morgan_freeman").unwrap_err().to_string();
        assert!(msg.contains("unknown voice"), "got: {msg}");
        assert!(msg.contains("bm_george"), "error should list voices: {msg}");
    }

    #[test]
    fn default_voice_is_a_real_voice() {
        assert!(resolve(DEFAULT_VOICE).is_ok());
    }

    /// The table must match the shipped voices.bin, or every id is wrong.
    #[test]
    fn voice_table_matches_the_model_file() {
        let path = crate::tts::kokoro::default_model_dir().join("voices.bin");
        if !path.exists() {
            eprintln!("skipping: model not installed");
            return;
        }
        let bytes = std::fs::metadata(&path).unwrap().len() as usize;
        const PER_VOICE: usize = 511 * 256 * 4;
        assert_eq!(
            bytes / PER_VOICE,
            VOICES.len(),
            "voices.bin holds {} voices but the name table has {}",
            bytes / PER_VOICE,
            VOICES.len()
        );
    }

    #[test]
    fn describes_accent_and_gender() {
        assert_eq!(describe("bm_george"), "British male");
        assert_eq!(describe("af_sarah"), "American female");
    }
}
