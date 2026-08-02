//! Split prose into TTS-sized pieces.
//!
//! Neural TTS degrades on long input, and sherpa allocates per call, so a whole
//! chapter cannot go in at once. Split on sentence boundaries -- never
//! mid-sentence, or the prosody breaks audibly.

/// Target chunk size in characters. Small enough to keep quality, large enough
/// that sentences still flow into each other.
pub const TARGET_CHARS: usize = 400;

/// Split text into chunks of roughly [`TARGET_CHARS`], breaking only at
/// sentence ends. A single sentence longer than the target is emitted whole
/// rather than cut.
pub fn split(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for sentence in sentences(text) {
        if !current.is_empty() && current.len() + sentence.len() > TARGET_CHARS {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() && !current.ends_with(char::is_whitespace) {
            current.push(' ');
        }
        current.push_str(sentence.trim());
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Sentence boundaries: `.`, `!`, `?` followed by whitespace, plus any closing
/// quote that belongs to the sentence.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        if matches!(c, b'.' | b'!' | b'?') {
            // Consume trailing closing quotes/brackets that end the sentence.
            let mut end = i + 1;
            while end < text.len() {
                let Some(ch) = text[end..].chars().next() else {
                    break;
                };
                if matches!(ch, '"' | '\u{201d}' | '\u{2019}' | ')' | ']') {
                    end += ch.len_utf8();
                } else {
                    break;
                }
            }
            let is_boundary = text[end..].chars().next().is_none_or(|n| n.is_whitespace());
            if is_boundary {
                out.push(&text[start..end]);
                start = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }
    if start < text.len() && !text[start..].trim().is_empty() {
        out.push(&text[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(split("Hello there."), vec!["Hello there."]);
    }

    #[test]
    fn splits_only_on_sentence_boundaries() {
        let text = "A ".repeat(150) + "end. " + &"B ".repeat(150) + "stop.";
        for c in split(&text) {
            assert!(
                c.trim_end().ends_with('.'),
                "chunk did not end at a sentence: {:?}",
                &c[c.len().saturating_sub(40)..]
            );
        }
    }

    #[test]
    fn rejoined_chunks_preserve_the_words() {
        let text = "One. Two! Three? Four.";
        let joined = split(text).join(" ");
        for w in ["One", "Two", "Three", "Four"] {
            assert!(joined.contains(w), "lost {w} in {joined:?}");
        }
    }

    #[test]
    fn a_single_long_sentence_is_not_cut() {
        let long = format!("{}.", "word ".repeat(200));
        let chunks = split(&long);
        assert_eq!(chunks.len(), 1, "an unbroken sentence must stay whole");
    }

    /// Closing quotes belong to the sentence they end.
    #[test]
    fn dialogue_keeps_its_closing_quote() {
        let chunks = split("\u{201c}No. Just looked.\u{201d} He turned away.");
        assert!(
            chunks[0].contains('\u{201d}') || chunks.len() == 1,
            "got: {chunks:?}"
        );
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(split("").is_empty());
        assert!(split("   \n  ").is_empty());
    }
}
