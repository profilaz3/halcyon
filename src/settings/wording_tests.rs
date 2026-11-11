//! User-facing text names pages the way the sidebar does, and never shows
//! the internal names of a change.
use std::path::Path;

/// Page names Settings no longer has, and what replaced them.
const RETIRED: &[(&str, &str)] = &[
    ("App settings", "Halcyon"),
    ("Theme and colors", "Appearance › Accent color"),
    ("Layout and displays", "Widgets › Arrange widgets"),
    ("Presets & schedules", "Looks"),
    ("Wallpaper & colors", "Wallpaper"),
    ("Floating apps", "Apps that don’t tile"),
    ("Desktop canvas", "Arrange widgets"),
    ("Dock & Panel", "Dock and panel"),
];
/// Code names that never appear on screen: the control says “Blend into
/// wallpaper”, “Arrange widgets” or “Time of day”.
const INTERNAL: &[&str] = &["Penumbra", "Canvas", "Moment", "Moments"];
/// Older records keep the labels they were saved with; these strings read
/// them back and never show them.
const READS_OLD_RECORDS: &[(&str, &str)] = &[("src/settings/support.rs", "Dock & Panel")];

#[test]
fn messages_name_current_pages_and_controls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    let mut files = Vec::new();
    collect(&root.join("src"), &mut files);
    for path in files {
        let name = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let source = std::fs::read_to_string(&path).unwrap();
        let source = match source.find("#[cfg(test)]\nmod tests") {
            Some(end) => &source[..end],
            None => &source[..],
        };
        for (line, text) in literals(source) {
            for (old, new) in RETIRED {
                if text.contains(old) && !READS_OLD_RECORDS.contains(&(name.as_str(), old)) {
                    found.push(format!("{name}:{line}: “{old}” is now “{new}”: {text}"));
                }
            }
            for word in text.split(|c: char| !c.is_alphanumeric()) {
                if INTERNAL.contains(&word) {
                    found.push(format!("{name}:{line}: internal name “{word}”: {text}"));
                }
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_scan_reads_string_literals_only() {
    let found = literals(
        "// \"Canvas\" in a comment\nlet a = 'x'; let b = '\\''; fn f<'a>() {}\n\
         let c = \"one \\\"Moment\\\"\";\n/* \"Penumbra\" */ let d = r#\"raw \"two\"\"#;",
    );
    assert_eq!(
        found,
        vec![
            (3, "one \\\"Moment\\\"".to_owned()),
            (4, "raw \"two\"".to_owned())
        ]
    );
}

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if path.is_dir() {
            if name != "tests" {
                collect(&path, files);
            }
        } else if name.ends_with(".rs")
            && !name.ends_with("tests.rs")
            && !name.ends_with("_test.rs")
        {
            files.push(path);
        }
    }
}

/// Every string literal in Rust source, with its line.
fn literals(source: &str) -> Vec<(usize, String)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let (mut at, mut line) = (0, 1);
    let ident =
        |at: usize| at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
    while at < bytes.len() {
        let rest = &source[at..];
        if bytes[at] == b'\n' {
            line += 1;
            at += 1;
        } else if rest.starts_with("//") {
            at += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            let end = rest.find("*/").map_or(rest.len(), |end| end + 2);
            line += rest[..end].matches('\n').count();
            at += end;
        } else if bytes[at] == b'r' && !ident(at) && raw_hashes(rest).is_some() {
            let hashes = raw_hashes(rest).unwrap();
            let open = 2 + hashes;
            let close = format!("\"{}", "#".repeat(hashes));
            let end = rest[open..]
                .find(&close)
                .map_or(rest.len(), |end| open + end);
            out.push((line, rest[open..end].to_owned()));
            line += rest[..end].matches('\n').count();
            at += end + close.len();
        } else if bytes[at] == b'\'' {
            // A char literal is skipped whole; a lifetime is one quote.
            let skip = if rest.starts_with("'\\") {
                rest[3..].find('\'').map_or(1, |end| end + 4)
            } else {
                match rest[1..].char_indices().nth(1) {
                    Some((end, '\'')) => end + 2,
                    _ => 1,
                }
            };
            at += skip;
        } else if bytes[at] == b'"' {
            let mut end = at + 1;
            while end < bytes.len() && bytes[end] != b'"' {
                end += if bytes[end] == b'\\' { 2 } else { 1 };
            }
            let text = &source[at + 1..end.min(bytes.len())];
            out.push((line, text.to_owned()));
            line += text.matches('\n').count();
            at = end + 1;
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

/// The `#` count of a raw string that starts here.
fn raw_hashes(rest: &str) -> Option<usize> {
    let hashes = rest[1..].bytes().take_while(|byte| *byte == b'#').count();
    (rest.as_bytes().get(1 + hashes) == Some(&b'"')).then_some(hashes)
}
