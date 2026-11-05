use super::*;
struct Bus {
    daemon: std::process::Child,
    directory: std::path::PathBuf,
    address: String,
}
impl Bus {
    fn new() -> Self {
        use std::io::BufRead;
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "halcyon-activity-bus-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let config = directory.join("bus.conf");
        std::fs::write(&config,r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>"#).unwrap();
        let mut daemon = std::process::Command::new("/usr/bin/dbus-daemon")
            .arg("--config-file")
            .arg(config)
            .args(["--nofork", "--print-address"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        std::io::BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        assert!(!address.is_empty());
        Self {
            daemon,
            directory,
            address: address.trim().into(),
        }
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
struct Power;
#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Power {
    #[zbus(property)]
    fn is_present(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn warning_level(&self) -> u32 {
        4
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        4.0
    }
    #[zbus(property)]
    fn battery_level(&self) -> u32 {
        0
    }
}
#[tokio::test]
async fn reactive_private_power_reads_owner_loss_and_rediscovery() {
    let bus = Bus::new();
    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(
        read(&client, Source::Power, "org.freedesktop.UPower")
            .await
            .is_none()
    );
    let service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.UPower")
        .unwrap()
        .serve_at("/org/freedesktop/UPower/devices/DisplayDevice", Power)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        read(&client, Source::Power, "org.freedesktop.UPower").await,
        Some(Raw::Power {
            state: 2,
            level: 4,
            percent: Some(4)
        })
    );
    service
        .release_name("org.freedesktop.UPower")
        .await
        .unwrap();
    assert!(
        read(&client, Source::Power, "org.freedesktop.UPower")
            .await
            .is_none()
    );
    service
        .request_name("org.freedesktop.UPower")
        .await
        .unwrap();
    assert!(
        read(&client, Source::Power, "org.freedesktop.UPower")
            .await
            .is_some()
    );
}
#[test]
fn reactive_update_completion_requires_an_observed_owned_transaction() {
    let signal = |path, code| {
        zbus::Message::signal(path, "org.freedesktop.PackageKit.Transaction", "Finished")
            .unwrap()
            .build(&(code, 0_u32))
            .unwrap()
    };
    for (code, title) in [
        (1_u32, "Software operation complete"),
        (2, "Software operation failed"),
        (3, "Software operation cancelled"),
    ] {
        let mut t = Tracker::default();
        t.update(
            Source::Updates,
            Some(Raw::Updates {
                ids: vec!["/active".into()],
                count: 1,
                phase: 10,
                progress: Some(42),
            }),
            1000,
        );
        t.signal(&signal("/other", code), 2000);
        assert!(t.completion.is_none());
        t.signal(&signal("/active", code), 2000);
        let a = t
            .update(
                Source::Updates,
                Some(Raw::Updates {
                    ids: vec![],
                    count: 0,
                    phase: 0,
                    progress: None,
                }),
                2100,
            )
            .unwrap();
        assert_eq!(a.title, title);
        assert_eq!(a.expires_ms, Some(5000));
        assert!(
            t.update(Source::Updates, None, 2200).is_none(),
            "service loss clears stale outcome"
        );
    }
}
