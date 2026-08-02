//! EPUB parsing: spine, table of contents, and metadata.

use eyre::Result;
use std::path::Path;

/// A parsed ebook, reduced to what narration needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub title: String,
    pub author: Option<String>,
    /// BCP-47 language tag. `None` when the EPUB declares something
    /// unusable such as `UND` (see East of Eden).
    pub language: Option<String>,
    pub chapters: Vec<Chapter>,
}

/// One narratable unit of a book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    /// Position in reading order, 0-based.
    pub index: usize,
    pub title: String,
    /// Plain text, HTML already stripped.
    pub text: String,
}

/// Language tags that carry no usable information.
const UNUSABLE_LANGS: &[&str] = &["und", "unknown", "xx", ""];

/// Parse an EPUB from disk.
pub fn open(path: impl AsRef<Path>) -> Result<Book> {
    let epub = rbook::Epub::open(path.as_ref())
        .map_err(|e| eyre::eyre!("failed to open {}: {e}", path.as_ref().display()))?;
    let md = epub.metadata();

    let title = md
        .title()
        .map(|t| t.value().to_owned())
        .unwrap_or_else(|| "Untitled".to_owned());

    let author = md.creators().next().map(|c| c.value().to_owned());

    // `UND` and friends are worse than nothing: they would silently drive
    // the wrong espeak voice. Normalize them away.
    let language = md
        .languages()
        .next()
        .map(|l| l.value().to_owned())
        .filter(|l| !UNUSABLE_LANGS.contains(&l.to_ascii_lowercase().as_str()));

    // TOC entries are keyed by the spine document they point at, so titles can
    // be matched by href rather than by position. East of Eden has 65 spine
    // documents but only 61 TOC entries, so a fallback is mandatory.
    let toc_titles = collect_toc_titles(&epub);

    let mut chapters = Vec::new();
    for (index, item) in epub.reader().enumerate() {
        let item = item.map_err(|e| eyre::eyre!("failed to read spine item {index}: {e}"))?;
        let href = item.manifest_entry().href().as_ref().to_string();
        let raw = item.content();

        // Fallback order: TOC label -> first heading -> a NON-colliding
        // placeholder. Never fabricate "Chapter N": untitled front matter would
        // then collide with the book's real "Chapter N" TOC labels, and
        // lookups by title would silently hit the wrong document.
        let title = toc_titles
            .get(strip_fragment(&href))
            .cloned()
            .or_else(|| heading_from_html(raw))
            .unwrap_or_else(|| format!("Untitled section {}", index + 1));

        chapters.push(Chapter {
            index,
            title,
            text: html_to_text(raw),
        });
    }

    Ok(Book {
        title,
        author,
        language,
        chapters,
    })
}

/// Map spine href -> TOC label.
///
/// The TOC is walked via `contents().flatten()`; `toc().iter()` yields only the
/// top-level entries (2 for East of Eden, vs 61 flattened).
fn collect_toc_titles(epub: &rbook::Epub) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Some(contents) = epub.toc().contents() {
        for entry in contents.flatten() {
            let Some(manifest) = entry.manifest_entry() else {
                continue;
            };
            let label = entry.label().trim().to_owned();
            if !label.is_empty() {
                map.entry(strip_fragment(manifest.href().as_ref()).to_owned())
                    .or_insert(label);
            }
        }
    }
    map
}

/// Normalize an href for matching: drop any `#fragment` and leading `/` or
/// `./` so TOC and spine hrefs compare equal regardless of how the producer
/// wrote them (calibre emits absolute TOC hrefs, relative spine hrefs).
fn strip_fragment(href: &str) -> &str {
    let no_frag = href.split('#').next().unwrap_or(href);
    no_frag
        .strip_prefix("./")
        .unwrap_or(no_frag)
        .strip_prefix('/')
        .unwrap_or_else(|| no_frag.strip_prefix("./").unwrap_or(no_frag))
}

/// First `<h1>`..`<h3>` in a document, used when the TOC has no entry for it.
fn heading_from_html(html: &str) -> Option<String> {
    for tag in ["h1", "h2", "h3"] {
        let open = format!("<{tag}");
        if let Some(start) = html.find(&open) {
            let after = html[start..].find('>')? + start + 1;
            let end = html[after..].find(&format!("</{tag}>"))? + after;
            let text = html_to_text(&html[after..end]);
            if !text.trim().is_empty() {
                return Some(text.trim().to_owned());
            }
        }
    }
    None
}

/// Strip markup to plain text, preserving paragraph breaks and dropping
/// `<sup>` footnote markers, `<script>`, and `<style>` outright.
fn html_to_text(html: &str) -> String {
    // NOTE: index by byte offsets found via `find`, never by fixed-width slices.
    // Ebook prose is full of multi-byte characters (curly quotes, em dashes),
    // and slicing at an arbitrary offset panics on a char boundary.
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;

    'outer: while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = &rest[lt..];

        // Elements whose contents are dropped entirely.
        for (open, close) in [
            ("<script", "</script>"),
            ("<style", "</style>"),
            ("<sup", "</sup>"),
        ] {
            if starts_with_tag(rest, open) {
                rest = match find_ci(rest, close) {
                    Some(end) => &rest[end + close.len()..],
                    None => "",
                };
                continue 'outer;
            }
        }

        // Block-level boundaries become paragraph breaks.
        for tag in [
            "</p", "<br", "</div", "</h1", "</h2", "</h3", "</li", "</tr",
        ] {
            if starts_with_tag(rest, tag) {
                out.push('\n');
                break;
            }
        }

        rest = match rest.find('>') {
            Some(end) => &rest[end + 1..],
            None => "",
        };
    }
    out.push_str(rest);

    decode_entities(&out)
}

/// Case-insensitive `starts_with` for a tag prefix, without allocating the
/// whole remainder or slicing at a fixed width.
fn starts_with_tag(haystack: &str, tag: &str) -> bool {
    haystack.len() >= tag.len()
        && haystack.as_bytes()[..tag.len()].eq_ignore_ascii_case(tag.as_bytes())
}

/// Case-insensitive `find`.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

/// Decode the handful of entities that actually appear in ebook prose.
fn decode_entities(s: &str) -> String {
    let mut out = s
        .replace("&nbsp;", " ")
        .replace("&mdash;", "\u{2014}")
        .replace("&ndash;", "\u{2013}")
        .replace("&lsquo;", "\u{2018}")
        .replace("&rsquo;", "\u{2019}")
        .replace("&ldquo;", "\u{201c}")
        .replace("&rdquo;", "\u{201d}")
        .replace("&hellip;", "\u{2026}")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "\u{2019}")
        .replace("&amp;", "&");

    // Collapse runs of blank lines and trailing spaces left by tag removal.
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Path to the gitignored real-book fixture. Absent on a clean clone,
    /// so every test using it must skip rather than fail.
    fn east_of_eden() -> Option<std::path::PathBuf> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/local/east-of-eden.epub");
        p.exists().then(|| p.canonicalize().unwrap())
    }

    #[test]
    fn parses_metadata_from_a_real_epub2() {
        let Some(path) = east_of_eden() else {
            eprintln!("skipping: fixtures/local/east-of-eden.epub not present");
            return;
        };
        let book = open(path).expect("should parse East of Eden");
        assert_eq!(book.title, "East of Eden");
        assert_eq!(book.author.as_deref(), Some("John Steinbeck"));
        // dc:language is "UND" -- undetermined must normalize to None, not "UND".
        assert_eq!(book.language, None);
    }

    #[test]
    fn extracts_chapters_with_toc_titles() {
        let Some(path) = east_of_eden() else {
            eprintln!("skipping: fixtures/local/east-of-eden.epub not present");
            return;
        };
        let book = open(path).expect("should parse East of Eden");

        // Spine is 65 docs; every one becomes a chapter at this stage.
        assert_eq!(book.chapters.len(), 65, "one chapter per spine document");

        // Indices are dense and in reading order.
        for (i, ch) in book.chapters.iter().enumerate() {
            assert_eq!(ch.index, i);
        }

        // TOC titles are matched onto the right spine documents.
        let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.contains(&"Chapter 1"), "got: {titles:?}");
        assert!(titles.contains(&"Part One"));
        assert!(titles.contains(&"Letter to Pascal Covici"));

        // HTML must be gone.
        let body = book
            .chapters
            .iter()
            .map(|c| c.text.as_str())
            .collect::<String>();
        assert!(!body.contains("<html"), "html tags leaked into text");
        assert!(!body.contains("<p>"), "html tags leaked into text");

        // A real chapter has real prose in it.
        let ch1 = book
            .chapters
            .iter()
            .find(|c| c.title == "Chapter 1")
            .unwrap();
        assert!(
            ch1.text.len() > 500,
            "Chapter 1 text was {} bytes",
            ch1.text.len()
        );
        assert!(ch1.text.contains("Salinas Valley"), "expected opening line");
    }

    /// Regression: untitled front matter must not be labelled "Chapter N".
    /// The titlepage is spine[0]; naming it "Chapter 1" collided with the real
    /// Chapter 1 further in, so `find(title == "Chapter 1")` hit the titlepage.
    #[test]
    fn untitled_sections_do_not_collide_with_real_chapter_numbers() {
        let Some(path) = east_of_eden() else {
            eprintln!("skipping: fixtures/local/east-of-eden.epub not present");
            return;
        };
        let book = open(path).expect("should parse East of Eden");

        let named_ch1: Vec<usize> = book
            .chapters
            .iter()
            .filter(|c| c.title == "Chapter 1")
            .map(|c| c.index)
            .collect();
        assert_eq!(named_ch1.len(), 1, "exactly one chapter may be 'Chapter 1'");

        // And it must be the one with the prose, not the titlepage.
        let ch1 = &book.chapters[named_ch1[0]];
        assert!(ch1.text.contains("Salinas Valley"));
    }

    /// Regression: byte-slicing HTML panics on multi-byte characters.
    /// Real ebook prose is full of curly quotes and em dashes.
    #[test]
    fn html_stripping_survives_multibyte_characters() {
        let html = "<p>\u{201c}Don\u{2019}t,\u{201d} he said\u{2014}quietly.</p><sup>1</sup>";
        let text = html_to_text(html);
        assert!(text.contains("Don\u{2019}t"), "got: {text:?}");
        assert!(text.contains("said\u{2014}quietly"), "got: {text:?}");
        assert!(!text.contains('1'), "sup footnote marker should be dropped");
    }

    #[test]
    fn strips_scripts_styles_and_decodes_entities() {
        let html = "<head><style>p{color:red}</style></head><body>\
                    <p>Tom &amp; Jerry &mdash; &ldquo;hi&rdquo;</p>\
                    <script>alert(1)</script></body>";
        let text = html_to_text(html);
        assert!(text.contains("Tom & Jerry"), "got: {text:?}");
        assert!(text.contains('\u{2014}'), "got: {text:?}");
        assert!(!text.contains("color:red"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn href_matching_ignores_fragments_and_leading_slash() {
        assert_eq!(strip_fragment("/ch1.html"), "ch1.html");
        assert_eq!(strip_fragment("./ch1.html"), "ch1.html");
        assert_eq!(strip_fragment("ch1.html#part2"), "ch1.html");
        assert_eq!(strip_fragment("/ch1.html#x"), "ch1.html");
    }

    #[test]
    fn unusable_language_tags_normalize_to_none() {
        // Guards the UND case without needing the fixture.
        for tag in ["UND", "und", "unknown", ""] {
            assert!(UNUSABLE_LANGS.contains(&tag.to_ascii_lowercase().as_str()));
        }
        assert!(!UNUSABLE_LANGS.contains(&"en-us"));
    }
}
