
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let book = reezy_core::epub::open("/home/chase/Repos/reezy/fixtures/local/east-of-eden.epub")?;
    println!("chapters={}", book.chapters.len());
    for c in book.chapters.iter().take(12) {
        println!("[{}] {} bytes | title={:?}", c.index, c.text.len(), c.title);
    }
    Ok(())
}
