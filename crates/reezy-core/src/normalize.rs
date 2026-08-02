//! Make written English safe to hand to a TTS engine.
//!
//! Two jobs, both identified by listening to real Kokoro output (BENCHMARKS.md
//! Finding 2):
//!
//! 1. **Pronunciation.** Kokoro reads `1904` as "nineteen hundred four".
//!    Audiobook English wants "nineteen oh four".
//! 2. **Sentence-boundary protection.** sherpa-onnx splits sentences on `.`,
//!    so `Dr.` ends a sentence and Kokoro applies a falling end-of-sentence
//!    prosody plus a pause -- mid-sentence. Expanding the abbreviation removes
//!    the period, and with it the spurious pause. Measured: 0.5-1.0s reclaimed
//!    per affected sentence.

use regex::Regex;
use std::sync::LazyLock;

/// Normalize prose for narration.
pub fn normalize(text: &str) -> String {
    let s = expand_abbreviations(text);
    expand_numbers(&s)
}

// ---------------------------------------------------------------- abbreviations

static RE_ST_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bSt\.\s+([A-Z])").unwrap());
static RE_ST_STREET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bSt\.").unwrap());
static RE_AMPM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([ap])\.\s?m\.").unwrap());
/// `No.` means "Number" ONLY before a digit ("No. 5"). In a novel it is
/// overwhelmingly the word "no" ending a sentence -- East of Eden has 330 of
/// them, and rewriting those to "Number" produced lines like
/// "Number Just looked at the city."
static RE_NUMBER_SIGN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bNo\.\s+(\d)").unwrap());

/// Title and common abbreviations, longest-first so `Mrs.` is tried before
/// `Mr.` and neither leaves a stray period behind.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("Mrs.", "Missus"),
    ("Mr.", "Mister"),
    ("Ms.", "Miz"),
    ("Dr.", "Doctor"),
    ("Prof.", "Professor"),
    ("Rev.", "Reverend"),
    ("Hon.", "Honorable"),
    ("Sgt.", "Sergeant"),
    ("Capt.", "Captain"),
    ("Lt.", "Lieutenant"),
    ("Col.", "Colonel"),
    ("Gen.", "General"),
    ("Jr.", "Junior"),
    ("Sr.", "Senior"),
    ("vs.", "versus"),
    ("etc.", "etcetera"),
    ("e.g.", "for example"),
    ("i.e.", "that is"),
    ("Ave.", "Avenue"),
    ("Rd.", "Road"),
    ("Blvd.", "Boulevard"),
    ("Mt.", "Mount"),
];

fn expand_abbreviations(text: &str) -> String {
    // `St.` is ambiguous: "St. John" is Saint, "Elm St." is Street. Resolve by
    // what follows -- a capitalized word means it is a name.
    let mut out = RE_NUMBER_SIGN.replace_all(text, "Number $1").into_owned();
    out = RE_ST_NAME.replace_all(&out, "Saint $1").into_owned();
    out = RE_ST_STREET.replace_all(&out, "Street").into_owned();
    out = RE_AMPM
        .replace_all(
            &out,
            |c: &regex::Captures| {
                if &c[1] == "a" { "AM" } else { "PM" }
            },
        )
        .into_owned();

    for (from, to) in ABBREVIATIONS {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

// ---------------------------------------------------------------------- numbers

static RE_YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(1[0-9]{3}|20[0-9]{2})\b").unwrap());

fn expand_numbers(text: &str) -> String {
    RE_YEAR
        .replace_all(text, |c: &regex::Captures| {
            say_year(c[0].parse().expect("regex guarantees digits"))
        })
        .into_owned()
}

/// Read a year the way a narrator would.
///
/// 1904 -> "nineteen oh four", 1987 -> "nineteen eighty seven",
/// 1900 -> "nineteen hundred", 2005 -> "two thousand five".
fn say_year(y: u32) -> String {
    let (hi, lo) = (y / 100, y % 100);

    // 2000-2009 read as "two thousand N", not "twenty oh N".
    if (2000..=2009).contains(&y) {
        return if lo == 0 {
            "two thousand".into()
        } else {
            format!("two thousand {}", say_below_100(lo))
        };
    }

    match lo {
        0 => format!("{} hundred", say_below_100(hi)),
        1..=9 => format!("{} oh {}", say_below_100(hi), say_below_100(lo)),
        _ => format!("{} {}", say_below_100(hi), say_below_100(lo)),
    }
}

const ONES: &[&str] = &[
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: &[&str] = &[
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

fn say_below_100(n: u32) -> String {
    match n {
        0..=19 => ONES[n as usize].to_owned(),
        _ => {
            let (t, o) = (n / 10, n % 10);
            if o == 0 {
                TENS[t as usize].to_owned()
            } else {
                format!("{} {}", TENS[t as usize], ONES[o as usize])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn years_read_like_a_narrator() {
        assert_eq!(say_year(1904), "nineteen oh four");
        assert_eq!(say_year(1987), "nineteen eighty seven");
        assert_eq!(say_year(1900), "nineteen hundred");
        assert_eq!(say_year(1066), "ten sixty six");
        assert_eq!(say_year(2005), "two thousand five");
        assert_eq!(say_year(2000), "two thousand");
        assert_eq!(say_year(2019), "twenty nineteen");
    }

    /// The exact defect heard in the Phase 0 listen test.
    #[test]
    fn fixes_the_1904_defect() {
        assert_eq!(
            normalize("In the winter of 1904, he arrived."),
            "In the winter of nineteen oh four, he arrived."
        );
    }

    /// The exact pause defect: no period may survive an abbreviation.
    #[test]
    fn abbreviations_leave_no_period_to_split_on() {
        let out = normalize("In 1904, Dr. Renwick met Mrs. Ash at 4 p.m.");
        assert!(!out.contains("Dr."), "got: {out}");
        assert!(!out.contains("Mrs."), "got: {out}");
        assert!(!out.contains("p.m."), "got: {out}");
        assert!(out.contains("Doctor Renwick"), "got: {out}");
        assert!(out.contains("Missus Ash"), "got: {out}");
        assert!(out.contains("PM"), "got: {out}");
    }

    /// `St.` means Saint before a name and Street after one.
    #[test]
    fn st_disambiguates_saint_from_street() {
        assert_eq!(normalize("St. John waited."), "Saint John waited.");
        assert_eq!(normalize("He lived on Elm St."), "He lived on Elm Street");
    }

    /// Mrs. must win over Mr. -- otherwise "Mrs." becomes "Misters."
    #[test]
    fn longest_abbreviation_wins() {
        let out = normalize("Mrs. Ash and Mr. Vane");
        assert_eq!(out, "Missus Ash and Mister Vane");
    }

    /// Regression: "No." as dialogue must stay the word "no".
    /// East of Eden has 330 of them; rewriting produced
    /// "Number Just looked at the city."
    #[test]
    fn no_as_dialogue_is_not_rewritten_to_number() {
        assert_eq!(normalize("\u{201c}No.\u{201d}"), "\u{201c}No.\u{201d}");
        assert_eq!(
            normalize("\u{201c}No. Just looked at the city.\u{201d}"),
            "\u{201c}No. Just looked at the city.\u{201d}"
        );
        // But a real numbered reference still expands.
        assert_eq!(normalize("See No. 5 below."), "See Number 5 below.");
    }

    #[test]
    fn ordinary_prose_is_untouched() {
        let s = "The Salinas Valley is in Northern California.";
        assert_eq!(normalize(s), s);
    }

    /// Page numbers and small counts are not years and must not be reworded.
    #[test]
    fn non_year_numbers_are_left_alone() {
        assert_eq!(normalize("He was 42 years old."), "He was 42 years old.");
        assert_eq!(normalize("Chapter 12"), "Chapter 12");
    }
}
