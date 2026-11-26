//! The desktop note: plain text in an owner-only file of its own.
//!
//! The note is user content, never configuration: it is not in
//! `config.json`, so profiles, exports and imports can't carry it. Saves
//! are compare-and-swap under a lock: the desktop card and Settings each
//! say which text they started from, and a save that would overwrite a
//! newer edit made elsewhere is refused with that newer text.
//!
//! Unsaved Settings edits are kept apart from the note, in a draft file of
//! their own, when Settings closes; they come back the next time it opens
//! and still reach the note only through Save.
//!
//! Lines may be checklist items (`[ ] task`, `- [x] done`); the desktop card
//! draws them as boxes that can be ticked without opening the editor.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "notes.json";
const FORMAT: &str = "desktop-cards-note";
const DRAFT_FILE: &str = "notes-draft.json";
const DRAFT_FORMAT: &str = "desktop-cards-note-draft";
/// The longest note, in bytes of text.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_FILE_BYTES: usize = 4 * MAX_TEXT_BYTES;

/// A desktop layer has no keyboard focus, so the toolkit's regular clipboard
/// silently refuses writes. wl-copy uses Wayland data control without taking
/// focus. Pass note contents through stdin, never a command argument or log.
pub async fn copy_from_desktop(text: String) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            "Copy needs wl-clipboard. You can also copy from Notes in Settings.".to_string()
        })?;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut input = child.stdin.take().ok_or("Could not copy the note")?;
        input
            .write_all(text.as_bytes())
            .await
            .map_err(|_| "Could not copy the note")?;
        drop(input);
        let status = child.wait().await.map_err(|_| "Could not copy the note")?;
        if status.success() {
            Ok(())
        } else {
            Err("Could not copy the note".to_string())
        }
    })
    .await
    .map_err(|_| "Clipboard did not respond. Try again.".to_string())?
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub text: String,
    /// When it was last saved (RFC 3339), if ever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    format: String,
    version: u32,
    #[serde(flatten)]
    note: Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The note changed elsewhere since the editor started from it.
    Conflict(Note),
    Failed(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(_) => write!(f, "This note changed elsewhere"),
            Self::Failed(message) => write!(f, "{message}"),
        }
    }
}

pub fn path() -> Option<PathBuf> {
    super::data_path(FILE)
}

/// The saved note; an absent file is an empty note.
pub fn load() -> Result<Note, String> {
    load_from(&path().ok_or("No data folder")?)
}

pub fn load_from(path: &Path) -> Result<Note, String> {
    let Some(bytes) = super::read_bounded(path, MAX_FILE_BYTES)? else {
        return Ok(Note::default());
    };
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|_| "The note file can't be read".to_string())?;
    if record.format != FORMAT || record.version != 1 {
        return Err("The note was saved by a newer version".into());
    }
    if record.note.text.len() > MAX_TEXT_BYTES {
        return Err("The note is too long".into());
    }
    Ok(record.note)
}

/// What a note shows when saving it failed with `error`: "Not saved" and
/// what to do, in plain words. The system's own words go to the log.
pub fn not_saved(error: &str) -> String {
    let reason = crate::desktop::store::message::plain_failure(error);
    if reason == crate::desktop::store::message::GENERIC {
        return "Not saved. Try again.".into();
    }
    let end = if reason.ends_with(['.', '!', '?']) {
        ""
    } else {
        "."
    };
    format!("Not saved. {reason}{end}")
}

/// Saves `text` if the note still holds `expected`.
pub fn save(expected: &str, text: &str) -> Result<Note, SaveError> {
    let path = path().ok_or_else(|| SaveError::Failed("No data folder".into()))?;
    save_at(&path, expected, text)
}

/// One caller-owned save off the UI thread. A competing writer is reported
/// promptly; it cannot occupy a blocking-pool thread indefinitely waiting for
/// the note lock. The existing compare-and-swap and durable write stay intact.
pub async fn save_async(expected: String, text: String) -> Result<Note, SaveError> {
    tokio::task::spawn_blocking(move || {
        crate::config::with_nonblocking_locks(|| save(&expected, &text))
    })
    .await
    .unwrap_or_else(|_| Err(SaveError::Failed("Saving stopped unexpectedly".into())))
}

pub fn save_at(path: &Path, expected: &str, text: &str) -> Result<Note, SaveError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(SaveError::Failed(format!(
            "Notes can be up to {} KB",
            MAX_TEXT_BYTES / 1024
        )));
    }
    let _lock = crate::config::lock_config(path).map_err(SaveError::Failed)?;
    let current = load_from(path).map_err(SaveError::Failed)?;
    // The note already says this, whatever it was edited from: nothing to
    // overwrite and nothing to write.
    if current.text == text {
        return Ok(current);
    }
    if current.text != expected {
        return Err(SaveError::Conflict(current));
    }
    let note = Note {
        text: text.to_string(),
        updated_at: Some(chrono::Local::now().to_rfc3339()),
    };
    let record = Record {
        format: FORMAT.into(),
        version: 1,
        note: note.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|e| SaveError::Failed(e.to_string()))?;
    super::write_private(path, &bytes).map_err(SaveError::Failed)?;
    Ok(note)
}

/// Unsaved edits kept while no editor is open: the text and the saved text
/// it was edited from, so a newer note saved meanwhile is noticed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub text: String,
    pub base: String,
}

#[derive(Serialize, Deserialize)]
struct DraftRecord {
    format: String,
    version: u32,
    #[serde(flatten)]
    draft: Draft,
}

pub fn draft_path() -> Option<PathBuf> {
    super::data_path(DRAFT_FILE)
}

/// The kept draft, if there is one that can be read.
pub fn load_draft() -> Option<Draft> {
    load_draft_from(&draft_path()?)
}

pub fn load_draft_from(path: &Path) -> Option<Draft> {
    let bytes = super::read_bounded(path, 2 * MAX_FILE_BYTES).ok()??;
    let record: DraftRecord = serde_json::from_slice(&bytes).ok()?;
    (record.format == DRAFT_FORMAT && record.version == 1).then_some(record.draft)
}

/// Keeps `draft` (owner-only, like the note), or forgets the kept one.
pub fn keep_draft(draft: Option<&Draft>) -> Result<(), String> {
    keep_draft_at(&draft_path().ok_or("No data folder")?, draft)
}

pub fn keep_draft_at(path: &Path, draft: Option<&Draft>) -> Result<(), String> {
    let Some(draft) = draft else {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        };
    };
    let record = DraftRecord {
        format: DRAFT_FORMAT.into(),
        version: 1,
        draft: draft.clone(),
    };
    let bytes = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
    if bytes.len() > 2 * MAX_FILE_BYTES {
        return Err(format!("Notes can be up to {} KB", MAX_TEXT_BYTES / 1024));
    }
    super::write_private(path, &bytes)
}

/// One displayed line of a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line<'a> {
    Blank,
    Heading(&'a str),
    Bullet(&'a str),
    /// A numbered item: its number as typed (`3`) and its text.
    Numbered(&'a str, &'a str),
    /// A checklist item and its line number in the note.
    Task {
        line: usize,
        done: bool,
        text: &'a str,
    },
    Text(&'a str),
}

/// `(done, text)` when `line` is a checklist item.
fn task(line: &str) -> Option<(bool, &str)> {
    let body = line.trim_start();
    let body = ["- ", "* "]
        .into_iter()
        .find_map(|bullet| body.strip_prefix(bullet))
        .unwrap_or(body);
    let (done, rest) = if let Some(rest) = body.strip_prefix("[ ]") {
        (false, rest)
    } else if let Some(rest) = body
        .strip_prefix("[x]")
        .or_else(|| body.strip_prefix("[X]"))
    {
        (true, rest)
    } else {
        return None;
    };
    (rest.is_empty() || rest.starts_with(' ')).then(|| (done, rest.trim()))
}

/// `(number, text)` for `1. item` or `1) item`.
fn numbered(line: &str) -> Option<(&str, &str)> {
    let body = line.trim_start();
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 4 {
        return None;
    }
    let (number, rest) = body.split_at(digits);
    let rest = rest
        .strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))?;
    Some((number, rest.trim()))
}

/// How a run of a line's text is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    Plain,
    Bold,
    Italic,
    Code,
}

/// A line's text in runs: `**bold**`, `*italic*` or `_italic_`, and
/// `` `code` ``. A marker without its closing pair stays as typed, and code
/// keeps any markers inside it.
pub fn spans(text: &str) -> Vec<(Emphasis, &str)> {
    let mut runs = Vec::new();
    let mut plain = 0;
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let found = [
            ("**", Emphasis::Bold),
            ("`", Emphasis::Code),
            ("*", Emphasis::Italic),
            ("_", Emphasis::Italic),
        ]
        .into_iter()
        .find_map(|(marker, emphasis)| {
            let inner = rest.strip_prefix(marker)?;
            let end = inner.find(marker)?;
            // `_` only around words, so snake_case stays as typed.
            let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric());
            if marker == "_"
                && (word(text[..at].chars().next_back()) || word(inner[end + 1..].chars().next()))
            {
                return None;
            }
            (end > 0 && !inner[..end].starts_with(' ') && !inner[..end].ends_with(' '))
                .then(|| (emphasis, &inner[..end], marker.len() * 2 + end))
        });
        match found {
            Some((emphasis, inner, taken)) => {
                if plain < at {
                    runs.push((Emphasis::Plain, &text[plain..at]));
                }
                runs.push((emphasis, inner));
                at += taken;
                plain = at;
            }
            None => at += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    if plain < text.len() {
        runs.push((Emphasis::Plain, &text[plain..]));
    }
    runs
}

/// The text a line shows, its markers gone: what is measured.
pub fn shown(text: &str) -> String {
    spans(text).into_iter().map(|(_, run)| run).collect()
}

/// A page of a note split at its top-level headings (`notes.pages`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<'a> {
    pub title: &'a str,
    /// The note's line numbers on this page, its heading excluded.
    pub lines: std::ops::Range<usize>,
}

/// The note's pages: each `# Heading` starts one; text before the first
/// heading is a page called Notes, when there is any.
pub fn pages(text: &str) -> Vec<Page<'_>> {
    let all: Vec<&str> = text.lines().collect();
    let mut pages: Vec<Page<'_>> = Vec::new();
    let mut start = 0;
    let mut title = "Notes";
    for (index, line) in all.iter().enumerate() {
        if let Some(heading) = line.strip_prefix("# ") {
            let body = start..index;
            if !pages.is_empty() || all[body.clone()].iter().any(|l| !l.trim().is_empty()) {
                pages.push(Page { title, lines: body });
            }
            title = heading.trim();
            start = index + 1;
        }
    }
    if !pages.is_empty() || title != "Notes" || all.iter().any(|l| !l.trim().is_empty()) {
        pages.push(Page {
            title,
            lines: start..all.len(),
        });
    }
    pages
}

pub fn lines(text: &str) -> Vec<Line<'_>> {
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            if let Some((done, text)) = task(line) {
                Line::Task {
                    line: index,
                    done,
                    text,
                }
            } else if line.trim().is_empty() {
                Line::Blank
            } else if let Some(heading) = line.strip_prefix("# ") {
                Line::Heading(heading.trim())
            } else if let Some(item) = line
                .trim_start()
                .strip_prefix("- ")
                .or_else(|| line.trim_start().strip_prefix("* "))
            {
                Line::Bullet(item.trim())
            } else if let Some((number, item)) = numbered(line) {
                Line::Numbered(number, item)
            } else {
                Line::Text(line.trim_end())
            }
        })
        .collect()
}

/// The note with checklist item `line` ticked or unticked; `None` when that
/// line is not an item. Only the box changes, so the rest stays exact.
pub fn toggle(text: &str, line: usize) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut found = false;
    for (index, piece) in text.split_inclusive('\n').enumerate() {
        if index == line
            && let Some((done, _)) = task(piece.trim_end_matches(['\n', '\r']))
        {
            let (from, to) = if done { ("[x]", "[ ]") } else { ("[ ]", "[x]") };
            let from = if done && !piece.contains(from) {
                "[X]"
            } else {
                from
            };
            out.push_str(&piece.replacen(from, to, 1));
            found = true;
        } else {
            out.push_str(piece);
        }
    }
    found.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_markdown_sets_runs_and_leaves_lone_markers() {
        use Emphasis::*;
        assert_eq!(
            spans("Ship **today**, *carefully*, with `cargo test`"),
            [
                (Plain, "Ship "),
                (Bold, "today"),
                (Plain, ", "),
                (Italic, "carefully"),
                (Plain, ", with "),
                (Code, "cargo test"),
            ]
        );
        assert_eq!(
            spans("a * b * c"),
            [(Plain, "a * b * c")],
            "spaced stars are not emphasis"
        );
        assert_eq!(spans("2 ** 3"), [(Plain, "2 ** 3")]);
        assert_eq!(spans("snake_case_name"), [(Plain, "snake_case_name")]);
        assert_eq!(spans("an _aside_ here")[1], (Italic, "aside"));
        assert_eq!(spans("`**not bold**`"), [(Code, "**not bold**")]);
        assert_eq!(shown("**Plan** the *week*"), "Plan the week");
        assert!(spans("").is_empty());
    }

    #[test]
    fn numbered_lines_and_pages_follow_the_text() {
        let text = "Inbox note\n# Work\n1. Ship it\n2) Review\n# Home\n[ ] Groceries\n";
        let lines = lines(text);
        assert_eq!(lines[2], Line::Numbered("1", "Ship it"));
        assert_eq!(lines[3], Line::Numbered("2", "Review"));
        assert_eq!(lines[0], Line::Text("Inbox note"));
        let pages = pages(text);
        let titles: Vec<_> = pages.iter().map(|p| p.title).collect();
        assert_eq!(titles, ["Notes", "Work", "Home"]);
        assert_eq!(pages[1].lines, 2..4);
        assert_eq!(pages[2].lines, 5..6);
        // No text before the first heading: no Notes page.
        let pages = super::pages("# Work\nA\n# Home\nB");
        assert_eq!(
            pages.iter().map(|p| p.title).collect::<Vec<_>>(),
            ["Work", "Home"]
        );
        assert_eq!(super::pages("Just text").len(), 1);
        assert!(super::pages("").is_empty());
        // `## sub` and `#tag` don't start pages.
        assert_eq!(super::pages("# A\n## sub\n#tag").len(), 1);
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("desktop-cards-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("notes.json")
    }

    #[test]
    fn saves_are_private_and_refuse_to_overwrite_a_newer_edit() {
        let path = temp("save");
        assert_eq!(load_from(&path).unwrap(), Note::default());
        let first = save_at(&path, "", "Buy milk").unwrap();
        assert_eq!(first.text, "Buy milk");
        assert!(first.updated_at.is_some());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // Settings started from the old text while the desktop saved.
        save_at(&path, "Buy milk", "Buy milk\n[ ] Call Sam").unwrap();
        match save_at(&path, "Buy milk", "Buy oat milk") {
            Err(SaveError::Conflict(current)) => assert_eq!(current.text, "Buy milk\n[ ] Call Sam"),
            other => panic!("expected a conflict, got {other:?}"),
        }
        assert_eq!(load_from(&path).unwrap().text, "Buy milk\n[ ] Call Sam");
        // An unchanged save writes nothing new.
        let before = std::fs::read(&path).unwrap();
        save_at(&path, "Buy milk\n[ ] Call Sam", "Buy milk\n[ ] Call Sam").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        // Two editors that arrive at the same text agree: no conflict, no write.
        assert_eq!(
            save_at(&path, "Buy milk", "Buy milk\n[ ] Call Sam")
                .unwrap()
                .text,
            "Buy milk\n[ ] Call Sam"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let long = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(matches!(
            save_at(&path, "Buy milk\n[ ] Call Sam", &long),
            Err(SaveError::Failed(_))
        ));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn drafts_are_kept_privately_and_forgotten() {
        let path = temp("draft").with_file_name("notes-draft.json");
        assert_eq!(load_draft_from(&path), None);
        keep_draft_at(&path, None).unwrap();
        let draft = Draft {
            text: "Buy oat milk".into(),
            base: "Buy milk".into(),
        };
        keep_draft_at(&path, Some(&draft)).unwrap();
        assert_eq!(load_draft_from(&path), Some(draft));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        keep_draft_at(&path, None).unwrap();
        assert!(!path.exists());
        // A foreign file is not a draft.
        std::fs::write(
            &path,
            br#"{"format":"other","version":1,"text":"x","base":""}"#,
        )
        .unwrap();
        assert_eq!(load_draft_from(&path), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn foreign_or_newer_files_are_not_mistaken_for_an_empty_note() {
        let path = temp("foreign");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            br#"{"format":"desktop-cards-note","version":2,"text":"x"}"#,
        )
        .unwrap();
        assert!(load_from(&path).is_err());
        // A save never replaces a file it could not read.
        assert!(matches!(save_at(&path, "", "y"), Err(SaveError::Failed(_))));
        std::fs::write(&path, b"not json").unwrap();
        assert!(load_from(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn checklists_headings_and_bullets_are_recognized() {
        let text = "# Today\n- [ ] Write report\n[x] Lunch\n* item\n\n[ ]\nplain [ ] text\n[X]done";
        assert_eq!(
            lines(text),
            [
                Line::Heading("Today"),
                Line::Task {
                    line: 1,
                    done: false,
                    text: "Write report"
                },
                Line::Task {
                    line: 2,
                    done: true,
                    text: "Lunch"
                },
                Line::Bullet("item"),
                Line::Blank,
                Line::Task {
                    line: 5,
                    done: false,
                    text: ""
                },
                Line::Text("plain [ ] text"),
                Line::Text("[X]done"),
            ]
        );
    }

    #[test]
    fn ticking_changes_only_that_box() {
        let text = "# Plan\r\n- [ ] One [ ] literal\n[X] Two\nThree";
        let ticked = toggle(text, 1).unwrap();
        assert_eq!(ticked, "# Plan\r\n- [x] One [ ] literal\n[X] Two\nThree");
        assert_eq!(
            toggle(&ticked, 2).unwrap(),
            "# Plan\r\n- [x] One [ ] literal\n[ ] Two\nThree"
        );
        assert_eq!(toggle(text, 0), None);
        assert_eq!(toggle(text, 3), None);
        assert_eq!(toggle(text, 9), None);
    }
}
