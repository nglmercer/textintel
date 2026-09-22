//! Store commands: `index`, `search`.

use textintel::cli::ParsedArgs;

use super::common::{build_engine, print_json};

pub fn index(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let store = parsed
        .positional(0)
        .ok_or("index requires <store.json> <id> <text>")?;
    let id = parsed
        .positional(1)
        .ok_or("index requires <store.json> <id> <text>")?;
    let text = parsed.rest(2).join(" ");
    if text.trim().is_empty() {
        return Err("index requires <store.json> <id> <text>".into());
    }
    let indexed = build_engine(parsed)?.with_json_store(store)?;
    indexed.add_document(id, &text)?;
    println!("indexed {}", id);
    Ok(0)
}

pub fn search(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let store = parsed
        .positional(0)
        .ok_or("search requires <store.json> <text> <limit>")?;
    let text = parsed
        .positional(1)
        .ok_or("search requires <store.json> <text> <limit>")?;
    let limit = parsed
        .positional(2)
        .ok_or("search requires <store.json> <text> <limit>")?
        .parse::<usize>()?;
    let indexed = build_engine(parsed)?.with_json_store(store)?;
    let results = indexed.find_similar(text, limit)?;
    if parsed.flag("json") {
        print_json(&results)?;
    } else {
        for result in results {
            println!("{:.3}\t{}", result.score, result.id);
        }
    }
    Ok(0)
}
