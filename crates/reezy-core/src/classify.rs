//! Decide which spine documents are worth narrating.
//!
//! A real book's spine is not all prose. East of Eden's 65 documents include a
//! titlepage, a copyright page, an inline table of contents, and four part
//! dividers ("Part One") that are only ~22 characters long. Narrating those
//! produces 1-2 second junk chapters scattered through the M4B chapter list.

use crate::epub::Chapter;

/// What a spine document actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Real prose. Narrate it.
    Body,
    /// Structural divider such as "Part One". Skippable; the label is worth
    /// keeping as a chapter marker but there is nothing to read aloud.
    Divider,
    /// Cover, titlepage, copyright, inline TOC, index. Skip.
    FrontMatter,
}

/// Documents shorter than this are structural, not prose.
const MIN_BODY_CHARS: usize = 300;

/// Titles that mark non-prose sections, matched case-insensitively as whole
/// titles (not substrings -- "Contents" must not match "Table of Contents of
/// the Heart", and a chapter titled "The Index of Small Things" is real prose).
const FRONT_MATTER_TITLES: &[&str] = &[
    "cover",
    "title page",
    "titlepage",
    "copyright",
    "copyright page",
    "table of contents",
    "contents",
    "index",
    "colophon",
    "about the author",
    "about the publisher",
    "also by this author",
    "front matter",
    "back matter",
    "dedication",
    "acknowledgements",
    "acknowledgments",
];

/// Phrases that only ever appear on a copyright/imprint page. Front matter is
/// often untitled (calibre gives it no TOC entry), so the title heuristic alone
/// misses it -- East of Eden's copyright page is 1371 chars of publisher
/// addresses and would otherwise have been narrated.
const IMPRINT_MARKERS: &[&str] = &[
    "all rights reserved",
    "library of congress",
    "isbn",
    "printed in the united states",
    "no part of this publication may be reproduced",
    "first published",
    "penguin books",
    "catalog card number",
];

/// How many imprint markers must appear before a section is judged an imprint
/// page. Two, because a novel may legitimately mention one in passing.
const IMPRINT_MARKER_THRESHOLD: usize = 2;

/// Classify one chapter.
pub fn classify(chapter: &Chapter) -> Kind {
    let title = chapter.title.trim().to_ascii_lowercase();
    let len = chapter.text.chars().count();

    if FRONT_MATTER_TITLES.contains(&title.as_str()) {
        return Kind::FrontMatter;
    }
    if is_divider_title(&title) && len < MIN_BODY_CHARS {
        return Kind::Divider;
    }
    if len < MIN_BODY_CHARS {
        return Kind::FrontMatter;
    }
    if looks_like_imprint(&chapter.text) {
        return Kind::FrontMatter;
    }
    Kind::Body
}

/// Detect a copyright/imprint page by its boilerplate, not its title.
fn looks_like_imprint(text: &str) -> bool {
    // Only the opening matters; a long novel could coincidentally contain these
    // phrases in its body, but an imprint page front-loads them.
    let head: String = text
        .chars()
        .take(2000)
        .collect::<String>()
        .to_ascii_lowercase();
    let hits = IMPRINT_MARKERS
        .iter()
        .filter(|m| head.contains(**m))
        .count();
    hits >= IMPRINT_MARKER_THRESHOLD
}

/// "Part One", "Part 2", "Book Three" -- a structural divider heading.
fn is_divider_title(title: &str) -> bool {
    let mut words = title.split_whitespace();
    matches!(words.next(), Some("part" | "book" | "volume")) && words.next().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(title: &str, text: &str) -> Chapter {
        Chapter {
            index: 0,
            title: title.into(),
            text: text.into(),
        }
    }

    fn prose() -> String {
        "The Salinas Valley is in Northern California. ".repeat(20)
    }

    #[test]
    fn real_chapters_are_body() {
        assert_eq!(classify(&ch("Chapter 1", &prose())), Kind::Body);
        assert_eq!(
            classify(&ch("Letter to Pascal Covici", &prose())),
            Kind::Body
        );
    }

    #[test]
    fn part_dividers_are_dividers() {
        assert_eq!(classify(&ch("Part One", "Part One")), Kind::Divider);
        assert_eq!(classify(&ch("Book Two", "")), Kind::Divider);
    }

    #[test]
    fn front_matter_titles_are_skipped_even_when_long() {
        // An inline TOC can be long; the title is what damns it.
        assert_eq!(
            classify(&ch("Table of Contents", &prose())),
            Kind::FrontMatter
        );
        assert_eq!(classify(&ch("Copyright", &prose())), Kind::FrontMatter);
    }

    #[test]
    fn short_untitled_stubs_are_front_matter() {
        assert_eq!(
            classify(&ch("Untitled section 1", "EAST")),
            Kind::FrontMatter
        );
    }

    /// A divider label must not swallow a real chapter that happens to be long.
    #[test]
    fn long_part_sections_are_body_not_divider() {
        assert_eq!(classify(&ch("Part One", &prose())), Kind::Body);
    }

    #[test]
    fn untitled_copyright_pages_are_front_matter() {
        let imprint = "PENGUIN BOOKS. First published in the United States of \
             America by The Viking Press 1952. Copyright 1952 by John Steinbeck. \
             All rights reserved. ISBN 0-14-004997-8. "
            .repeat(3);
        assert_eq!(
            classify(&ch("Untitled section 3", &imprint)),
            Kind::FrontMatter
        );
    }

    /// A novel that mentions one such phrase in its prose is still prose.
    #[test]
    fn single_incidental_marker_does_not_condemn_a_chapter() {
        let text = format!(
            "{} He said all rights reserved, and laughed. {}",
            prose(),
            prose()
        );
        assert_eq!(classify(&ch("Chapter 9", &text)), Kind::Body);
    }

    /// Substring matching would wrongly skip these.
    #[test]
    fn front_matter_matching_is_whole_title_not_substring() {
        assert_eq!(
            classify(&ch("The Index of Small Things", &prose())),
            Kind::Body
        );
        assert_eq!(classify(&ch("Contents of the Heart", &prose())), Kind::Body);
    }

    /// Ground truth against a real book: East of Eden must yield exactly its
    /// 55 numbered chapters plus the Covici letter as Body, and must classify
    /// all five "Part N" dividers and the front matter away.
    #[test]
    fn classifies_east_of_eden_correctly() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/local/east-of-eden.epub");
        if !p.exists() {
            eprintln!("skipping: fixtures/local/east-of-eden.epub not present");
            return;
        }
        let book = crate::epub::open(p).expect("parse");

        let body: Vec<&Chapter> = book
            .chapters
            .iter()
            .filter(|c| classify(c) == Kind::Body)
            .collect();
        let dividers: Vec<&str> = book
            .chapters
            .iter()
            .filter(|c| classify(c) == Kind::Divider)
            .map(|c| c.title.as_str())
            .collect();

        assert_eq!(
            dividers,
            [
                "Part One",
                "Part Two",
                "Part Three",
                "Part Four",
                "Part Five"
            ],
            "all five part dividers must be detected"
        );

        // 55 numbered chapters + "Letter to Pascal Covici".
        assert_eq!(
            body.len(),
            56,
            "body sections: {:?}",
            body.iter().map(|c| &c.title).collect::<Vec<_>>()
        );

        // Nothing tiny survives into the narration set.
        for c in &body {
            assert!(
                c.text.chars().count() >= MIN_BODY_CHARS,
                "{:?} is only {} chars",
                c.title,
                c.text.chars().count()
            );
        }
        // The inline TOC must not be narrated.
        assert!(!body.iter().any(|c| c.title == "Table of Contents"));
    }
}
