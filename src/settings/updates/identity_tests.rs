use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn islands_needs_verified_package_and_preserves_source_choices() {
    let mut identity = Identity {
        channel: UpdateChannel::ManualDeb,
        enrollment: Enrollment::Disabled,
        architecture: Some("amd64".into()),
        installed_version: Some(env!("CARGO_PKG_VERSION").into()),
        evidence: Some("verified".into()),
        ..Identity::default()
    };
    assert!(identity.can_manage_islands());
    assert!(identity.enrollment != Enrollment::Available);
    for channel in [UpdateChannel::Source, UpdateChannel::Unknown] {
        identity.channel = channel;
        assert!(!identity.can_manage_islands());
    }
    identity.channel = UpdateChannel::Apt;
    let original = identity.clone();
    identity.evidence = Some("changed".into());
    assert!(!original.same_package(&identity));
    identity.installed_version = Some("0.0.1".into());
    assert!(!identity.can_manage_islands());
}

#[test]
fn bundled_islands_requires_main_ownership_and_matching_manifest() {
    let fixture = Fixture::new();
    let base = fs::read_to_string(fixture.path("/var/lib/dpkg/status")).unwrap();
    assert_eq!(fixture.measure().islands_match, IslandsMatch::Absent);
    let bundled = base.replacen(
        "Package: cosmic-ext-halcyon\n",
        "Package: cosmic-ext-halcyon\nX-Halcyon-Islands: bundled\n",
        1,
    );
    fixture.put("/var/lib/dpkg/status", bundled.as_bytes());
    let helper = "/usr/bin/halcyon-islands";
    let manifest = "/usr/lib/cosmic-ext-halcyon-islands/companion-manifest.json";
    fixture.put(
        helper,
        include_bytes!("../../../packaging/islands/halcyon-islands"),
    );
    fs::set_permissions(fixture.path(helper), fs::Permissions::from_mode(0o755)).unwrap();
    let files = [EXE, helper, manifest];
    for (owner, version, expected) in [
        (MAIN, "0.2.0", IslandsMatch::Ready),
        (MAIN, "99.0.0", IslandsMatch::VersionMismatch),
        (
            "cosmic-ext-halcyon-islands",
            "0.2.0",
            IslandsMatch::VersionMismatch,
        ),
    ] {
        fixture.put(
            manifest,
            serde_json::json!({"package_owner": owner, "package_version": version})
                .to_string()
                .as_bytes(),
        );
        fixture.put(
            "/var/lib/dpkg/info/cosmic-ext-halcyon.list",
            files.join("\n").as_bytes(),
        );
        let hashes = files
            .iter()
            .map(|p| {
                format!(
                    "{}  {}\n",
                    md5(&fixture.path(p)).unwrap(),
                    p.trim_start_matches('/')
                )
            })
            .collect::<String>();
        fixture.put(
            "/var/lib/dpkg/info/cosmic-ext-halcyon.md5sums",
            hashes.as_bytes(),
        );
        assert_eq!(fixture.measure().islands_match, expected);
    }
    fixture.put("/var/lib/dpkg/info/cosmic-ext-halcyon.list", EXE.as_bytes());
    assert_eq!(
        fixture.measure().islands_match,
        IslandsMatch::HelperMismatch
    );
    fixture.put(
        "/var/lib/dpkg/status",
        bundled
            .replace("install ok installed", "install ok unpacked")
            .as_bytes(),
    );
    assert!(!fixture.measure().can_enable_islands());
}

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "halcyon-identity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let f = Self { root };
        f.put(EXE, b"fixture app");
        f.put(SOURCE, SOURCE_TEXT.as_bytes());
        f.put(PREFS, PREFS_TEXT.as_bytes());
        f.put(KEY, &[0x99; 256]);
        f.put("/var/lib/dpkg/status",b"Package: cosmic-ext-halcyon\nStatus: install ok installed\nArchitecture: amd64\nVersion: 0.2.0\n\nPackage: halcyon-update-source\nStatus: install ok installed\nArchitecture: all\nVersion: 0.2.0\n\nPackage: halcyon-archive-keyring\nStatus: install ok installed\nArchitecture: all\nVersion: 0.2.0\n\n");
        f.records();
        symlink(f.path(EXE), f.path("/running")).unwrap();
        f
    }
    fn path(&self, path: &str) -> PathBuf {
        rooted(&self.root, path)
    }
    fn put(&self, path: &str, bytes: &[u8]) {
        let p = self.path(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn records(&self) {
        for (name, files) in [
            (MAIN, vec![EXE]),
            ("halcyon-update-source", vec![SOURCE, PREFS]),
            ("halcyon-archive-keyring", vec![KEY]),
        ] {
            self.put(
                &format!("/var/lib/dpkg/info/{name}.list"),
                files.join("\n").as_bytes(),
            );
            let hashes = files
                .iter()
                .map(|path| {
                    format!(
                        "{}  {}\n",
                        md5(&self.path(path)).unwrap(),
                        path.trim_start_matches('/')
                    )
                })
                .collect::<String>();
            self.put(
                &format!("/var/lib/dpkg/info/{name}.md5sums"),
                hashes.as_bytes(),
            );
        }
    }
    fn measure(&self) -> Identity {
        measure(
            &self.root,
            &self.path("/running"),
            &self.path("/build/halcyon"),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn actual_files_package_checksum_source_and_other_path_are_independent() {
    let f = Fixture::new();
    assert_eq!(f.measure().channel, UpdateChannel::Apt);
    fs::remove_file(f.path(SOURCE)).unwrap();
    assert_eq!(f.measure().channel, UpdateChannel::ManualDeb);
    f.put(SOURCE, format!("{SOURCE_TEXT}Enabled: no\n").as_bytes());
    assert_eq!(f.measure().channel, UpdateChannel::ManualDeb);
    f.put(SOURCE, SOURCE_TEXT.as_bytes());
    f.put("/build/halcyon", b"fixture app");
    fs::remove_file(f.path("/running")).unwrap();
    symlink(f.path("/build/halcyon"), f.path("/running")).unwrap();
    assert_eq!(f.measure().channel, UpdateChannel::Source);
    assert_eq!(f.measure().installed_version, None);
    assert_eq!(
        measure(
            &f.root,
            &f.path("/running"),
            &f.path("/different-build/halcyon")
        )
        .channel,
        UpdateChannel::Unknown
    );
    // The genuine package and scoped source still exist beside this process.
    assert!(f.path(EXE).is_file());
    assert!(f.path(SOURCE).is_file());
}
#[test]
fn damaged_owner_architecture_package_or_enrollment_cannot_offer_check() {
    for change in [
        "binary",
        "owner",
        "architecture",
        "status",
        "checksum",
        "key-checksum",
        "key",
        "source",
        "preferences",
        "duplicate-source",
        "diversion",
        "symlink",
        "unknown-source",
    ] {
        let f = Fixture::new();
        match change {
            "binary" => f.put(EXE, b"modified executable"),
            "owner" => f.put(
                &format!("/var/lib/dpkg/info/{MAIN}.list"),
                b"/usr/bin/something-else",
            ),
            "architecture" | "status" => {
                let p = f.path("/var/lib/dpkg/status");
                let s = fs::read_to_string(&p).unwrap();
                fs::write(
                    p,
                    if change == "architecture" {
                        s.replace("amd64", "arm64")
                    } else {
                        s.replace("install ok installed", "install ok unpacked")
                    },
                )
                .unwrap();
            }
            "checksum" => {
                fs::remove_file(f.path(&format!("/var/lib/dpkg/info/{MAIN}.md5sums"))).unwrap();
            }
            "key-checksum" => {
                fs::remove_file(f.path("/var/lib/dpkg/info/halcyon-archive-keyring.md5sums"))
                    .unwrap();
            }
            "key" => f.put(KEY, b"wrong key"),
            "source" => f.put(SOURCE, SOURCE_TEXT.replace("https:", "http:").as_bytes()),
            "unknown-source" => f.put(SOURCE, b"URIs: https://unknown.invalid\n"),
            "preferences" => f.put(PREFS, PREFS_TEXT.replace("-1", "1001").as_bytes()),
            "duplicate-source" => f.put(
                "/etc/apt/sources.list.d/other.sources",
                SOURCE_TEXT.as_bytes(),
            ),
            "diversion" => f.put(
                "/var/lib/dpkg/diversions",
                format!("{EXE}\n/usr/bin/other\nlocal\n").as_bytes(),
            ),
            "symlink" => {
                f.put("/another-app", b"fixture app");
                fs::remove_file(f.path(EXE)).unwrap();
                symlink(f.path("/another-app"), f.path(EXE)).unwrap();
            }
            _ => unreachable!(),
        }
        let result = f.measure();
        assert!(!result.can_check(), "{change}: {result:?}");
        assert_eq!(result.channel, UpdateChannel::Unknown, "{change}");
    }
}
#[test]
fn procfs_running_inode_survives_real_replacement_and_deletion() {
    struct Process(std::process::Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let f = Fixture::new();
    fs::copy("/usr/bin/sleep", f.path(EXE)).unwrap();
    fs::set_permissions(f.path(EXE), fs::Permissions::from_mode(0o755)).unwrap();
    f.records();
    let process = Process(Command::new(f.path(EXE)).arg("30").spawn().unwrap());
    let proc_exe = PathBuf::from(format!("/proc/{}/exe", process.0.id()));
    assert_eq!(
        measure(&f.root, &proc_exe, &f.path("/build/halcyon")).channel,
        UpdateChannel::Apt
    );
    fs::copy("/usr/bin/true", f.path("/replacement")).unwrap();
    fs::rename(f.path("/replacement"), f.path(EXE)).unwrap();
    f.records();
    let changed = measure(&f.root, &proc_exe, &f.path("/build/halcyon"));
    assert_eq!(changed.channel, UpdateChannel::Unknown);
    assert_eq!(changed.installed_version.as_deref(), Some("0.2.0"));
    assert!(changed.reason.contains("replaced or removed"));
    fs::remove_file(f.path(EXE)).unwrap();
    let deleted = measure(&f.root, &proc_exe, &f.path("/build/halcyon"));
    assert_eq!(deleted.channel, UpdateChannel::Unknown);
    assert!(deleted.reason.contains("replaced or removed"));
}
#[test]
fn checks_bind_enrollment_bytes_and_ignore_unrelated_multiarch_packages() {
    let f = Fixture::new();
    let first = f.measure();
    let p = f.path("/var/lib/dpkg/status");
    let mut text = fs::read_to_string(&p).unwrap();
    for arch in ["amd64", "i386"] {
        text.push_str(&format!(
            "Package: libc6\nStatus: install ok installed\nArchitecture: {arch}\nVersion: 1.0\n\n"
        ));
    }
    fs::write(p, text).unwrap();
    assert!(first.same_installation(&f.measure()));
    f.put(KEY, &[0x98; 256]);
    f.records();
    assert!(!first.same_installation(&f.measure()));
}
#[test]
fn store_desktop_id_respects_xdg_shadowing_hidden_and_missing_entries() {
    let f = Fixture::new();
    let system = f.path("/system/applications");
    let user = f.path("/user/applications");
    fs::create_dir_all(&system).unwrap();
    fs::create_dir_all(&user).unwrap();
    let dirs = vec![user.clone(), system.clone()];
    let launcher = f.path("/tools/gio");
    let program = f.path("/tools/cosmic-store");
    f.put("/tools/gio", b"fixture");
    f.put("/tools/cosmic-store", b"fixture");
    for path in [&launcher, &program] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(store_entry(&dirs, &launcher, &program).is_none());
    fs::write(
        system.join(STORE),
        "[Desktop Entry]\nType=Application\nExec=cosmic-store %u\nTerminal=false\n",
    )
    .unwrap();
    assert_eq!(
        store_entry(&dirs, &launcher, &program),
        Some(system.join(STORE))
    );
    fs::remove_file(&program).unwrap();
    assert!(store_entry(&dirs, &launcher, &program).is_none());
    f.put("/tools/cosmic-store", b"fixture");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    for text in [
        "[Desktop Entry]\nType=Application\nExec=cosmic-store %u\nHidden=true\n",
        "[Desktop Entry]\nType=Application\nExec=cosmic-store updates\n",
        "[Desktop Entry]\nType=Application\nExec=other-store\n",
        "[Desktop Entry]\nType=Application\nExec=cosmic-store %u\nDBusActivatable=true\n",
        "[Desktop Entry]\nType=Application\nExec=cosmic-store %u\n[Desktop Entry]\nType=Application\nExec=other-store\n",
        "broken",
    ] {
        fs::write(user.join(STORE), text).unwrap();
        assert!(store_entry(&dirs, &launcher, &program).is_none());
    }
}

#[test]
fn ignored_removal_backups_do_not_disable_reenrollment_but_active_conflicts_do() {
    let f = Fixture::new();
    f.put(
        "/etc/apt/sources.list.d/halcyon.sources.removed.fixture",
        SOURCE_TEXT.as_bytes(),
    );
    assert_eq!(f.measure().channel, UpdateChannel::Apt);
    fs::remove_file(f.path(SOURCE)).unwrap();
    assert_eq!(f.measure().channel, UpdateChannel::ManualDeb);
    f.put(
        "/etc/apt/sources.list.d/another.sources",
        SOURCE_TEXT.as_bytes(),
    );
    assert_eq!(f.measure().channel, UpdateChannel::Unknown);
}

#[test]
fn enrollment_preserves_disabled_sources_and_partial_installations() {
    let f = Fixture::new();
    assert_eq!(f.measure().enrollment, Enrollment::Enrolled);
    f.put(SOURCE, format!("{SOURCE_TEXT}Enabled: no\n").as_bytes());
    let disabled = f.measure();
    assert_eq!(disabled.enrollment, Enrollment::Disabled);
    assert!(disabled.enrollment != Enrollment::Available);
    fs::remove_file(f.path(SOURCE)).unwrap();
    assert_eq!(
        f.measure().enrollment,
        Enrollment::Disabled,
        "deleted conffile stays disabled"
    );
    f.put("/var/lib/dpkg/status", b"Package: cosmic-ext-halcyon\nStatus: install ok installed\nArchitecture: amd64\nVersion: 0.2.0\n\n");
    assert_eq!(f.measure().enrollment, Enrollment::Incomplete);
    fs::remove_file(f.path(KEY)).unwrap();
    fs::remove_file(f.path(PREFS)).unwrap();
    let clean = f.measure();
    assert_eq!(clean.enrollment, Enrollment::Available);
    f.put(SOURCE, format!("{SOURCE_TEXT}Enabled: no\n").as_bytes());
    assert_eq!(f.measure().enrollment, Enrollment::Disabled);
}

fn default_package() -> Fixture {
    let f = Fixture::new();
    let status = fs::read_to_string(f.path("/var/lib/dpkg/status")).unwrap();
    f.put(
        "/var/lib/dpkg/status",
        format!("{}\n\n", status.split("\n\n").next().unwrap()).as_bytes(),
    );
    let mut files = vec![EXE.to_owned()];
    let mut hashes = serde_json::Map::new();
    for (name, target) in [
        ("source.sources", SOURCE),
        ("preferences", PREFS),
        ("archive-keyring.gpg", KEY),
    ] {
        let template = format!("{DEFAULTS}/{name}");
        f.put(&template, &fs::read(f.path(target)).unwrap());
        hashes.insert(target.into(), digest(&f.path(target)).unwrap().into());
        files.push(template);
    }
    f.put(
        &format!("/var/lib/dpkg/info/{MAIN}.list"),
        files.join("\n").as_bytes(),
    );
    f.put(
        &format!("/var/lib/dpkg/info/{MAIN}.md5sums"),
        files
            .iter()
            .map(|p| {
                format!(
                    "{}  {}\n",
                    md5(&f.path(p)).unwrap(),
                    p.trim_start_matches('/')
                )
            })
            .collect::<String>()
            .as_bytes(),
    );
    f.put(RECEIPT, &serde_json::to_vec(&serde_json::json!({"format":"halcyon-default-updates","version":1,"phase":"complete","files":hashes})).unwrap());
    f
}

#[test]
fn package_default_updates_require_verified_inputs_and_completed_receipt() {
    let f = default_package();
    assert!(f.measure().can_check());
    for change in [
        "source",
        "pins",
        "key",
        "template",
        "receipt",
        "pending",
        "symlink",
        "duplicate",
    ] {
        let f = default_package();
        match change {
            "source" => f.put(SOURCE, SOURCE_TEXT.replace("noble", "other").as_bytes()),
            "pins" => f.put(PREFS, PREFS_TEXT.replace("-1", "1001").as_bytes()),
            "key" => f.put(KEY, &[1; 256]),
            "template" => f.put(&format!("{DEFAULTS}/archive-keyring.gpg"), &[1; 256]),
            "receipt" => f.put(RECEIPT, b"{}"),
            "pending" => {
                let value = fs::read_to_string(f.path(RECEIPT))
                    .unwrap()
                    .replace("complete", "pending");
                f.put(RECEIPT, value.as_bytes());
            }
            "symlink" => {
                fs::remove_file(f.path(KEY)).unwrap();
                symlink(
                    f.path(&format!("{DEFAULTS}/archive-keyring.gpg")),
                    f.path(KEY),
                )
                .unwrap();
            }
            "duplicate" => f.put(
                "/etc/apt/sources.list.d/other.sources",
                SOURCE_TEXT.as_bytes(),
            ),
            _ => unreachable!(),
        }
        let identity = f.measure();
        assert!(
            !identity.can_check() && identity.enrollment != Enrollment::Available,
            "{change}: {identity:?}"
        );
    }
}

#[test]
fn deleted_or_disabled_default_sources_never_offer_automatic_reenrollment() {
    let f = default_package();
    f.put(SOURCE, format!("{SOURCE_TEXT}Enabled: no\n").as_bytes());
    assert_eq!(f.measure().enrollment, Enrollment::Disabled);
    fs::remove_file(f.path(SOURCE)).unwrap();
    let identity = f.measure();
    assert_eq!(identity.enrollment, Enrollment::Disabled);
    assert!(identity.enrollment != Enrollment::Available && !identity.can_check());
}

#[test]
fn current_release_never_offers_legacy_bootstrap_after_partial_enrollment() {
    let f = default_package();
    fs::remove_file(f.path(RECEIPT)).unwrap();
    for path in [SOURCE, KEY, PREFS] {
        fs::remove_file(f.path(path)).unwrap();
        let measured = f.measure();
        assert_eq!(
            measured.enrollment,
            Enrollment::Incomplete,
            "{path}: {measured:?}"
        );
        assert!(!measured.can_check());
    }
    assert!(f.path(DEFAULTS).exists());
}
