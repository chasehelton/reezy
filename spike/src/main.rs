//! Phase 1 spike: can rbook parse a real calibre-converted EPUB 2?
//! Target: East of Eden (Steinbeck) -- 66 content docs, hierarchical NCX TOC,
//! dc:language = "UND".

use rbook::Epub;
use rbook::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "/home/chase/Repos/reezy/fixtures/local/east-of-eden.epub";
    let epub = Epub::open(path)?;

    println!("=== METADATA ===");
    let md = epub.metadata();
    println!("title    : {:?}", md.title().map(|t| t.value()));
    println!(
        "creators : {:?}",
        md.creators().map(|c| c.value()).collect::<Vec<_>>()
    );
    println!(
        "languages: {:?}",
        md.languages().map(|l| l.value()).collect::<Vec<_>>()
    );

    println!("\n=== SPINE ===");
    println!("spine entries: {}", epub.spine().len());

    println!("\n=== TOC ===");
    let toc = epub.toc();
    println!("top-level entries: {}", toc.iter().count());
    let contents = toc.contents().expect("no contents/NCX navMap");
    let all: Vec<_> = contents.flatten().collect();
    println!("flattened toc entries: {}", all.len());
    for e in all.iter().take(16) {
        println!("  depth={} label={:?}", e.depth(), e.label());
    }
    let mut by_depth = std::collections::BTreeMap::new();
    for e in &all {
        *by_depth.entry(e.depth()).or_insert(0usize) += 1;
    }
    println!("  entries by depth: {by_depth:?}");

    println!("\n=== READER: first 3 spine docs ===");
    for (i, data) in epub.reader().enumerate().take(3) {
        let data = data?;
        let content = data.content();
        let preview: String = content.chars().take(160).collect();
        println!(
            "[{i}] {} bytes | {}",
            content.len(),
            preview.replace('\n', " ")
        );
    }
    Ok(())
}
