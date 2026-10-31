//! RPM database facts, independent of Debian's package/version conventions.
use super::*;

const SOURCE: &str = "/etc/yum.repos.d/halcyon.repo";
const KEY: &str = "/etc/pki/rpm-gpg/RPM-GPG-KEY-halcyon";
const BASE: &str = "/usr/share/cosmic-ext-halcyon/updates-rpm";
const RECEIPT: &str = "/var/lib/halcyon/updates/rpm-source.json";
const METADATA: &str = "/usr/share/cosmic-ext-halcyon/installation.json";

pub(super) fn system(root: &Path) -> bool {
    read(&rooted(root, "/etc/os-release"), 65536).is_some_and(|text| {
        text.lines().any(|line| {
            line.strip_prefix("ID=")
                .is_some_and(|id| id.trim_matches('"') == "fedora")
        })
    })
}

/// Bounded fixed-program queries. Drain stdout while the child is alive so a
/// large file list cannot deadlock on the pipe. Kill and reap on every timeout.
fn query(root: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("/usr/bin/rpm");
    if root != Path::new("/") {
        command.arg("--root").arg(root);
    }
    let mut child = command
        .args(["--dbpath", "/usr/lib/sysimage/rpm"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(262145).read_to_end(&mut bytes).ok()?;
        (bytes.len() <= 262144).then_some(bytes)
    });
    let start = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(5))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader.join().ok()??;
    success.then_some(String::from_utf8(bytes).ok()?)
}

#[derive(Debug)]
struct Package {
    evr: String,
    version: String,
    files: BTreeMap<String, String>,
    raw: String,
}
impl Package {
    fn parse(raw: String) -> Option<Self> {
        let mut lines = raw.lines();
        let header: Vec<_> = lines.next()?.split('\t').collect();
        if header.len() != 5
            || header[0] != MAIN
            || header[3] != "x86_64"
            || header[4] != "8"
            || !valid_version(header[1])
            || !valid_version(header[2])
        {
            return None;
        }
        let (evr, version) = (header[1].to_owned(), header[2].to_owned());
        let mut files = BTreeMap::new();
        for line in lines {
            let (path, hash) = line.split_once('\t')?;
            // Directories/symlinks have no file digest. They never authorize a file.
            if hash.is_empty() {
                continue;
            }
            if !path.starts_with('/')
                || path.split('/').any(|s| s == "..")
                || hash.len() != 64
                || !hash.bytes().all(|c| c.is_ascii_hexdigit())
                || files.insert(path.into(), hash.into()).is_some()
            {
                return None;
            }
        }
        Some(Self {
            evr,
            version,
            files,
            raw,
        })
    }
    fn load(root: &Path) -> Option<Self> {
        Self::parse(query(
            root,
            &[
                "-q",
                "--qf",
                "%{NAME}\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\t%{VERSION}\t%{ARCH}\t%{FILEDIGESTALGO}\n[%{FILENAMES}\t%{FILEDIGESTS}\n]",
                MAIN,
            ],
        )?)
    }
    fn verified(&self, root: &Path, path: &str) -> Option<String> {
        let file = rooted(root, path);
        let metadata = file.symlink_metadata().ok()?;
        if !metadata.is_file()
            || metadata.mode() & 0o022 != 0
            || (root == Path::new("/") && metadata.uid() != 0)
        {
            return None;
        }
        let before = stamp(&file)?;
        let hash = digest(&file)?;
        (self.files.get(path) == Some(&hash) && stamp(&file)? == before).then_some(hash)
    }
    fn product(&self, root: &Path) -> Option<String> {
        self.verified(root, METADATA)?;
        let value: serde_json::Value =
            serde_json::from_str(&read(&rooted(root, METADATA), 8192)?).ok()?;
        let version = value["app_version"].as_str()?;
        (value["format"] == "halcyon-installation"
            && value["version"] == 1
            && value["package_format"] == "rpm"
            && value["architecture"] == "x86_64"
            && value["package_version"] == self.evr
            && version == self.version
            && valid_version(version))
        .then(|| version.to_owned())
    }
    fn cli(&self, root: &Path) -> bool {
        self.verified(root, "/usr/bin/halcyon-islands").is_some()
            && read(&rooted(root, "/usr/bin/halcyon-islands"), 65536).is_some_and(|s| {
                s.as_bytes() == include_bytes!("../../packaging/islands/halcyon-islands")
            })
    }
}

fn enrollment(root: &Path, package: &Package) -> (Enrollment, Option<String>) {
    let source_reference = format!("{BASE}/source.repo");
    let key_reference = format!("{BASE}/archive-key.asc");
    let Some(source_hash) = package.verified(root, &source_reference) else {
        return (Enrollment::Incomplete, None);
    };
    let Some(key_hash) = package.verified(root, &key_reference) else {
        return (Enrollment::Incomplete, None);
    };
    // Refuse conflicting/renamed definitions rather than guessing which wins.
    if let Ok(entries) = fs::read_dir(rooted(root, "/etc/yum.repos.d")) {
        let mut count = 0;
        for entry in entries {
            count += 1;
            if count > 256 {
                return (Enrollment::Unknown, None);
            }
            let Ok(entry) = entry else {
                return (Enrollment::Unknown, None);
            };
            let path = entry.path();
            if path == rooted(root, SOURCE) || path.extension().is_none_or(|s| s != "repo") {
                continue;
            }
            let Some(text) = read(&path, 1024 * 1024) else {
                return (Enrollment::Unknown, None);
            };
            if text.to_ascii_lowercase().contains("halcyon") {
                return (Enrollment::Unknown, None);
            }
        }
    }
    let source = read(&rooted(root, SOURCE), 65536);
    if source.is_none() {
        return (missing_source(root), None);
    }
    let source = source.unwrap();
    if source.lines().any(|l| {
        l.trim()
            .split_once('=')
            .is_some_and(|(k, v)| k.trim() == "enabled" && matches!(v.trim(), "0" | "false" | "no"))
    }) {
        return (Enrollment::Disabled, None);
    }
    if protected_digest(root, SOURCE).as_ref() == Some(&source_hash)
        && protected_digest(root, KEY).as_ref() == Some(&key_hash)
    {
        return (
            Enrollment::Enrolled,
            Some(format!("{source_hash}{key_hash}")),
        );
    }
    (Enrollment::Unknown, None)
}

fn missing_source(root: &Path) -> Enrollment {
    let path = rooted(root, RECEIPT);
    if !path.try_exists().unwrap_or(true) {
        return Enrollment::Available;
    }
    let Some(value) =
        read(&path, 65536).and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return Enrollment::Unknown;
    };
    if value["format"] != "halcyon-rpm-updates" || value["version"] != 1 {
        return Enrollment::Unknown;
    }
    match value["phase"].as_str() {
        Some("complete" | "disabled" | "preserved") => Enrollment::Disabled,
        Some("pending" | "key-update" | "conflict") => Enrollment::Incomplete,
        _ => Enrollment::Unknown,
    }
}

pub(super) fn cli_verified(root: &Path) -> bool {
    Package::load(root).is_some_and(|p| p.cli(root))
}

pub(super) fn observation(root: &Path) -> Observation {
    let Some(package) = Package::load(root) else {
        return Observation::default();
    };
    let Some(version) = package.product(root) else {
        return Observation::default();
    };
    let (state, _) = enrollment(root, &package);
    if Package::load(root).is_none_or(|p| p.raw != package.raw) {
        return Observation::default();
    }
    Observation {
        package_format: "rpm",
        update_channel: observed_channel(state, "dnf"),
        installed_app_version: Some(version),
    }
}

pub(super) fn measure(root: &Path, proc_exe: &Path, running_hash: &str, deleted: bool) -> Identity {
    let mut result = Identity::default();
    let Some(package) = Package::load(root) else {
        return result;
    };
    let Some(version) = package.product(root) else {
        return result;
    };
    let Some(installed_hash) = package.verified(root, EXE) else {
        return result;
    };
    result.installed_version = Some(version.clone());
    result.package_version = Some(package.evr.clone());
    result.architecture = Some("x86_64".into());
    let manifest_path = "/usr/lib/cosmic-ext-halcyon-islands/companion-manifest.json";
    result.islands_match = IslandsMatch::HelperMismatch;
    if package.verified(root, manifest_path).is_some() && package.cli(root) {
        if let Some(manifest) = read(&rooted(root, manifest_path), 65536)
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            result.islands_match = if manifest["package_owner"] == MAIN
                && manifest["package_version"] == package.evr
            {
                result.islands_version = Some(version);
                IslandsMatch::Ready
            } else {
                IslandsMatch::VersionMismatch
            };
        }
    }
    if deleted || installed_hash != running_hash || stamp(proc_exe) != stamp(&rooted(root, EXE)) {
        result.reason = "The running app was replaced or removed. Restart Halcyon to recheck.";
        return result;
    }
    let (state, source) = enrollment(root, &package);
    result.enrollment = state;
    result.channel = if source.is_some() {
        UpdateChannel::Dnf
    } else {
        UpdateChannel::ManualRpm
    };
    result.reason = match state {
        Enrollment::Enrolled => "System updates enabled · managed by your system.",
        Enrollment::Disabled => "System updates were disabled. Your source settings are preserved.",
        Enrollment::Available => "Manual package · system updates are not enabled.",
        Enrollment::Incomplete => {
            "System update setup is incomplete. Review the installation instructions."
        }
        Enrollment::Unknown => {
            "System update setup could not be verified. Review the installation instructions."
        }
    };
    result.evidence = Some(format!(
        "{running_hash}{}{}",
        package.evr,
        source.unwrap_or_default()
    ));
    if Package::load(root).is_none_or(|p| p.raw != package.raw)
        || digest(proc_exe).as_deref() != Some(running_hash)
        || digest(&rooted(root, EXE)).as_deref() != Some(running_hash)
    {
        result.channel = UpdateChannel::Unknown;
        result.evidence = None;
        result.islands_match = IslandsMatch::Unknown;
        result.reason = "Installation changed. Reopen Settings to check it again.";
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_rpm_source_distinguishes_interruption_from_disabled_choice() {
        let root = std::env::temp_dir().join(format!(
            "halcyon-rpm-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert_eq!(missing_source(&root), Enrollment::Available);
        let path = rooted(&root, RECEIPT);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for (phase, expected) in [
            ("pending", Enrollment::Incomplete),
            ("key-update", Enrollment::Incomplete),
            ("conflict", Enrollment::Incomplete),
            ("complete", Enrollment::Disabled),
            ("disabled", Enrollment::Disabled),
            ("preserved", Enrollment::Disabled),
            ("unexpected", Enrollment::Unknown),
        ] {
            fs::write(
                &path,
                format!(r#"{{"format":"halcyon-rpm-updates","version":1,"phase":"{phase}"}}"#),
            )
            .unwrap();
            assert_eq!(missing_source(&root), expected);
        }
        fs::write(&path, "damaged").unwrap();
        assert_eq!(missing_source(&root), Enrollment::Unknown);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rpm_identity_keeps_evr_and_product_version_distinct() {
        let p = Package::parse(format!(
            "{MAIN}\t2:0.5.0-4.fc43\t0.5.0\tx86_64\t8\n/usr/bin/halcyon\t{}\n",
            "a".repeat(64)
        ))
        .unwrap();
        assert_eq!(p.evr, "2:0.5.0-4.fc43");
        assert_eq!(p.version, "0.5.0");
    }
    #[test]
    fn rpm_identity_rejects_ambiguous_or_unverifiable_inventory() {
        let base = format!(
            "{MAIN}\t0:0.5.0-1.fc43\t0.5.0\tx86_64\t8\n/usr/bin/halcyon\t{}\n",
            "a".repeat(64)
        );
        for bad in [
            base.replace("x86_64", "aarch64"),
            base.replace("\t8\n", "\t1\n"),
            format!("{base}/usr/bin/halcyon\t{}\n", "b".repeat(64)),
            base.replace("/usr/bin/halcyon", "/usr/../bin/halcyon"),
        ] {
            assert!(Package::parse(bad).is_none());
        }
    }

    #[test]
    #[ignore = "Requires the installed local candidate in the isolated Fedora acceptance container"]
    fn fedora_installed_package_identity_and_disabled_source() {
        let root = Path::new("/");
        assert!(system(root));
        let exe = Path::new(EXE);
        let hash = digest(exe).unwrap();
        let identity = measure(root, exe, &hash, false);
        assert_eq!(identity.channel, UpdateChannel::Dnf);
        assert_eq!(
            identity.installed_version.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert!(
            identity
                .package_version
                .as_deref()
                .unwrap()
                .ends_with(".fc43")
        );
        assert_eq!(identity.islands_match, IslandsMatch::Ready);
        let observed = observation(root);
        assert_eq!(observed.package_format, "rpm");
        assert_eq!(observed.update_channel, "dnf");
        assert_eq!(observed.installed_app_version, identity.installed_version);
        assert!(identity.can_check());
        assert!(identity.can_manage_islands());
        let path = Path::new(SOURCE);
        let before = fs::read_to_string(path).unwrap();
        fs::write(path, before.replace("enabled=1", "enabled=0")).unwrap();
        let disabled = measure(root, exe, &hash, false);
        let observed = observation(root);
        fs::write(path, before).unwrap();
        assert_eq!(disabled.enrollment, Enrollment::Disabled);
        assert!(!disabled.can_check());
        assert!(disabled.can_manage_islands());
        assert_eq!(observed.update_channel, "disabled");
        assert!(!measure(root, exe, &hash, true).can_check());
    }
}
