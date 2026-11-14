//! One installation contract. IPC proves a running host, never installation.
use super::*;
use serde::Deserialize;
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Installation {
    #[default]
    Pending,
    Absent,
    Installed {
        provenance: Provenance,
        enabled: bool,
        compatible: Option<bool>,
        status: Option<CompatibilityStatus>,
        diagnostics: String,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Package,
    Manual,
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum CompatibilityStatus {
    #[serde(rename = "Ready")]
    Ready,
    #[serde(rename = "Log out to activate")]
    LogoutRequired,
    #[serde(rename = "Using standard panel")]
    Standard,
    #[serde(rename = "Compatibility update needed")]
    UpdateNeeded,
    #[serde(rename = "Checking compatibility")]
    Checking,
    #[serde(rename = "Islands disabled")]
    Disabled,
}
impl CompatibilityStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::LogoutRequired => "Log out to activate",
            Self::Standard => "Using standard panel",
            Self::UpdateNeeded => "Compatibility update needed",
            Self::Checking => "Checking compatibility",
            Self::Disabled => "Islands disabled",
        }
    }
}
#[derive(Deserialize)]
struct Status {
    format: String,
    version: u32,
    installation: String,
    provenance: Option<Provenance>,
    enabled: bool,
    compatible: Option<bool>,
    #[serde(default)]
    status_version: Option<u32>,
    #[serde(default)]
    status: Option<CompatibilityStatus>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    installed_generation: Option<String>,
    #[serde(default)]
    active_generation: Option<String>,
    #[serde(default)]
    package_versions: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    installed_cohort: Option<String>,
    #[serde(default)]
    running_profiles: Vec<String>,
    #[serde(default)]
    qualification_cohort: Option<String>,
    #[serde(default)]
    runtime_targets: Vec<String>,
    #[serde(default)]
    applet_issues: Vec<AppletIssue>,
}
#[derive(Deserialize, serde::Serialize)]
struct AppletIssue {
    applet: String,
    reason: String,
}
fn applet_problem(reason: &str) -> Option<&'static str> {
    Some(match reason {
        "desktop_entry_missing" => "Applet is not installed",
        "desktop_exec_missing" => "Applet has no launch command",
        "desktop_exec_invalid" | "desktop_entry_invalid" => "Applet launcher is invalid",
        "desktop_exec_unresolved" => "Applet launch path could not be checked",
        "applet_executable_missing" => "Applet program is missing or not executable",
        "applet_not_observed" => "Applet process not observed; it may still be starting",
        _ => return None,
    })
}
thread_local! {
    static NATIVE_VIEW: std::cell::RefCell<Option<Installation>> = const { std::cell::RefCell::new(None) };
}
pub struct NativeView(Option<Installation>);
impl Drop for NativeView {
    fn drop(&mut self) {
        NATIVE_VIEW.with(|v| {
            v.replace(self.0.take());
        });
    }
}
impl Installation {
    /// Native read-only planning consumes the last background result. A write
    /// must run outside this context and re-discover under transaction locks.
    pub fn native_view(state: Self) -> NativeView {
        NativeView(NATIVE_VIEW.with(|v| v.replace(Some(state))))
    }
    pub fn on_native_thread() -> bool {
        NATIVE_VIEW.with(|v| v.borrow().is_some())
    }
    pub fn needs_worker(store: &DesktopStore) -> bool {
        match store.fixture_root() {
            None => true,
            Some(_root) => {
                #[cfg(test)]
                {
                    // Execution policy is explicit in a fixture; provider probe
                    // contents must not select a different Settings lifecycle.
                    return _root.join("native-worker-enabled").is_file();
                }
                #[cfg(not(test))]
                {
                    false
                }
            }
        }
    }
    pub(crate) fn for_write(store: &DesktopStore) -> Self {
        if Self::on_native_thread() && Self::needs_worker(store) {
            // Fail closed if a future UI path bypasses the background executor.
            Self::Pending
        } else {
            Self::discover(store)
        }
    }
    pub fn installed(&self) -> bool {
        matches!(self, Self::Installed { .. })
    }
    /// The Islands panel is enabled, compatible and the one running now
    /// (not the standard panel it falls back to).
    pub fn running(&self) -> bool {
        matches!(
            self,
            Self::Installed {
                enabled: true,
                compatible: Some(true) | None,
                status: Some(CompatibilityStatus::Ready),
                ..
            }
        )
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn status_label(&self) -> &'static str {
        match self {
            Self::Installed {
                status: Some(status),
                ..
            } => status.label(),
            Self::Absent => "Islands not installed",
            _ => "Checking compatibility",
        }
    }
    pub fn diagnostics(&self) -> Option<&str> {
        match self {
            Self::Installed { diagnostics, .. } => Some(diagnostics),
            _ => None,
        }
    }
    /// Separate optional applet problems from the companion's availability.
    /// Only validated IDs and known codes leave the diagnostics boundary.
    pub fn applet_messages(&self) -> Vec<String> {
        let Some(value) = self
            .diagnostics()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        else {
            return Vec::new();
        };
        value["applet_issues"]
            .as_array()
            .into_iter()
            .flatten()
            .take(64)
            .filter_map(|issue| {
                let text = applet_problem(issue["reason"].as_str()?)?;
                Some(format!("{text}: {}", issue["applet"].as_str()?))
            })
            .collect()
    }
    fn parse(bytes: &[u8]) -> Self {
        if bytes.len() > 65536 {
            return Self::Pending;
        }
        let Ok(status) = serde_json::from_slice::<Status>(bytes) else {
            return Self::Pending;
        };
        if status.format != "halcyon-islands-installation" || status.version != 1 {
            return Self::Pending;
        }
        if status.status_version.is_some_and(|v| v != 1 && v != 2)
            || (status.status.is_some() && status.status_version.is_none())
        {
            return Self::Pending;
        }
        let safe_hash = |s: &Option<String>| {
            s.as_ref()
                .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
                .cloned()
        };
        let reason = status
            .reason
            .as_ref()
            .filter(|v| v.len() <= 80 && v.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
        let packages: std::collections::BTreeMap<_, _> = status
            .package_versions
            .iter()
            .filter(|(name, value)| {
                matches!(
                    name.as_str(),
                    "cosmic-panel" | "cosmic-comp" | "cosmic-session" | "cosmic-settings-daemon"
                ) && value.len() <= 128
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".+:~_-".contains(&b))
            })
            .collect();
        let running_profiles: Vec<_> = status
            .running_profiles
            .iter()
            .take(64)
            .filter_map(|value| safe_hash(&Some(value.clone())))
            .collect();
        let applet_issues: Vec<_> = status
            .applet_issues
            .iter()
            .take(64)
            .filter(|issue| {
                !issue.applet.is_empty()
                    && issue.applet.len() <= 200
                    && issue
                        .applet
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    && applet_problem(&issue.reason).is_some()
            })
            .collect();
        let diagnostics = serde_json::json!({
            "version": 1, "enabled": status.enabled, "compatible": status.compatible,
            "status": status.status.map(CompatibilityStatus::label), "reason": reason,
            "installed_generation": safe_hash(&status.installed_generation),
            "active_generation": safe_hash(&status.active_generation), "package_versions": packages,
            "installed_cohort": safe_hash(&status.installed_cohort), "running_profiles": running_profiles,
            "qualification_cohort": safe_hash(&status.qualification_cohort),
            "runtime_targets": status.runtime_targets.iter().take(64).filter_map(|v| safe_hash(&Some(v.clone()))).collect::<Vec<_>>(),
            "applet_issues": applet_issues,
        })
        .to_string();
        match (status.installation.as_str(), status.provenance) {
            ("absent", None) => Self::Absent,
            ("installed", Some(provenance)) => Self::Installed {
                provenance,
                enabled: status.enabled,
                compatible: status.compatible,
                status: status.status,
                diagnostics,
            },
            _ => Self::Pending,
        }
    }
    pub fn checking(&self) -> Self {
        match self {
            Self::Installed {
                provenance,
                enabled,
                ..
            } => Self::Installed {
                provenance: *provenance,
                enabled: *enabled,
                compatible: None,
                status: Some(CompatibilityStatus::Checking),
                diagnostics: r#"{"version":1,"reason":"verification_pending"}"#.into(),
            },
            _ => self.clone(),
        }
    }
    pub fn needs_recheck(&self) -> bool {
        matches!(
            self,
            Self::Pending
                | Self::Installed {
                    status: Some(CompatibilityStatus::Checking),
                    ..
                }
        )
    }
    pub fn discover_presence_until(store: &DesktopStore, deadline: std::time::Instant) -> Self {
        Self::discover_mode(store, true, deadline)
    }
    pub fn discover(store: &DesktopStore) -> Self {
        Self::discover_until(store, std::time::Instant::now() + Duration::from_secs(6))
    }
    pub fn discover_until(store: &DesktopStore, deadline: std::time::Instant) -> Self {
        Self::discover_mode(store, false, deadline)
    }
    fn discover_mode(store: &DesktopStore, presence: bool, deadline: std::time::Instant) -> Self {
        if Self::needs_worker(store) {
            if let Some(state) = NATIVE_VIEW.with(|v| v.borrow().clone()) {
                return state;
            }
        }
        if let Some(root) = store.fixture_root() {
            #[cfg(test)]
            if root.join("status-islands-probe.py").is_file() {
                return Self::run_helper(
                    &root.join("status-islands-probe.py"),
                    presence,
                    deadline.saturating_duration_since(std::time::Instant::now()),
                );
            }
            #[cfg(test)]
            if root.join("slow-islands-probe.py").is_file() {
                let output = std::process::Command::new("/usr/bin/python3")
                    .arg("-B")
                    .arg(root.join("slow-islands-probe.py"))
                    .output()
                    .unwrap();
                return Self::parse(&output.stdout);
            }
            return match crate::desktop::recovery::read_path(
                &root.join("islands-installation.json"),
                65536,
            ) {
                Ok(Some((bytes, _))) => Self::parse(&bytes),
                Ok(None) => Self::Absent,
                Err(_) => Self::Pending,
            };
        }
        if std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
            return Self::Pending;
        }
        let helper = std::path::Path::new("/usr/lib/cosmic-ext-halcyon/islands-status.py");
        if !helper.is_file() {
            return Self::Absent;
        }
        Self::run_helper(
            helper,
            presence,
            deadline.saturating_duration_since(std::time::Instant::now()),
        )
    }
    fn run_helper(helper: &std::path::Path, presence: bool, limit: Duration) -> Self {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        let started = std::time::Instant::now();
        if limit.is_zero() {
            return Self::Pending;
        }
        let Ok(mut child) = std::process::Command::new("/usr/bin/python3")
            .args([
                "-B",
                helper.to_str().unwrap(),
                if presence { "--presence" } else { "--progress" },
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return Self::Pending;
        };
        let mut pipe = child.stdout.take().unwrap();
        // No blocking reader thread or inherited pipe can extend the outer deadline.
        if unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } == -1 {
            let _ = child.kill();
            let _ = child.wait();
            return Self::Pending;
        }
        let mut buffered = Vec::new();
        let mut total = 0;
        let mut latest = Self::Pending;
        loop {
            let mut chunk = [0u8; 4096];
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    total += n;
                    if total > 131072 {
                        latest = latest.checking();
                        break;
                    }
                    buffered.extend_from_slice(&chunk[..n]);
                    while let Some(end) = buffered.iter().position(|b| *b == b'\n') {
                        let parsed = Self::parse(&buffered[..end]);
                        if !matches!(parsed, Self::Pending) {
                            latest = parsed;
                        }
                        buffered.drain(..=end);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => {
                    latest = latest.checking();
                    break;
                }
            }
            if started.elapsed() >= limit {
                latest = latest.checking();
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !buffered.is_empty() {
            let parsed = Self::parse(&buffered);
            if !matches!(parsed, Self::Pending) {
                latest = parsed;
            }
        }
        let success = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if started.elapsed() < limit => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                _ => break false,
            }
        };
        let _ = child.kill();
        let _ = child.wait();
        if success { latest } else { latest.checking() }
    }
}
#[cfg(test)]
pub(crate) fn fixture_installed(root: &std::path::Path) {
    std::fs::write(root.join("islands-installation.json"), br#"{"format":"halcyon-islands-installation","version":1,"installation":"installed","provenance":"manual","enabled":false,"compatible":false}"#).unwrap();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_boundary_progress_timeout_retains_installed_without_ready_claim() {
        let f = crate::desktop::store::tests::Fixture::new("runtime-boundary-timeout");
        let helper = f.0.join("helper.py");
        std::fs::write(&helper, r#"import json,time
print(json.dumps(dict(format='halcyon-islands-installation',version=1,installation='installed',provenance='package',enabled=True,compatible=None,status_version=2,status='Checking compatibility')),flush=True)
time.sleep(20)
"#).unwrap();
        let start = std::time::Instant::now();
        let status = Installation::run_helper(&helper, false, Duration::from_millis(200));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(status.installed());
        assert_eq!(status.status_label(), "Checking compatibility");
        assert!(status.needs_recheck());
        assert!(matches!(
            status,
            Installation::Installed {
                compatible: None,
                ..
            }
        ));
    }
    #[test]
    fn configured_applet_diagnostics_do_not_change_ready_and_exclude_paths() {
        let data = serde_json::json!({"format":"halcyon-islands-installation", "version":1,
        "installation":"installed", "provenance":"package", "enabled":true,
        "compatible":true, "status_version":2, "status":"Ready",
        "applet_issues":[
            {"applet":"net.tropicbliss.CosmicExtAppletCaffeine", "reason":"desktop_entry_missing"},
            {"applet":"/home/private/token", "reason":"desktop_entry_missing"},
            {"applet":"org.example.Bad", "reason":"/private/command"}
        ]});
        let parsed = Installation::parse(&serde_json::to_vec(&data).unwrap());
        assert_eq!(parsed.status_label(), "Ready");
        assert_eq!(
            parsed.applet_messages(),
            vec!["Applet is not installed: net.tropicbliss.CosmicExtAppletCaffeine"]
        );
        assert!(!parsed.diagnostics().unwrap().contains("private"));
        let mut many = data;
        many["applet_issues"] = serde_json::json!(
            (0..100)
                .map(|i| serde_json::json!({
                    "applet":format!("org.example.Applet{i}"), "reason":"applet_not_observed"
                }))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            Installation::parse(&serde_json::to_vec(&many).unwrap())
                .applet_messages()
                .len(),
            64
        );
    }
    #[test]
    fn runtime_boundary_v2_scope_and_disabled_are_truthful() {
        let data = serde_json::json!({"format":"halcyon-islands-installation","version":1,
            "installation":"installed","provenance":"package","enabled":false,
            "compatible":null,"status_version":2,"status":"Islands disabled",
            "qualification_cohort":"a".repeat(64),"runtime_targets":["b".repeat(64),"/private/key"]});
        let parsed = Installation::parse(&serde_json::to_vec(&data).unwrap());
        assert!(parsed.installed());
        assert_eq!(parsed.status_label(), "Islands disabled");
        let diagnostic: serde_json::Value =
            serde_json::from_str(parsed.diagnostics().unwrap()).unwrap();
        assert_eq!(diagnostic["qualification_cohort"], "a".repeat(64));
        assert_eq!(
            diagnostic["runtime_targets"],
            serde_json::json!(["b".repeat(64)])
        );
        assert!(diagnostic["installed_cohort"].is_null());
        assert!(!parsed.diagnostics().unwrap().contains("private"));
    }
    #[test]
    fn task3_status_is_versioned_and_copy_diagnostics_excludes_untrusted_paths() {
        let data = br#"{"format":"halcyon-islands-installation","version":1,"installation":"installed","provenance":"package","enabled":true,"compatible":true,"status_version":1,"status":"Log out to activate","reason":"installed_running_differ","installed_generation":"/home/private/token","package_versions":{"cosmic-panel":"1.11.0~24.04~ca20ce3","private":"/home/private"}}"#;
        let status = Installation::parse(data);
        assert_eq!(status.status_label(), "Log out to activate");
        let diagnostic = status.diagnostics().unwrap();
        assert!(!diagnostic.contains("/home"));
        assert!(!diagnostic.contains("private"));
        assert!(diagnostic.contains("ca20ce3"));
        let unknown = String::from_utf8(data.to_vec())
            .unwrap()
            .replace("status_version\":1", "status_version\":99");
        assert_eq!(
            Installation::parse(unknown.as_bytes()),
            Installation::Pending
        );
    }
    #[test]
    fn task3_runtime_profiles_are_sanitized_cached_diagnostics_not_a_running_lock() {
        let data = serde_json::json!({
            "format": "halcyon-islands-installation", "version": 1,
            "installation": "installed", "provenance": "package", "enabled": true,
            "compatible": true, "status_version": 1, "status": "Ready",
            "installed_cohort": "a".repeat(64),
            "running_profiles": ["b".repeat(64), "/home/private/token".to_string()],
        });
        let parsed = Installation::parse(&serde_json::to_vec(&data).unwrap());
        let diagnostics: serde_json::Value =
            serde_json::from_str(parsed.diagnostics().unwrap()).unwrap();
        assert_eq!(diagnostics["installed_cohort"], "a".repeat(64));
        assert_eq!(
            diagnostics["running_profiles"],
            serde_json::json!(["b".repeat(64)])
        );
        assert!(diagnostics.get("running_cohort").is_none());
        assert!(!parsed.diagnostics().unwrap().contains("private"));
    }
    #[test]
    fn task2_installed_disabled_incompatible_and_unknown_are_distinct() {
        let f = crate::desktop::store::tests::Fixture::new("installed-contract");
        assert_eq!(Installation::discover(&f.store()), Installation::Absent);
        fixture_installed(&f.0);
        assert!(matches!(
            Installation::discover(&f.store()),
            Installation::Installed {
                enabled: false,
                compatible: Some(false),
                ..
            }
        ));
        std::fs::write(f.0.join("islands-installation.json"), b"{broken").unwrap();
        assert_eq!(Installation::discover(&f.store()), Installation::Pending);
        std::fs::remove_file(f.0.join("islands-installation.json")).unwrap();
        f.user(NAMESPACE, "Dock", "(enabled:true)");
        assert_eq!(Installation::discover(&f.store()), Installation::Absent);
    }
}
