
use reezy_core::{classify, epub, normalize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let book = epub::open("/home/chase/Repos/reezy/fixtures/local/east-of-eden.epub")?;
    let body: Vec<_> = book.chapters.iter()
        .filter(|c| classify::classify(c) == classify::Kind::Body).collect();
    println!("body chapters: {}", body.len());

    let mut changed = 0usize;
    let mut examples = Vec::new();
    for c in &body {
        let n = normalize::normalize(&c.text);
        if n != c.text {
            changed += 1;
            // find a differing line to show
            for (a, b) in c.text.lines().zip(n.lines()) {
                if a != b && examples.len() < 6 && a.len() < 200 {
                    examples.push((a.to_string(), b.to_string()));
                    break;
                }
            }
        }
    }
    println!("chapters modified by normalize(): {changed}/{}", body.len());
    println!("\n=== sample rewrites ===");
    for (a, b) in &examples {
        println!("  RAW : {a}");
        println!("  NORM: {b}\n");
    }

    // How many periods-after-abbreviation did we remove book-wide?
    let raw_all: String = body.iter().map(|c| c.text.as_str()).collect();
    let norm_all = normalize::normalize(&raw_all);
    for pat in ["Dr.", "Mr.", "Mrs.", "St.", " 18", " 19"] {
        println!("{pat:6} raw={:<6} norm={}", raw_all.matches(pat).count(), norm_all.matches(pat).count());
    }
    Ok(())
}
