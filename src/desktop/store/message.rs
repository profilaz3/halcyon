//! Plain sentences for errors people see. `Display` stays the technical text
//! for logs and the Details line; these never show a path, a config key or
//! an operating-system code.
use super::DesktopError;
use std::io;
use std::sync::LazyLock;

/// What Settings shows when it can't say anything more useful.
pub const GENERIC: &str = "Halcyon couldn’t read or save your settings. Try again.";
const CONFLICT: &str =
    "These settings changed somewhere else since you started editing. Nothing was changed.";
const ROLLBACK_INCOMPLETE: &str = "Couldn’t save this change, and some settings couldn’t be \
     put back. Choose Restore to try again.";
const ROLLED_BACK: &str = "Couldn’t save this change, so your previous settings were put back.";

/// An error with a sentence for people and technical text for Details.
pub trait UserMessage: std::fmt::Display {
    fn user_message(&self) -> String;
}

impl UserMessage for DesktopError {
    fn user_message(&self) -> String {
        let shown = |text: &str, fallback: &str| {
            if leaks(text) { fallback } else { text }.to_string()
        };
        match self {
            Self::ReadOnly(reason) => shown(reason, "Halcyon can’t change your settings right now."),
            Self::InvalidKey(_) | Self::NotAllowed(_) => "Halcyon can’t change this setting.".into(),
            Self::InvalidValue { key: Some(_), .. } => {
                "That value isn’t allowed for this setting. Nothing was changed.".into()
            }
            Self::InvalidValue { key: None, message } => shown(
                message,
                "Part of this change isn’t allowed. Nothing was changed.",
            ),
            Self::Conflict(_) | Self::FieldConflict { .. } => CONFLICT.into(),
            Self::Overlap(_) => "Parts of this change disagree, so nothing was changed.".into(),
            Self::UndoUnavailable(reason) => {
                shown(reason, "This change can’t be undone anymore. Nothing was changed.")
            }
            Self::RecoveryPending => self.to_string(),
            Self::Io(text) => plain(text),
            Self::RolledBack(error) => match io_sentence(error) {
                Some(cause) => format!("{ROLLED_BACK} {cause}"),
                None => ROLLED_BACK.into(),
            },
            Self::RollbackIncomplete { .. } => ROLLBACK_INCOMPLETE.into(),
        }
    }
}

/// A plain sentence for technical text that only survives as a string.
pub fn plain(text: &str) -> String {
    if let Some(sentence) = io_sentence(text) {
        return sentence.into();
    }
    if text.starts_with("Changed elsewhere since you started editing") {
        return CONFLICT.into();
    }
    if text.starts_with("Could not apply changes; previous settings restored") {
        return ROLLED_BACK.into();
    }
    if text.starts_with("Could not apply changes (") {
        return ROLLBACK_INCOMPLETE.into();
    }
    // "Context: cause" is how the store reports a failed step; the cause is
    // for Details.
    if leaks(text) || text.contains(": ") {
        return GENERIC.into();
    }
    text.into()
}

/// The sentence for an operating-system error code in `text`, when there is
/// one people can act on.
pub fn io_sentence(text: &str) -> Option<&'static str> {
    let code = text
        .rsplit_once("(os error ")
        .and_then(|(_, rest)| rest.split_once(')'))
        .and_then(|(code, _)| code.parse().ok())?;
    use io::ErrorKind::*;
    Some(match io::Error::from_raw_os_error(code).kind() {
        StorageFull | QuotaExceeded => "Your disk is full. Free some space, then try again.",
        PermissionDenied => {
            "Halcyon isn’t allowed to change your settings files. Check the permissions of your \
             settings folder, then try again."
        }
        ReadOnlyFilesystem => {
            "Your settings are on a disk that can’t be changed right now. Try again once it can."
        }
        WouldBlock | ResourceBusy | TimedOut | Interrupted => {
            "Your settings are busy. Wait a moment, then try again."
        }
        NotFound => "A settings file went missing. Try again.",
        _ => return None,
    })
}

/// Whether text carries something only a developer can use: an
/// operating-system code, a path, a config key or a parser position.
pub fn leaks(text: &str) -> bool {
    static TECHNICAL: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(concat!(
            r"(?i)os error",
            r"|(^|[\s(“'\x22=])(~?/)[^\s/]+/",
            r"|(^|[\s(“'\x22=])/[A-Za-z0-9._-]+",
            r"|\b[A-Za-z][\w.-]*/v\d+/\w+",
            r"|\bline \d+|\bcolumn \d+|\b\d+:\d+\b",
        ))
        .expect("valid pattern")
    });
    TECHNICAL.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::ConfigKey;

    fn io(kind: io::ErrorKind) -> io::Error {
        // The codes behind each kind on Linux.
        let code = match kind {
            io::ErrorKind::StorageFull => 28,
            io::ErrorKind::PermissionDenied => 13,
            io::ErrorKind::ReadOnlyFilesystem => 30,
            io::ErrorKind::WouldBlock => 11,
            io::ErrorKind::ResourceBusy => 16,
            io::ErrorKind::NotFound => 2,
            io::ErrorKind::QuotaExceeded => 122,
            io::ErrorKind::TimedOut => 110,
            _ => 5,
        };
        let error = io::Error::from_raw_os_error(code);
        assert!(kind == io::ErrorKind::Other || error.kind() == kind);
        error
    }

    #[test]
    fn people_never_see_paths_keys_or_os_codes() {
        let key = ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "size").unwrap();
        let path = "/home/someone/.config/cosmic/com.system76.CosmicPanel.Dock/v1/size";
        let mut errors = vec![
            DesktopError::NotAllowed(key.clone()),
            DesktopError::InvalidKey(format!("Invalid key {key}")),
            DesktopError::InvalidValue {
                key: Some(key.clone()),
                message: "Malformed value: 1:4: Expected integer".into(),
            },
            DesktopError::InvalidValue {
                key: None,
                message: "Malformed value: 1:4: Expected integer".into(),
            },
            DesktopError::Conflict(vec![key.clone()]),
            DesktopError::FieldConflict {
                keys: vec![key.clone()],
                fields: vec!["clock.size".into()],
            },
            DesktopError::Overlap(vec![key.clone()]),
            DesktopError::UndoUnavailable(format!("Could not read {path}")),
            DesktopError::ReadOnly(format!("{path} is read-only")),
            DesktopError::RecoveryPending,
            DesktopError::Io("Recovered history is too large".into()),
            DesktopError::Io(
                "Could not read the saved history: expected value at line 1 column 1".into(),
            ),
            DesktopError::RollbackIncomplete {
                error: format!("Could not write {path}: No space left on device (os error 28)"),
                keys: vec![key.clone()],
                fields: vec!["clock.size".into()],
            },
        ];
        for kind in [
            io::ErrorKind::StorageFull,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::ReadOnlyFilesystem,
            io::ErrorKind::WouldBlock,
            io::ErrorKind::ResourceBusy,
            io::ErrorKind::NotFound,
            io::ErrorKind::QuotaExceeded,
            io::ErrorKind::TimedOut,
            io::ErrorKind::Other,
        ] {
            let error = super::super::io_error(format!("Could not write {path}"), io(kind));
            errors.push(DesktopError::RolledBack(error.to_string()));
            errors.push(error);
        }
        for error in &errors {
            let message = error.user_message();
            assert!(!leaks(&message), "{error:?} shows “{message}”");
            assert!(!message.contains("os error") && !message.contains('/'), "{message}");
            // The technical text stays for Details.
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn common_failures_say_what_to_do() {
        let write = |kind| super::super::io_error("Could not write a/b", io(kind));
        assert_eq!(
            write(io::ErrorKind::StorageFull).user_message(),
            "Your disk is full. Free some space, then try again."
        );
        assert!(
            write(io::ErrorKind::PermissionDenied)
                .user_message()
                .contains("permissions")
        );
        assert!(
            write(io::ErrorKind::WouldBlock)
                .user_message()
                .contains("Wait a moment")
        );
        assert_eq!(write(io::ErrorKind::Other).user_message(), GENERIC);
        let rolled = DesktopError::RolledBack(write(io::ErrorKind::StorageFull).to_string());
        assert_eq!(
            rolled.user_message(),
            "Couldn’t save this change, so your previous settings were put back. \
             Your disk is full. Free some space, then try again."
        );
        // Sentences that already read plainly pass through.
        for text in ["Islands is no longer installed", "The saved change is damaged."] {
            assert_eq!(plain(text), text);
        }
        assert_eq!(
            DesktopError::UndoUnavailable("No settings action to undo".into()).user_message(),
            "No settings action to undo"
        );
    }

    #[test]
    fn leak_check_finds_technical_text_only() {
        for text in [
            "Permission denied (os error 13)",
            "Could not read /home/a/.config/x",
            "Could not read ~/.config/halcyon",
            "com.system76.CosmicTk/v1/accent_palette",
            "expected `,` at line 3 column 9",
            "1:4: Expected integer",
        ] {
            assert!(leaks(text), "{text}");
        }
        for text in [
            "Your disk is full. Free some space, then try again.",
            "Choose a picture wallpaper and/or color.",
            "Settings saved at 10 / 12",
        ] {
            assert!(!leaks(text), "{text}");
        }
    }
}
