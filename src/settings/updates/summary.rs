//! One presentation decision from the already-measured package and runtime state.
use super::{Check, Identity, UpdateChannel, identity::IslandsMatch};
use crate::desktop::islands::Installation;

pub(in crate::settings) struct Summary {
    pub title: String,
    pub detail: String,
    pub system_action: bool,
}
impl Summary {
    pub fn new(identity: &Identity, check: &Check, islands: &Installation) -> Self {
        let result = |title: &str, detail: String, system_action| Self {
            title: title.into(),
            detail,
            system_action,
        };
        if identity
            .installed_version
            .as_ref()
            .is_some_and(|v| *v != identity.running_version)
        {
            return result(
                "Update installed",
                format!(
                    "Settings is still running {}. Close and reopen it to use {}.",
                    identity.running_version,
                    identity.installed_version.as_deref().unwrap()
                ),
                false,
            );
        }
        if matches!(
            identity.islands_match,
            IslandsMatch::VersionMismatch | IslandsMatch::HelperMismatch
        ) {
            return result("Islands needs attention",identity.islands_activation_notice().unwrap_or("Update or reinstall Halcyon and Islands together in your system package manager.").into(),true);
        }
        if super::pending_islands(islands) {
            return result("Islands is ready for your next login","Save your work, then log out and back in when convenient. The current panel keeps running until then.".into(),false);
        }
        match check {
            Check::Checking=>result("Checking system updates…","Your desktop’s package service is checking the configured sources.".into(),false),
            Check::Failed(error)=>result("Update check failed",error.label().into(),false),
            Check::Available(version)=>result(&format!("Update {version} available"),if identity.islands_version.is_some() {"Halcyon and Islands update together in your system package manager."} else {"Install the update in your system package manager. Islands remains optional."}.into(),true),
            Check::Current=>result("No updates found",if identity.islands_version.is_some() {"Halcyon and Islands have no newer matching packages in the configured sources."} else {"Halcyon has no newer package in the configured sources."}.into(),false),
            Check::NotChecked=> match identity.channel {
                UpdateChannel::Apt=>result("System updates are set up","Your system package manager updates Halcyon and any installed Islands package.".into(),false),
                UpdateChannel::ManualDeb=>result("Manual installation",identity.reason.into(),false),
                UpdateChannel::Source=>result("Local development build",identity.reason.into(),false),
                UpdateChannel::Unknown=>result("Installation needs checking",identity.reason.into(),false),
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::islands::{CompatibilityStatus, Provenance};
    fn identity() -> Identity {
        Identity {
            channel: UpdateChannel::Apt,
            running_version: "0.4.1".into(),
            installed_version: Some("0.4.1".into()),
            islands_version: Some("0.4.1".into()),
            islands_match: IslandsMatch::Ready,
            ..Default::default()
        }
    }
    #[test]
    fn unified_updates_prioritize_replaced_app_mismatch_and_pending_login() {
        let mut id = identity();
        let pending = Installation::Installed {
            provenance: Provenance::Package,
            enabled: true,
            compatible: Some(true),
            status: Some(CompatibilityStatus::LogoutRequired),
            diagnostics: String::new(),
        };
        // pending_islands requires validated generation evidence; an unproven label cannot qualify it.
        let current = Summary::new(&id, &Check::Current, &Installation::Absent);
        assert_eq!(current.title, "No updates found");
        id.installed_version = Some("0.5.0".into());
        assert_eq!(
            Summary::new(&id, &Check::Current, &pending).title,
            "Update installed"
        );
        id.installed_version = Some("0.4.1".into());
        id.islands_match = IslandsMatch::VersionMismatch;
        assert!(Summary::new(&id, &Check::Current, &pending).system_action);
        assert_eq!(
            Summary::new(&id, &Check::Current, &pending).title,
            "Islands needs attention"
        );
        id.islands_match = IslandsMatch::Absent;
        id.islands_version = None;
        assert!(
            Summary::new(
                &id,
                &Check::Available("0.5.0".into()),
                &Installation::Absent
            )
            .detail
            .contains("optional")
        );
        assert_eq!(
            Summary::new(
                &id,
                &Check::Failed(super::super::Failure::Offline),
                &Installation::Absent
            )
            .title,
            "Update check failed"
        );
    }
}
