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
    embed(&out, "resources/looks", "built_in_looks.rs", "");
    embed(&out, "resources/themes", "built_in_themes.rs", "theme-");
}

/// Embeds every `<dir>/<id>.json`: the built-in looks in
/// `resources/looks` and the built-in themes in `resources/themes`. The file
/// name is the record's stable ID (lowercase letters, digits and single
/// hyphens); `prefix` goes in front of it (a theme's ID starts with
/// `theme-`, like the styles' own). The records themselves are checked when
/// the app parses them (and by the tests), so a bad file never ships quietly.
fn embed(out: &std::path::Path, folder: &str, file: &str, prefix: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(folder);
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut records = Vec::new();
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
                "{folder}/{id}.json: a built-in record's file name is its ID: lowercase letters, digits and hyphens"
            );
            println!("cargo:rerun-if-changed={}", path.display());
            records.push((format!("{prefix}{id}"), path));
        }
    }
    records.sort();
    let mut code = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for (id, path) in records {
        code.push_str(&format!(
            "    ({id:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    code.push_str("];\n");
    std::fs::write(out.join(file), code).unwrap();
}
