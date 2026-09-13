use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=resources/languages");
    println!("cargo:rerun-if-changed=resources/symbols");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let languages = json_files(&manifest_dir.join("resources/languages"))?;
    let symbols = json_files(&manifest_dir.join("resources/symbols"))?;

    let mut generated = String::new();
    generated.push_str("pub const LANGUAGE_PACKS: &[(&str, &str)] = &[\n");
    for path in &languages {
        write_entry(&mut generated, path, &manifest_dir, "language");
    }
    generated.push_str("];\n\n");
    generated.push_str("pub const SYMBOL_PACKS: &[(&str, &str)] = &[\n");
    for path in &symbols {
        write_entry(&mut generated, path, &manifest_dir, "symbol");
    }
    generated.push_str("];\n");

    fs::write(out_dir.join("embedded_resources.rs"), generated)
}

fn write_entry(output: &mut String, path: &Path, manifest_dir: &Path, kind: &str) {
    let relative = path
        .strip_prefix(manifest_dir)
        .expect("resource path must be inside the manifest directory")
        .to_string_lossy()
        .replace('\\', "/");
    let absolute = path.to_string_lossy().replace('\\', "/");
    output.push_str(&format!(
        "    (\"{kind}:{relative}\", include_str!(r\"{absolute}\")),\n"
    ));
}

fn json_files(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_json_files(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_json_files(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_json_files(&entry_path, files)?;
        } else if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            files.push(entry_path);
        }
    }
    Ok(())
}
