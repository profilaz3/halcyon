//! Sidecar access uses the same journal and conflict checks as native keys.
use super::panels::PanelKind;
use super::store::{ConfigKey, DesktopStore, KeyRule, Namespace, RawValue};
use dock_islands_core::{Capabilities, MAX_WIRE_BYTES, NAMESPACE, Request, Response, Settings};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::{
    fs::{FileTypeExt, MetadataExt},
    net::UnixStream,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn key(kind: PanelKind) -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, kind.entry()).expect("fixed island key")
}
fn validate(raw: &RawValue) -> Result<(), String> {
    if raw.as_str().len() > MAX_WIRE_BYTES {
        return Err("Island settings are too large".into());
    }
    raw.decode::<Settings>()?.validate()
}
pub static RULES: [KeyRule; 2] = [
    KeyRule {
        namespace: Namespace::Exact(NAMESPACE),
        version: 1,
        key: "Dock",
        validate,
    },
    KeyRule {
        namespace: Namespace::Exact(NAMESPACE),
        version: 1,
        key: "Panel",
        validate,
    },
];

#[derive(Debug, PartialEq)]
enum InactiveReason {
    NotInstalled,
    Disabled,
    CosmicChanged,
    Failed,
    NotRunning,
}
impl InactiveReason {
    fn message(&self) -> &'static str {
        match self {
            Self::NotInstalled => {
                "Install and enable the Islands add-on to use this style. The standard dock and panel remain available."
            }
            Self::Disabled => {
                "Islands is disabled. Run halcyon-islands enable, then log out and back in."
            }
            Self::CosmicChanged => {
                "Islands needs an update for this COSMIC version. Install the matching add-on, run halcyon-islands enable, then log out and back in. Your saved layout is kept."
            }
            Self::Failed => {
                "Islands stopped unexpectedly, so the standard panel was selected. See the Islands troubleshooting guide before enabling it again."
            }
            Self::NotRunning => {
                "Islands is not running. Startup checks may have selected the standard panel. After enabling or updating the add-on, log out and back in; check the Islands status and session log if it remains unavailable."
            }
        }
    }
}

fn read_record(path: &std::path::Path) -> Option<serde_json::Value> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= 65536)
        .then(|| serde_json::from_slice(&bytes).ok())
        .flatten()
}

// Explanatory mirror of the selector's reviewed-source-v1 policy. This never
// activates a host; ABI/global checks remain the launcher's responsibility.
fn source_revision<'a>(package: &str, version: &'a str) -> Option<&'a str> {
    static FORMAT: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let captures = FORMAT
        .get_or_init(|| {
            regex::Regex::new(r"^(\d+(?:\.\d+)*)~\d+~[0-9.]+~([0-9a-f]{7,40})$")
                .expect("fixed package pattern")
        })
        .captures(version)?;
    let release: Vec<u64> = captures[1]
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if package != "cosmic-comp" && release.as_slice() < [1, 10].as_slice() {
        return None;
    }
    Some(captures.get(2)?.as_str())
}

fn inactive_reason(
    bundle: &std::path::Path,
    state: &std::path::Path,
    installed_version: impl Fn(&str) -> Option<String>,
) -> InactiveReason {
    if !bundle.join("manifest.json").is_file() || !bundle.join("cosmic-panel-islands").is_file() {
        return InactiveReason::NotInstalled;
    }
    let selection = read_record(&bundle.join("selection.json"));
    if selection.as_ref().and_then(|v| v["enabled"].as_bool()) == Some(false) {
        return InactiveReason::Disabled;
    }
    if state.join("failed").is_file() {
        return InactiveReason::Failed;
    }
    if let Some(manifest) = read_record(&bundle.join("manifest.json")) {
        // New add-ons advertise tested combinations. Older manifests retain
        // their single combination; missing evidence must not invent an upgrade.
        let source_policy =
            manifest["source_policy"] == "reviewed-source-v1" && manifest["runtime_probe"] == 1;
        let legacy = &manifest["packages"];
        let candidates = manifest["supported_packages"]
            .as_array()
            .filter(|rows| !rows.is_empty())
            .map(Vec::as_slice)
            .unwrap_or_else(|| std::slice::from_ref(legacy));
        let versions = ["cosmic-panel", "cosmic-comp", "cosmic-session"]
            .map(|name| (name, installed_version(name)));
        if candidates.iter().all(|candidate| {
            versions.iter().any(|(name, actual)| {
                candidate[*name]
                    .as_str()
                    .zip(actual.as_deref())
                    .is_some_and(|(expected, actual)| {
                        expected != actual
                            && !(source_policy
                                && source_revision(name, expected)
                                    .zip(source_revision(name, actual))
                                    .is_some_and(|(a, b)| a == b))
                    })
            })
        }) {
            return InactiveReason::CosmicChanged;
        }
    }
    InactiveReason::NotRunning
}

fn inactive_message(store: &DesktopStore) -> String {
    if store.fixture_root().is_some() || std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
        return "The isolated Islands panel component is not active".into();
    }
    let Some(data) = dirs::data_dir() else {
        return InactiveReason::NotRunning.message().into();
    };
    let Some(state) = dirs::state_dir() else {
        return InactiveReason::NotRunning.message().into();
    };
    inactive_reason(
        &data.join("cosmic-desktop-cards/companion"),
        &state.join("desktop-cards-panel"),
        |package| {
            let result = std::process::Command::new("/usr/bin/dpkg-query")
                .args(["-W", "-f=${Version}", package])
                .output()
                .ok()?;
            if !result.status.success() {
                return None;
            }
            let version = String::from_utf8(result.stdout).ok()?.trim().to_string();
            (!version.is_empty()).then_some(version)
        },
    )
    .message()
    .into()
}

pub struct Client {
    stream: BufReader<UnixStream>,
    read_only_deadline: Option<Instant>,
}
impl Client {
    pub fn connect(store: &DesktopStore) -> Result<Self, String> {
        if Installation::on_native_thread() && Installation::needs_worker(store) {
            return Err("Islands runtime status is refreshed in the background".into());
        }
        if !Installation::discover(store).installed() {
            return Err(
                "Islands is not installed or its installation could not be verified".into(),
            );
        }
        let path = Self::socket_path(store)?;
        Self::connect_at(&path, || inactive_message(store))
    }

    fn socket_path(store: &DesktopStore) -> Result<PathBuf, String> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .ok_or("No desktop session")?;
        let path = if store.fixture_root().is_some() {
            let path = std::env::var_os("DESKTOP_CARDS_ISLANDS_SOCKET")
                .map(PathBuf::from)
                .ok_or("No isolated panel component")?;
            if path.parent() != Some(runtime.as_path()) {
                return Err(
                    "The isolated panel socket must be in the isolated runtime directory".into(),
                );
            }
            path
        } else if std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
            return Err("An isolated desktop root is needed for panel previews".into());
        } else {
            runtime.join("desktop-cards-islands-v1.sock")
        };
        Ok(path)
    }

    fn connect_at(path: &std::path::Path, inactive: impl Fn() -> String) -> Result<Self, String> {
        let meta = std::fs::symlink_metadata(path).map_err(|_| inactive())?;
        if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
            return Err("The panel socket is not owned by this user".into());
        }
        let stream = UnixStream::connect(path).map_err(|error| match error.kind() {
            std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound => inactive(),
            _ => "The Islands panel component is not responding".into(),
        })?;
        stream
            .set_read_timeout(Some(Duration::from_millis(750)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_millis(750)))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            stream: BufReader::new(stream),
            read_only_deadline: None,
        })
    }
    /// Reuse only this operation's installation observation for read-only IPC.
    /// Requests that can alter a lease or preferences are rejected before IO.
    pub fn connect_observed(
        store: &DesktopStore,
        observed: &Installation,
        deadline: Instant,
    ) -> Result<Self, String> {
        if Installation::on_native_thread() || !observed.installed() || observed.needs_recheck() {
            return Err("Islands compatibility is still being checked".into());
        }
        Self::connect_read_only_at(&Self::socket_path(store)?, deadline)
    }

    fn connect_read_only_at(path: &std::path::Path, deadline: Instant) -> Result<Self, String> {
        use std::os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        };
        if Instant::now() >= deadline {
            return Err("Islands status timed out".into());
        }
        let meta = std::fs::symlink_metadata(path).map_err(|_| "Islands is not running")?;
        if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
            return Err("The panel socket is not owned by this user".into());
        }
        let bytes = path.as_os_str().as_bytes();
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
            return Err("Invalid panel socket path".into());
        }
        address.sun_family = libc::AF_UNIX as _;
        for (to, from) in address.sun_path.iter_mut().zip(bytes) {
            *to = *from as _;
        }
        let fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
            )
        };
        if fd < 0 {
            return Err("Cannot open panel socket".into());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        // AF_UNIX nonblocking connect either completes immediately or refuses a
        // full listen queue. No blocking connect or diagnostic package query.
        if unsafe {
            libc::connect(
                fd.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                std::mem::size_of_val(&address) as _,
            )
        } != 0
        {
            return Err("Islands is not responding".into());
        }
        Ok(Self {
            stream: BufReader::new(UnixStream::from(fd)),
            read_only_deadline: Some(deadline),
        })
    }

    fn read_only_exchange(&mut self, bytes: &[u8], deadline: Instant) -> Result<Vec<u8>, String> {
        // The socket is nonblocking. One absolute operation budget includes all
        // requests and trickled replies, rather than resetting per read byte.
        let mut sent = 0;
        let mut data = Vec::new();
        loop {
            if Instant::now() >= deadline {
                return Err("Islands status timed out".into());
            }
            if sent < bytes.len() {
                match self.stream.get_mut().write(&bytes[sent..]) {
                    Ok(0) => return Err("Islands disconnected".into()),
                    Ok(n) => {
                        sent += n;
                        continue;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Err("Islands disconnected".into()),
                }
            } else {
                // A bounded chunk avoids BufRead losing a partial reply on EAGAIN.
                let mut chunk = [0; 4096];
                match self.stream.get_mut().read(&mut chunk) {
                    Ok(0) => return Err("Islands disconnected".into()),
                    Ok(n) => {
                        data.extend_from_slice(&chunk[..n]);
                        if data.len() > MAX_WIRE_BYTES {
                            return Err("Invalid panel reply".into());
                        }
                        if data.contains(&b'\n') {
                            return Ok(data);
                        }
                        continue;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Err("Islands disconnected".into()),
                }
            }
            std::thread::sleep(
                Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    pub fn request(&mut self, request: &Request) -> Result<Response, String> {
        if self.read_only_deadline.is_some()
            && !matches!(
                request,
                Request::Capabilities | Request::MaterialSupport | Request::Inspect { .. }
            )
        {
            return Err("Read-only status cannot authorize a panel change".into());
        }
        let mut bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if bytes.len() > MAX_WIRE_BYTES {
            return Err("Island request is too large".into());
        }
        let data = self.exchange(&bytes)?;
        match serde_json::from_slice(&data).map_err(|_| "Invalid panel reply")? {
            Response::Error(error) => Err(error),
            response => Ok(response),
        }
    }

    /// Penumbra is an independent optional extension, not an Islands capability.
    /// An older host returns its ordinary unknown-request error and stays usable.
    pub fn penumbra_supported(&mut self) -> bool {
        self.exchange(b"\"PenumbraSupport\"\n")
            .ok()
            .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
            .is_some_and(|reply| reply["PenumbraSupport"]["version"] == 1)
    }

    fn exchange(&mut self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        let data = if let Some(deadline) = self.read_only_deadline {
            self.read_only_exchange(bytes, deadline)?
        } else {
            self.stream
                .get_mut()
                .write_all(bytes)
                .map_err(|_| "The Islands panel component disconnected")?;
            let mut data = Vec::new();
            self.stream
                .by_ref()
                .take((MAX_WIRE_BYTES + 1) as u64)
                .read_until(b'\n', &mut data)
                .map_err(|_| "The panel did not acknowledge the change")?;
            data
        };
        if data.is_empty() {
            return Err("The Islands panel component disconnected".into());
        }
        if data.len() > MAX_WIRE_BYTES || !data.ends_with(b"\n") {
            return Err("Invalid panel reply".into());
        }
        Ok(data)
    }
    pub fn capabilities(&mut self) -> Result<Capabilities, String> {
        match self.request(&Request::Capabilities)? {
            Response::Capabilities(cap) if cap.compatible() => Ok(cap),
            _ => Err("The panel component needs a compatible Dock Islands version".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{
        panels::{PanelDraft, Setting},
        store::{Owner, tests::Fixture},
    };
    #[test]
    fn penumbra_probe_is_independent_and_old_hosts_keep_their_protocol() {
        for (reply, supported) in [
            ("{\"PenumbraSupport\":{\"version\":1}}\n", true),
            ("{\"PenumbraSupport\":{\"version\":2}}\n", false),
            ("{\"Error\":\"Invalid islands request\"}\n", false),
        ] {
            let (stream, mut server) = UnixStream::pair().unwrap();
            stream.set_nonblocking(true).unwrap();
            let worker = std::thread::spawn(move || {
                let mut reader = BufReader::new(server.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line, "\"PenumbraSupport\"\n");
                server.write_all(reply.as_bytes()).unwrap();
                line.clear();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line, "\"Capabilities\"\n");
                let mut reply =
                    serde_json::to_vec(&Response::Capabilities(Capabilities::default())).unwrap();
                reply.push(b'\n');
                server.write_all(&reply).unwrap();
            });
            let mut client = Client {
                stream: BufReader::new(stream),
                read_only_deadline: Some(Instant::now() + Duration::from_secs(2)),
            };
            assert_eq!(client.penumbra_supported(), supported);
            assert!(client.capabilities().is_ok());
            worker.join().unwrap();
        }
    }
    #[test]
    fn observed_status_is_read_only_and_trickle_cannot_extend_deadline() {
        let fixture = Fixture::new("islands-read-only-deadline");
        let path = fixture.0.join("panel.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert!(matches!(
                serde_json::from_str::<Request>(&request).unwrap(),
                Request::MaterialSupport
            ));
            let mut response =
                serde_json::to_vec(&Response::MaterialSupport { version: 1 }).unwrap();
            response.push(b'\n');
            stream.write_all(&response).unwrap();
            request.clear();
            reader.read_line(&mut request).unwrap();
            assert!(matches!(
                serde_json::from_str::<Request>(&request).unwrap(),
                Request::Capabilities
            ));
            for _ in 0..50 {
                if stream.write_all(b" ").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        });
        let started = Instant::now();
        let mut client =
            Client::connect_read_only_at(&path, started + Duration::from_millis(150)).unwrap();
        assert!(matches!(
            client.request(&Request::MaterialSupport),
            Ok(Response::MaterialSupport { version: 1 })
        ));
        assert!(
            client
                .request(&Request::Cancel { token: 1 })
                .unwrap_err()
                .contains("Read-only")
        );
        assert!(client.capabilities().unwrap_err().contains("timed out"));
        assert!(started.elapsed() < Duration::from_millis(500));
        drop(client);
        server.join().unwrap();
        assert!(Client::connect_read_only_at(&path, Instant::now()).is_err());
    }

    #[test]
    fn stale_socket_reports_failure_and_respects_isolation() {
        let fixture = Fixture::new("islands-stale-socket");
        let socket = fixture.0.join("panel.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        drop(listener);
        assert!(
            std::fs::symlink_metadata(&socket)
                .unwrap()
                .file_type()
                .is_socket()
        );
        let bundle = fixture.0.join("bundle");
        let state = fixture.0.join("state");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(bundle.join("manifest.json"), "{}").unwrap();
        std::fs::write(bundle.join("cosmic-panel-islands"), "fixture").unwrap();
        std::fs::write(state.join("failed"), "crashed").unwrap();
        let result = Client::connect_at(&socket, || {
            inactive_reason(&bundle, &state, |_| {
                panic!("failure needs no package query")
            })
            .message()
            .into()
        });
        assert_eq!(result.err().unwrap(), InactiveReason::Failed.message());
        assert_eq!(
            Client::connect_at(&socket, || inactive_message(&fixture.store()))
                .err()
                .unwrap(),
            "The isolated Islands panel component is not active"
        );
        std::fs::remove_file(state.join("failed")).unwrap();
        assert_eq!(
            Client::connect_at(&socket, || inactive_reason(&bundle, &state, |_| None)
                .message()
                .into())
            .err()
            .unwrap(),
            InactiveReason::NotRunning.message()
        );
    }

    #[test]
    fn invalid_socket_is_rejected_before_diagnosis() {
        let fixture = Fixture::new("islands-invalid-socket");
        let socket = fixture.0.join("panel.sock");
        std::fs::write(&socket, "not a socket").unwrap();
        assert_eq!(
            Client::connect_at(&socket, || panic!("invalid socket must not diagnose"))
                .err()
                .unwrap(),
            "The panel socket is not owned by this user"
        );
    }

    #[test]
    fn inactive_status_distinguishes_upgrade_from_disabled_and_failure() {
        let fixture = Fixture::new("islands-status");
        let bundle = fixture.0.join("bundle");
        let state = fixture.0.join("state");
        assert_eq!(
            inactive_reason(&bundle, &state, |_| None),
            InactiveReason::NotInstalled
        );
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("cosmic-panel-islands"), "fixture").unwrap();
        std::fs::write(
            bundle.join("manifest.json"),
            r#"{"packages":{"cosmic-panel":"1.11","cosmic-comp":"2","cosmic-session":"3"}}"#,
        )
        .unwrap();
        std::fs::write(bundle.join("selection.json"), r#"{"enabled":false}"#).unwrap();
        assert_eq!(
            inactive_reason(&bundle, &state, |_| panic!(
                "disabled must not query packages"
            )),
            InactiveReason::Disabled
        );
        std::fs::write(bundle.join("selection.json"), r#"{"enabled":true}"#).unwrap();
        for changed in ["cosmic-panel", "cosmic-comp", "cosmic-session"] {
            let status = inactive_reason(&bundle, &state, |name| {
                Some(
                    if name == changed {
                        "new"
                    } else {
                        match name {
                            "cosmic-panel" => "1.11",
                            "cosmic-comp" => "2",
                            _ => "3",
                        }
                    }
                    .into(),
                )
            });
            assert_eq!(status, InactiveReason::CosmicChanged);
        }
        assert_eq!(
            inactive_reason(&bundle, &state, |name| Some(
                match name {
                    "cosmic-panel" => "1.11",
                    "cosmic-comp" => "2",
                    _ => "3",
                }
                .into()
            )),
            InactiveReason::NotRunning
        );
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("failed"), "previous crash").unwrap();
        assert_eq!(
            inactive_reason(&bundle, &state, |_| panic!("failure has priority")),
            InactiveReason::Failed
        );
    }

    #[test]
    fn source_policy_diagnoses_rebuilds_without_claiming_unknown_revisions() {
        let fixture = Fixture::new("islands-source-policy");
        let bundle = &fixture.0;
        std::fs::write(bundle.join("cosmic-panel-islands"), "fixture").unwrap();
        std::fs::write(
            bundle.join("manifest.json"),
            r#"{
            "source_policy":"reviewed-source-v1", "runtime_probe":1,
            "supported_packages":[{"cosmic-panel":"1.11.0~1~24.04~ca20ce3",
                "cosmic-comp":"0.1~1~24.04~8cb1802", "cosmic-session":"1.11.0~1~24.04~4b0a0a2"}]
        }"#,
        )
        .unwrap();
        let actual = |name: &str| {
            Some(
                match name {
                    "cosmic-panel" => "1.11.0~2~26.04~ca20ce3",
                    "cosmic-comp" => "0.1~2~26.04~8cb1802",
                    _ => "1.11.0~2~26.04~4b0a0a2",
                }
                .into(),
            )
        };
        assert_eq!(
            inactive_reason(bundle, bundle, actual),
            InactiveReason::NotRunning
        );
        assert_eq!(
            inactive_reason(bundle, bundle, |name| {
                if name == "cosmic-comp" {
                    Some("0.1~2~26.04~abcdef0".into())
                } else {
                    actual(name)
                }
            }),
            InactiveReason::CosmicChanged
        );
    }

    #[test]
    fn isolated_inactive_message_never_reads_host_companion_state() {
        let fixture = Fixture::new("islands-status-isolated");
        let before = fixture.tree();
        assert_eq!(
            inactive_message(&fixture.store()),
            "The isolated Islands panel component is not active"
        );
        assert_eq!(fixture.tree(), before);
    }

    #[test]
    fn inactive_status_accepts_each_supported_combination_but_not_mixed_releases() {
        let fixture = Fixture::new("islands-status-matrix");
        let bundle = &fixture.0;
        std::fs::write(bundle.join("cosmic-panel-islands"), "fixture").unwrap();
        std::fs::write(bundle.join("selection.json"), r#"{"enabled":true}"#).unwrap();
        std::fs::write(
            bundle.join("manifest.json"),
            r#"{
            "packages":{"cosmic-panel":"1.11","cosmic-comp":"new","cosmic-session":"1.11"},
            "supported_packages":[
                {"cosmic-panel":"1.11","cosmic-comp":"new","cosmic-session":"1.11"},
                {"cosmic-panel":"1.10","cosmic-comp":"old","cosmic-session":"1.10"}
            ]
        }"#,
        )
        .unwrap();
        for (panel, compositor, session, expected) in [
            ("1.10", "old", "1.10", InactiveReason::NotRunning),
            ("1.11", "new", "1.11", InactiveReason::NotRunning),
            ("1.10", "new", "1.10", InactiveReason::CosmicChanged),
            ("1.12", "newer", "1.12", InactiveReason::CosmicChanged),
        ] {
            assert_eq!(
                inactive_reason(bundle, &bundle.join("state"), |name| Some(
                    match name {
                        "cosmic-panel" => panel,
                        "cosmic-comp" => compositor,
                        _ => session,
                    }
                    .into()
                )),
                expected
            );
        }
    }

    #[test]
    fn inactive_status_does_not_invent_an_upgrade_without_version_evidence() {
        let fixture = Fixture::new("islands-status-unknown");
        let bundle = &fixture.0;
        std::fs::write(bundle.join("cosmic-panel-islands"), "fixture").unwrap();
        std::fs::write(bundle.join("selection.json"), r#"{"enabled":true}"#).unwrap();
        for manifest in ["{}", "invalid", r#"{"packages":{"cosmic-panel":"1.11"}}"#] {
            std::fs::write(bundle.join("manifest.json"), manifest).unwrap();
            assert_eq!(
                inactive_reason(bundle, &fixture.0.join("state"), |_| None),
                InactiveReason::NotRunning
            );
        }
    }

    #[test]
    fn sidecar_apply_conflict_and_persistent_undo_preserve_native_lists() {
        let fixture = Fixture::new("islands-transaction");
        fixture_installed(&fixture.0);
        fixture.system(
            "com.system76.CosmicPanel.Dock",
            "plugins_center",
            "Some([\"unknown\",\"a\",\"a\"])",
        );
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        let Setting::Known(mut settings) = draft.islands() else {
            panic!()
        };
        settings.enabled = true;
        settings.gap = 32;
        draft.set_islands(settings.clone());
        let before = fixture.tree();
        assert!(!before.iter().any(|p| format!("{p:?}").contains(NAMESPACE)));
        let applied = store
            .apply_owned(&draft.patch(), None, Owner::DockPanel, "Dock Islands")
            .unwrap();
        assert_eq!(
            PanelDraft::load(&store, PanelKind::Dock)
                .unwrap()
                .islands()
                .known(),
            Some(&settings)
        );
        assert!(
            fixture
                .read("com.system76.CosmicPanel.Dock", "plugins_center")
                .is_none()
        );
        let reopened = fixture.store();
        reopened.undo_latest(&applied.id).unwrap();
        assert!(fixture.read(NAMESPACE, "Dock").is_none());
        let mut other = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        other.set_islands(settings.clone());
        fixture.user(
            NAMESPACE,
            "Dock",
            &ron::to_string(&Settings {
                gap: 1,
                ..Settings::default()
            })
            .unwrap(),
        );
        assert!(
            store
                .apply_owned(&other.patch(), None, Owner::DockPanel, "Dock Islands")
                .is_err()
        );
        assert_eq!(
            fixture.read("com.system76.CosmicPanel.Dock", "plugins_center"),
            None
        );
    }
    #[test]
    fn unknown_extension_is_kept_by_native_edits_and_saved_draft() {
        let f = Fixture::new("islands-unknown");
        f.user(NAMESPACE, "Dock", "(version:99,new_field:true)");
        let mut draft = PanelDraft::load(&f.store(), PanelKind::Dock).unwrap();
        assert!(matches!(draft.islands(), Setting::Unsupported(_)));
        draft.set_anchor(crate::desktop::panels::Anchor::Left);
        let restored: PanelDraft = ron::from_str(&ron::to_string(&draft).unwrap()).unwrap();
        f.store()
            .apply_owned(&restored.patch(), None, Owner::DockPanel, "Dock & Panel")
            .unwrap();
        assert_eq!(
            f.read(NAMESPACE, "Dock").as_deref(),
            Some("(version:99,new_field:true)")
        );
    }
}

#[cfg(test)]
mod installation_tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    #[test]
    fn task2_absent_islands_cannot_write_even_a_disabled_sidecar() {
        let f = Fixture::new("absent-islands-write");
        let store = f.store();
        let mut patch = super::super::DesktopPatch::new();
        patch.set(
            key(PanelKind::Dock),
            None,
            Some(RawValue::encode(&Settings::default()).unwrap()),
        );
        assert!(
            store.apply(&patch).is_err(),
            "leftovers/default settings must not install a sidecar"
        );
        assert!(!store.user_path(&key(PanelKind::Dock)).exists());
    }
}

mod installation;
#[cfg(test)]
pub(crate) use installation::fixture_installed;
pub use installation::{CompatibilityStatus, Installation, Provenance};

#[cfg(test)]
mod installation_races {
    use super::*;
    #[test]
    fn task2_removal_after_preview_blocks_sidecar_but_preserves_stored_preferences() {
        let f = crate::desktop::store::tests::Fixture::new("islands-removed");
        fixture_installed(&f.0);
        let store = f.store();
        let mut patch = crate::desktop::DesktopPatch::new();
        let key = key(PanelKind::Panel);
        patch.set(
            key.clone(),
            None,
            Some(RawValue::encode(&Settings::default()).unwrap()),
        );
        store.apply(&patch).unwrap();
        let saved = std::fs::read(store.user_path(&key)).unwrap();
        let mut changed = Settings::default();
        changed.gap = 42;
        patch.set(
            key.clone(),
            store.read_user(&key).unwrap(),
            Some(RawValue::encode(&changed).unwrap()),
        );
        std::fs::remove_file(f.0.join("islands-installation.json")).unwrap();
        assert!(store.apply(&patch).is_err());
        assert_eq!(std::fs::read(store.user_path(&key)).unwrap(), saved);
    }
}
