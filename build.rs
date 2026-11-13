fn main() {
    // Positive local-build identity. A copied binary is not a verified source
    // installation, and a package beside this build never owns this process.
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).unwrap();
    println!(
        "cargo:rustc-env=HALCYON_BUILD_BINARY={}",
        profile.join("halcyon").display()
    );
    println!("cargo:rerun-if-changed=build.rs");
    built_in_looks(&out);
}

/// Embeds every `resources/looks/<id>.json` as a built-in look. The file
/// name is the look's stable ID: lowercase letters, digits and single
/// hyphens. The records themselves are checked when the app parses them
/// (and by the tests), so a bad file never ships quietly.
fn built_in_looks(out: &std::path::Path) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/looks");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut looks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".json"))
            else {
                continue;
            };
            let valid = !id.is_empty()
                && id.len() <= 48
                && !id.starts_with('-')
                && !id.ends_with('-')
                && !id.contains("--")
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            assert!(
                valid,
                "resources/looks/{id}.json: a built-in look's file name is its ID: lowercase letters, digits and hyphens"
            );
            println!("cargo:rerun-if-changed={}", path.display());
            looks.push((id.to_string(), path));
        }
    }
    looks.sort();
    let mut code = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for (id, path) in looks {
        code.push_str(&format!(
            "    ({id:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    code.push_str("];\n");
    std::fs::write(out.join("built_in_looks.rs"), code).unwrap();
}
