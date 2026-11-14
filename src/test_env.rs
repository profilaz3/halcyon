//! Environment boundary for re-executed tests. Never mutates this process's env.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

const ROOT: &str = "HALCYON_TEST_ROOT";
// Inject a second writer after a durable preference commit, before its caller
// resumes. Thread-local and one-shot; never compiled into the application.
thread_local! {
    pub static AFTER_PREFERENCE_COMMIT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
const DIRECTORIES: [(&str, &str); 8] = [
    ("HOME", "home"),
    ("XDG_CONFIG_HOME", "home/.config"),
    ("XDG_CONFIG_DIRS", "system-config"),
    ("XDG_DATA_HOME", "home/.local/share"),
    ("XDG_DATA_DIRS", "data"),
    ("XDG_STATE_HOME", "home/.local/state"),
    ("XDG_CACHE_HOME", "home/.cache"),
    ("XDG_RUNTIME_DIR", "runtime"),
];

pub fn configure(command: &mut Command, root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let root = root.canonicalize().expect("fixture root exists");
    command.env(ROOT, &root);
    for (key, relative) in DIRECTORIES {
        let path = root.join(relative);
        std::fs::create_dir_all(&path).unwrap();
        command.env(key, path);
    }
    std::fs::set_permissions(root.join("runtime"), std::fs::Permissions::from_mode(0o700)).unwrap();
    // Explicitly unavailable fixture sockets prevent both inherited session-bus
    // access and D-Bus autolaunch. Tests with a private bus may override these.
    for (key, name) in [
        ("DBUS_SESSION_BUS_ADDRESS", "session-bus"),
        ("DBUS_SYSTEM_BUS_ADDRESS", "system-bus"),
    ] {
        command.env(
            key,
            format!("unix:path={}", root.join("runtime").join(name).display()),
        );
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "WAYLAND_SOCKET",
        "XAUTHORITY",
        "DBUS_STARTER_ADDRESS",
        "DBUS_STARTER_BUS_TYPE",
        "DESKTOP_CARDS_CONFIG",
        "DESKTOP_CARDS_DESKTOP_ROOT",
        "DESKTOP_CARDS_CAPTURE_DIR",
        "DESKTOP_CARDS_SETTINGS_CAPTURE_DIR",
        "DESKTOP_CARDS_SETTINGS_SIZE",
    ] {
        command.env_remove(key);
    }
}

pub fn assert_isolated() -> PathBuf {
    let root = PathBuf::from(std::env::var_os(ROOT).expect("isolated child root"));
    assert!(root.is_absolute());
    for (key, _) in DIRECTORIES {
        let path = PathBuf::from(std::env::var_os(key).expect(key));
        assert!(
            path.canonicalize().unwrap().starts_with(&root),
            "{key} escapes fixture"
        );
    }
    for key in ["DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS"] {
        let address = std::env::var(key).unwrap();
        let path = Path::new(address.strip_prefix("unix:path=").expect("fixture bus"));
        assert!(path.starts_with(&root), "{key} escapes fixture");
    }
    for key in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"] {
        assert!(std::env::var_os(key).is_none(), "inherited {key}");
    }
    for path in [
        crate::config::Config::path(),
        crate::paths::state_dir(),
        crate::paths::cache_dir(),
        crate::paths::runtime_dir(),
    ] {
        assert!(
            path.unwrap().starts_with(&root),
            "application path escapes fixture"
        );
    }
    root
}

/// A path inside a fresh `/tmp/halcyon-<label>-*` folder. Dropping it, also
/// while a failed test unwinds, unmounts anything left mounted under the folder
/// and removes the folder.
pub struct TempPath {
    root: PathBuf,
    path: PathBuf,
}

impl TempPath {
    pub fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).unwrap();
        let root = std::env::temp_dir().join(format!(
            "halcyon-{label}-{}-{}-{:x}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            u64::from_le_bytes(bytes)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self {
            path: root.clone(),
            root,
        }
    }

    /// The same folder, pointing at `relative` inside it.
    pub fn at(mut self, relative: &str) -> Self {
        self.path = self.root.join(relative);
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl std::ops::Deref for TempPath {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempPath {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        unmount_under(&self.root);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Mount points at or below `root` in `/proc/mounts` text, deepest first.
fn mounts_under(mounts: &str, root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = mounts
        .lines()
        .filter_map(|line| line.split(' ').nth(1))
        .map(|field| {
            // /proc/mounts writes space, tab, newline and backslash as octal.
            PathBuf::from(
                field
                    .replace("\\040", " ")
                    .replace("\\011", "\t")
                    .replace("\\012", "\n")
                    .replace("\\134", "\\"),
            )
        })
        .filter(|point| point.starts_with(root))
        .collect();
    found.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    found
}

/// Detach FUSE mounts a test left under `root`, such as a document portal or
/// gvfs mount whose daemon has gone.
pub fn unmount_under(root: &Path) {
    let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else {
        return;
    };
    for point in mounts_under(&mounts, root) {
        for tool in ["fusermount3", "fusermount"] {
            let done = Command::new(tool)
                .args(["-u", "-z"])
                .arg(&point)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if done {
                break;
            }
        }
    }
}

/// A private session bus that can't start anything by itself. The stock
/// session config lists every installed service, so a test that touched the
/// document portal or gvfs started a private xdg-document-portal or
/// gvfsd-fuse that outlived the bus and left FUSE mounts behind.
pub struct PrivateBus {
    daemon: std::process::Child,
    pub address: String,
    _folder: TempPath,
}

impl PrivateBus {
    pub fn start() -> std::io::Result<Self> {
        use std::io::BufRead;
        let folder = TempPath::new("bus");
        let config = folder.join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<busconfig><type>session</type><listen>unix:path={}</listen><auth>EXTERNAL</auth><policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>"#,
                folder.join("socket").display()
            ),
        )?;
        let mut daemon = Command::new("dbus-daemon")
            .arg("--config-file")
            .arg(&config)
            .args(["--nofork", "--print-address"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let mut address = String::new();
        std::io::BufReader::new(daemon.stdout.take().unwrap()).read_line(&mut address)?;
        if address.trim().is_empty() {
            let _ = daemon.kill();
            let _ = daemon.wait();
            return Err(std::io::Error::other("dbus-daemon printed no address"));
        }
        Ok(Self {
            daemon,
            address: address.trim().into(),
            _folder: folder,
        })
    }

    pub fn new() -> Self {
        Self::start().expect("private DBus fixture needs dbus-daemon")
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[test]
fn a_temp_path_goes_away_even_when_the_test_fails() {
    let mut kept = PathBuf::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let file = TempPath::new("guard").at("inside/usage.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"{}").unwrap();
        kept = file.root().to_path_buf();
        assert!(
            kept.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("halcyon-guard-")
        );
        panic!("a failing test");
    }));
    assert!(result.is_err());
    assert!(!kept.as_os_str().is_empty());
    assert!(!kept.exists(), "{} was left behind", kept.display());
}

#[test]
fn leftover_mounts_are_found_deepest_first() {
    let mounts = "\
proc /proc proc rw 0 0
xdg-document-portal /tmp/halcyon-qa-1/runtime/doc fuse.portal rw 0 0
gvfsd-fuse /tmp/halcyon-qa-1/runtime/gvfs fuse.gvfsd-fuse rw 0 0
gvfsd-fuse /tmp/halcyon-qa-10/runtime/gvfs fuse.gvfsd-fuse rw 0 0
tmpfs /tmp/halcyon-qa-1 tmpfs rw 0 0
x /tmp/halcyon-qa-1/a\\040b/doc fuse rw 0 0
";
    let found = mounts_under(mounts, Path::new("/tmp/halcyon-qa-1"));
    assert_eq!(
        found,
        [
            "/tmp/halcyon-qa-1/runtime/doc",
            "/tmp/halcyon-qa-1/runtime/gvfs",
            "/tmp/halcyon-qa-1/a b/doc",
            "/tmp/halcyon-qa-1",
        ]
        .map(PathBuf::from)
    );
}

#[test]
fn a_private_bus_starts_nothing_on_its_own() {
    let bus = PrivateBus::start().expect("dbus-daemon");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let names = runtime.block_on(async {
        let connection = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        zbus::fdo::DBusProxy::new(&connection)
            .await
            .unwrap()
            .list_activatable_names()
            .await
            .unwrap()
    });
    let names: Vec<String> = names.into_iter().map(|name| name.to_string()).collect();
    assert_eq!(
        names,
        ["org.freedesktop.DBus"],
        "the bus can start {names:?}"
    );
}

/// Select production worker dispatch independently of any provider stub.
pub fn native_worker(root: &Path, enabled: bool) {
    assert_eq!(
        std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT")
            .map(PathBuf::from)
            .as_deref(),
        Some(root),
        "worker mode belongs to the active isolated desktop"
    );
    let path = root.join("native-worker-enabled");
    if enabled {
        std::fs::write(path, b"1").unwrap();
    } else if let Err(error) = std::fs::remove_file(path) {
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
}

#[test]
fn worker_mode_is_independent_of_probe_files() {
    if crate::displays::test_support::isolated(
        "test_env::worker_mode_is_independent_of_probe_files",
    ) {
        return;
    }
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let root = store.fixture_root().unwrap();
    use crate::desktop::islands::Installation;
    assert!(!Installation::needs_worker(&store));
    std::fs::write(root.join("status-islands-probe.py"), "print('{}')\n").unwrap();
    assert!(
        !Installation::needs_worker(&store),
        "a probe does not select dispatch"
    );
    std::fs::remove_file(root.join("status-islands-probe.py")).unwrap();
    native_worker(root, true);
    assert!(
        Installation::needs_worker(&store),
        "worker needs no provider script"
    );
    native_worker(root, false);
    assert!(!Installation::needs_worker(&store));
}

#[test]
fn isolated_child_covers_paths_buses_and_cleanup() {
    if crate::displays::test_support::isolated(
        "test_env::isolated_child_covers_paths_buses_and_cleanup",
    ) {
        return;
    }
    let root = assert_isolated();
    for (key, _) in DIRECTORIES {
        let directory = PathBuf::from(std::env::var_os(key).unwrap());
        std::fs::write(directory.join("containment-marker"), key).unwrap();
        assert!(
            directory
                .join("containment-marker")
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
}
