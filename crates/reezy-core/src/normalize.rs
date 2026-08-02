//! Text normalization: make written English safe for a TTS engine.
//!
//! Two jobs, both validated in Phase 0 (see BENCHMARKS.md Finding 2):
//! 1. Pronunciation: `1904` -> `nineteen oh four`
//! 2. Sentence-boundary protection: `Dr.` -> `Doctor`, so sherpa's sentence
//!    splitter does not insert a spurious end-of-sentence pause mid-sentence.

/// Normalize a chunk of prose for TTS.
///
/// Currently a passthrough; Phase 1 Task 1.5 implements this test-first.
pub fn normalize(text: &str) -> String {
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_for_now() {
        assert_eq!(normalize("hello"), "hello");
    }
}
