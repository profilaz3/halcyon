use super::*;
// No standard session/system config or service directories: nothing can activate
// a real desktop shim/daemon when testing the missing-service path.
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
            "halcyon-update-bus-{}-{}",
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
use std::sync::{Arc, Mutex};
use zbus::{Connection, ObjectServer, object_server::SignalEmitter, zvariant::OwnedObjectPath};
const PK: &str = "org.freedesktop.PackageKit";
const TX: &str = "org.freedesktop.PackageKit.Transaction";
#[derive(Clone)]
struct Script {
    error: Option<u32>,
    finish: Option<u32>,
    malformed: bool,
    packages: Vec<String>,
    allow_cancel: bool,
    delay_reply: bool,
    info: u32,
    flags: u64,
    files: usize,
}
impl Default for Script {
    fn default() -> Self {
        Self {
            error: None,
            finish: Some(1),
            malformed: false,
            packages: vec![],
            allow_cancel: true,
            delay_reply: false,
            info: 2,
            flags: 0,
            files: 1,
        }
    }
}
#[derive(Default)]
struct Trace {
    calls: Vec<String>,
    paths: usize,
    active: Vec<OwnedObjectPath>,
}
struct Root {
    trace: Arc<Mutex<Trace>>,
    scripts: Vec<Script>,
}
#[zbus::interface(name = "org.freedesktop.PackageKit")]
impl Root {
    fn get_transaction_list(&self) -> Vec<OwnedObjectPath> {
        self.trace.lock().unwrap().active.clone()
    }
    #[zbus(property)]
    fn roles(&self) -> u64 {
        1 << 10
    }
    async fn create_transaction(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let index = {
            let mut t = self.trace.lock().unwrap();
            let n = t.paths;
            t.paths += 1;
            t.calls.push("CreateTransaction".into());
            n
        };
        let script = self
            .scripts
            .get(index)
            .cloned()
            .ok_or_else(|| zbus::fdo::Error::Failed("unexpected transaction".into()))?;
        let path = format!("/{}_fixture", index + 1);
        server
            .at(
                path.as_str(),
                Transaction {
                    trace: self.trace.clone(),
                    script,
                },
            )
            .await?;
        Ok(OwnedObjectPath::try_from(path).unwrap())
    }
}
struct Transaction {
    trace: Arc<Mutex<Trace>>,
    script: Script,
}
impl Transaction {
    async fn signals(&self, emitter: &SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        let c = emitter.connection();
        let p = emitter.path();
        for id in &self.script.packages {
            c.emit_signal(
                None::<&str>,
                p,
                TX,
                "Package",
                &(self.script.info, id, "fixture"),
            )
            .await?;
        }
        if self.script.malformed {
            c.emit_signal(None::<&str>, p, TX, "Package", &"wrong signature")
                .await?;
        }
        if let Some(error) = self.script.error {
            c.emit_signal(
                None::<&str>,
                p,
                TX,
                "ErrorCode",
                &(error, "fixture details"),
            )
            .await?;
        }
        if let Some(exit) = self.script.finish {
            c.emit_signal(None::<&str>, p, TX, "Finished", &(exit, 0u32))
                .await?;
        }
        // Emitting before returning deliberately tests subscribe-before-call.
        if self.script.delay_reply {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        Ok(())
    }
}
#[zbus::interface(name = "org.freedesktop.PackageKit.Transaction")]
impl Transaction {
    async fn install_files(
        &self,
        flags: u64,
        files: Vec<String>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        assert_eq!(flags, self.script.flags);
        assert_eq!(files.len(), self.script.files);
        assert!(files.iter().all(|p| std::path::Path::new(p).is_file()));
        self.trace.lock().unwrap().calls.push("InstallFiles".into());
        self.signals(&emitter).await
    }
    fn set_hints(&self, hints: Vec<String>) {
        assert_eq!(hints, ["interactive=true", "background=false"]);
        self.trace.lock().unwrap().calls.push("SetHints".into());
    }
    async fn refresh_cache(
        &self,
        force: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        assert!(!force);
        self.trace
            .lock()
            .unwrap()
            .calls
            .push("RefreshCache(false)".into());
        self.signals(&emitter).await
    }
    async fn get_updates(
        &self,
        filter: u64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        assert_eq!(filter, 2);
        self.trace
            .lock()
            .unwrap()
            .calls
            .push("GetUpdates(2)".into());
        self.signals(&emitter).await
    }
    #[zbus(property)]
    fn allow_cancel(&self) -> bool {
        self.script.allow_cancel
    }
    fn cancel(&self) {
        self.trace.lock().unwrap().calls.push("Cancel".into());
    }
}
async fn service(bus: &Bus, scripts: Vec<Script>) -> (Connection, Connection, Arc<Mutex<Trace>>) {
    let trace = Arc::new(Mutex::new(Trace::default()));
    let service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(PK)
        .unwrap()
        .serve_at(
            "/org/freedesktop/PackageKit",
            Root {
                trace: trace.clone(),
                scripts,
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    (service, client, trace)
}
fn apt() -> Identity {
    Identity {
        format_version: 1,
        enrollment: Enrollment::Enrolled,
        channel: UpdateChannel::Apt,
        reason: "System updates · checks only when you ask.",
        running_version: "0.2.0".into(),
        installed_version: Some("0.2.0".into()),
        islands_version: None,
        architecture: Some("amd64".into()),
        store: Some("/usr/share/applications/com.system76.CosmicStore.desktop".into()),
        islands_match: identity::IslandsMatch::Ready,
        evidence: Some("fixture verified identity".into()),
    }
}
#[tokio::test]
async fn fresh_transactions_capture_early_signals_and_use_installed_protocol() {
    let bus = Bus::new();
    let packages = vec![
        "cosmic-ext-halcyon;0.2.0-10;amd64;not-an-origin".into(),
        "cosmic-ext-halcyon-islands;8.0;amd64;archive".into(),
    ];
    let (_service, client, trace) = service(
        &bus,
        vec![
            Script {
                delay_reply: true,
                ..Default::default()
            },
            Script {
                packages: packages.clone(),
                delay_reply: true,
                ..Default::default()
            },
        ],
    )
    .await;
    let (_sender, cancel) = watch::channel(false);
    let ids = packagekit::check_on(
        &client,
        cancel,
        tokio::time::Instant::now() + Duration::from_secs(3),
    )
    .await
    .unwrap();
    assert_eq!(ids, packages);
    assert_eq!(
        trace.lock().unwrap().calls,
        [
            "CreateTransaction",
            "SetHints",
            "RefreshCache(false)",
            "CreateTransaction",
            "SetHints",
            "GetUpdates(2)"
        ]
    );
    assert_eq!(
        packagekit::candidate(&apt(), &ids)
            .await
            .unwrap()
            .as_deref(),
        Some("0.2.0-10")
    );
}
#[tokio::test]
async fn refresh_failures_never_query_or_claim_current() {
    let bus = Bus::new();
    for (script, expected) in [
        (
            Script {
                error: Some(2),
                ..Default::default()
            },
            Failure::Offline,
        ),
        (
            Script {
                error: Some(48),
                ..Default::default()
            },
            Failure::Authorization,
        ),
        (
            Script {
                error: Some(30),
                ..Default::default()
            },
            Failure::Untrusted,
        ),
        (
            Script {
                finish: Some(3),
                ..Default::default()
            },
            Failure::Cancelled,
        ),
        (
            Script {
                finish: Some(2),
                ..Default::default()
            },
            Failure::Failed,
        ),
        (
            Script {
                malformed: true,
                ..Default::default()
            },
            Failure::InvalidResponse,
        ),
    ] {
        let (server, client, trace) = service(&bus, vec![script]).await;
        let (_sender, cancel) = watch::channel(false);
        let result = packagekit::check_on(
            &client,
            cancel,
            tokio::time::Instant::now() + Duration::from_secs(2),
        )
        .await;
        assert_eq!(result, Err(expected));
        assert_eq!(trace.lock().unwrap().paths, 1);
        client.close().await.unwrap();
        server.close().await.unwrap();
    }
}
#[tokio::test]
async fn method_reply_is_not_completion_timeout_and_cancel_are_bounded() {
    for allow_cancel in [true, false] {
        for explicit in [true, false] {
            let bus = Bus::new();
            let (_service, client, trace) = service(
                &bus,
                vec![Script {
                    finish: None,
                    allow_cancel,
                    ..Default::default()
                }],
            )
            .await;
            let (sender, cancel) = watch::channel(false);
            let result = packagekit::check_on(
                &client,
                cancel,
                tokio::time::Instant::now() + Duration::from_millis(180),
            );
            let stop = async {
                if explicit {
                    tokio::time::timeout(Duration::from_secs(1), async {
                        while !trace
                            .lock()
                            .unwrap()
                            .calls
                            .iter()
                            .any(|v| v == "RefreshCache(false)")
                        {
                            tokio::time::sleep(Duration::from_millis(2)).await;
                        }
                    })
                    .await
                    .unwrap();
                    sender.send(true).unwrap();
                }
            };
            let (result, _) = tokio::join!(result, stop);
            assert_eq!(
                result,
                Err(if explicit {
                    Failure::Cancelled
                } else {
                    Failure::Timeout
                })
            );
            assert_eq!(
                trace.lock().unwrap().calls.iter().any(|v| v == "Cancel"),
                allow_cancel
            );
            assert_eq!(trace.lock().unwrap().paths, 1);
        }
    }
}
#[tokio::test]
async fn missing_service_and_query_error_discard_candidates() {
    let bus = Bus::new();
    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let (_sender, cancel) = watch::channel(false);
    assert_eq!(
        packagekit::check_on(
            &client,
            cancel,
            tokio::time::Instant::now() + Duration::from_secs(1)
        )
        .await,
        Err(Failure::MissingPackageKit)
    );
    let (_service, client, _) = service(
        &bus,
        vec![
            Script::default(),
            Script {
                packages: vec!["cosmic-ext-halcyon;9;amd64;archive".into()],
                error: Some(5),
                ..Default::default()
            },
        ],
    )
    .await;
    let (_sender, cancel) = watch::channel(false);
    assert_eq!(
        packagekit::check_on(
            &client,
            cancel,
            tokio::time::Instant::now() + Duration::from_secs(1)
        )
        .await,
        Err(Failure::Untrusted)
    );
}
#[tokio::test]
async fn actual_debian_ordering_architecture_absence_and_malformed_candidates() {
    for (installed, candidate, newer) in [
        ("0.2.0", "0.2.0~rc1", false),
        ("0.2.0-9", "0.2.0-10", true),
        ("9.0", "1:0.1", true),
        ("1:1.0", "9.0", false),
        ("0.2.0", "0.2.00", false),
    ] {
        let mut identity = apt();
        identity.installed_version = Some(installed.into());
        let result = packagekit::candidate(
            &identity,
            &[format!("cosmic-ext-halcyon;{candidate};amd64;repo")],
        )
        .await
        .unwrap();
        assert_eq!(result.is_some(), newer);
    }
    assert_eq!(
        packagekit::candidate(
            &apt(),
            &[
                "cosmic-ext-halcyon-islands;9;amd64;repo".into(),
                "cosmic-ext-halcyon;9;arm64;repo".into()
            ]
        )
        .await,
        Ok(None)
    );
    for ids in [
        vec!["broken".into()],
        vec!["cosmic-ext-halcyon;9;amd64;repo".into(); 2],
        vec!["cosmic-ext-halcyon;1:bad:version;amd64;repo".into()],
    ] {
        assert_eq!(
            packagekit::candidate(&apt(), &ids).await,
            Err(Failure::InvalidResponse)
        );
    }
}
#[test]
fn obsolete_tokens_and_close_invalidate_results_without_erasing_new_request() {
    let mut state = Updates::default();
    let (old, old_cancel) = state.begin();
    state.cancel();
    assert!(*old_cancel.borrow());
    state.finish(old, Ok(Some("9".into())));
    assert_eq!(state.check, Check::Failed(Failure::Cancelled));
    let (new, _) = state.begin();
    state.finish(old, Ok(None));
    assert_eq!(state.check, Check::Checking);
    state.finish(new, Err(Failure::Offline));
    assert_eq!(state.check, Check::Failed(Failure::Offline));
    let (_, cancel) = state.begin();
    drop(state);
    assert!(*cancel.borrow());
}

fn ui_child() -> bool {
    const NAME: &str =
        "settings::updates::tests::native_updates_clicks_renders_failure_unknown_and_absence";
    if std::env::var("HALCYON_UPDATES_UI_TEST").as_deref() == Ok("1") {
        return false;
    }
    let root = std::env::temp_dir().join(format!("halcyon-updates-ui-{}", std::process::id()));
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    // Dock activation needs a configured bar. Supply its defaults here so the
    // fixture behaves identically with and without COSMIC installed on the host.
    for (namespace, key, value) in [
        (
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        ),
        ("com.system76.CosmicPanel.Dock", "anchor", "Bottom"),
        (
            "com.system76.CosmicPanel.Dock",
            "plugins_center",
            "Some([\"com.system76.CosmicAppList\"])",
        ),
        ("com.system76.CosmicPanel.Panel", "anchor", "Top"),
        (
            "com.system76.CosmicPanel.Panel",
            "plugins_wings",
            "Some(([], [\"com.system76.CosmicAppletTime\"]))",
        ),
    ] {
        let file = root
            .join("desktop/system/cosmic")
            .join(namespace)
            .join("v1")
            .join(key);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, value).unwrap();
    }
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    std::fs::write(
        root.join("config.json"),
        r#"{"setup_dismissed":true,"onboarded":true,"enabled_widgets":[]}"#,
    )
    .unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture"])
        .env("HALCYON_UPDATES_UI_TEST", "1")
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", root.join("desktop"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_DATA_DIRS", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("DCONF_PROFILE", root.join("dconf-profile"))
        .output()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    true
}
fn ui_click(
    app: &Settings,
    native: &mut crate::native_test::Native,
    id: &str,
) -> Vec<SettingsMessage> {
    use cosmic::iced::{Event, mouse};
    native.scroll(app.settings_view(), "settings-content", 0.);
    let target = native.bounds(app.settings_view(), id);
    if target.y + target.height > native.size.height - 64. {
        native.scroll(
            app.settings_view(),
            "settings-content",
            (target.y - 240.).max(0.),
        );
    }
    let bounds = native.bounds(app.settings_view(), id);
    assert!(
        bounds.x >= 0.
            && bounds.y >= 0.
            && bounds.x + bounds.width <= native.size.width
            && bounds.y + bounds.height <= native.size.height,
        "{id}: {bounds:?}"
    );
    let mut messages = Vec::new();
    for event in [
        mouse::Event::CursorMoved {
            position: bounds.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        messages.extend(
            native
                .event(app.settings_view(), Event::Mouse(event), bounds.center())
                .0,
        );
    }
    messages
}
#[test]
fn native_updates_clicks_renders_failure_unknown_and_absence() {
    if ui_child() {
        return;
    }
    use crate::{
        desktop::islands::Installation,
        native_test::Native,
        settings::{Page, WindowControl},
    };
    use cosmic::{Application, app::Core, iced::Size};
    for size in [Size::new(820., 600.), Size::new(1024., 760.)] {
        let (mut app, init) = Settings::init(Core::default(), Page::General);
        drop(init); // No background worker, network or host bus in the UI fixture.
        app.window = size;
        app.service_known = true;
        app.desktop.islands.installation = Installation::Absent;
        app.updates.identity = Some(apt());
        let mut native = Native::new(size);
        for message in ui_click(&app, &mut native, "app-settings-updates") {
            drop(app.update(message));
        }
        assert_eq!(
            app.app_settings_page,
            crate::settings::AppSettingsPage::Updates
        );
        let theme = crate::settings::controls_theme(&app.config);
        let suffix = format!("{}x{}", size.width as u32, size.height as u32);
        native.capture(
            app.settings_view(),
            &theme,
            &format!("updates-apt-{suffix}"),
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "get-islands")
                .is_none()
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "install-islands-confirm")
                .is_none()
        );
        app.desktop.islands.installation = Installation::Installed {
            provenance: crate::desktop::islands::Provenance::Package,
            enabled: false,
            compatible: None,
            status: Some(crate::desktop::islands::CompatibilityStatus::Disabled),
            diagnostics: String::new(),
        };
        assert!(
            ui_click(&app, &mut native, "enable-islands")
                .iter()
                .any(|m| matches!(
                    m,
                    SettingsMessage::Updates(Message::Islands(islands::Message::Enable))
                ))
        );
        native.capture(
            app.settings_view(),
            &theme,
            &format!("islands-enable-{suffix}"),
        );
        app.desktop.open();
        assert!(
            app.desktop.current().is_some_and(|draft| draft.installed()),
            "fixture provides the configured dock without host COSMIC defaults"
        );
        app.page = Page::DesktopDock;
        crate::settings::navigation::select(&mut app.nav_model, app.page);
        app.installation_busy = false;
        app.collapsed_scenes.push(Page::DesktopDock);
        // Open remeasures the fixture. Restore only its declared package state.
        app.desktop.islands.installation = Installation::Installed {
            provenance: crate::desktop::islands::Provenance::Package,
            enabled: false,
            compatible: None,
            status: Some(crate::desktop::islands::CompatibilityStatus::Disabled),
            diagnostics: String::new(),
        };
        native.capture(
            app.settings_view(),
            &theme,
            &format!("dock-islands-off-{suffix}"),
        );
        assert!(
            ui_click(&app, &mut native, "enable-islands")
                .iter()
                .any(|m| matches!(
                    m,
                    SettingsMessage::Updates(Message::Islands(islands::Message::Enable))
                ))
        );
        if let Installation::Installed {
            enabled, status, ..
        } = &mut app.desktop.islands.installation
        {
            *enabled = true;
            *status = None;
        }
        native.capture(
            app.settings_view(),
            &theme,
            &format!("dock-islands-on-{suffix}"),
        );
        assert!(
            ui_click(&app, &mut native, "disable-islands")
                .iter()
                .any(|m| matches!(
                    m,
                    SettingsMessage::Updates(Message::Islands(islands::Message::Disable))
                ))
        );
        app.page = Page::General;
        crate::settings::navigation::select(&mut app.nav_model, app.page);
        if let Installation::Installed {
            enabled, status, ..
        } = &mut app.desktop.islands.installation
        {
            *enabled = false;
            *status = Some(crate::desktop::islands::CompatibilityStatus::Disabled);
        }
        for eligibility in [
            identity::IslandsMatch::VersionMismatch,
            identity::IslandsMatch::HelperMismatch,
            identity::IslandsMatch::Unknown,
        ] {
            app.updates.identity.as_mut().unwrap().islands_match = eligibility;
            assert!(
                native
                    .try_bounds(app.settings_view(), "enable-islands")
                    .is_none()
            );
            drop(app.update_updates(Message::Islands(islands::Message::Enable)));
            assert_eq!(app.updates.islands.phase, islands::Phase::Idle);
            assert!(
                app.updates
                    .identity
                    .as_ref()
                    .unwrap()
                    .islands_activation_notice()
                    .is_some()
            );
        }
        drop(app.update_updates(Message::Islands(islands::Message::Recheck)));
        assert!(
            app.updates.identity.is_none(),
            "Recheck must renew package measurements"
        );
        app.updates.identity = Some(apt());
        app.desktop.islands.installation = Installation::Absent;
        let messages = ui_click(&app, &mut native, "updates-check");
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, SettingsMessage::Updates(Message::Check)))
        );
        for message in messages {
            drop(app.update(message));
        }
        assert_eq!(app.updates.check, Check::Checking);
        let token = app.updates.generation;
        let cancel = app.updates.cancellation.as_ref().unwrap().subscribe();
        // Focus regained after the OS authorization dialog does not supersede
        // an active request, and cannot initiate another refresh.
        drop(app.update_updates(Message::Inspect));
        assert_eq!(app.updates.generation, token);
        native.capture(
            app.settings_view(),
            &theme,
            &format!("updates-checking-{suffix}"),
        );
        for message in ui_click(&app, &mut native, "updates-cancel") {
            drop(app.update(message));
        }
        assert!(*cancel.borrow());
        drop(app.update(SettingsMessage::Updates(Message::Checked(
            token,
            Ok(Some("9.0".into())),
        ))));
        assert_eq!(app.updates.check, Check::Failed(Failure::Cancelled));
        for error in [
            Failure::Offline,
            Failure::MissingPackageKit,
            Failure::Authorization,
            Failure::Untrusted,
            Failure::Timeout,
        ] {
            app.updates.check = Check::Failed(error.clone());
            native.text_bounds(app.settings_view(), error.label());
        }
        app.updates.check = Check::Failed(Failure::Offline);
        native.capture(
            app.settings_view(),
            &theme,
            &format!("updates-offline-{suffix}"),
        );
        app.updates.check = Check::Available("0.2.0-10".into());
        native.capture(
            app.settings_view(),
            &theme,
            &format!("updates-available-{suffix}"),
        );
        let store = ui_click(&app, &mut native, "updates-store");
        assert!(
            store
                .iter()
                .any(|m| matches!(m, SettingsMessage::Updates(Message::OpenStore)))
        ); // inspect click, never launch host Store
        app.updates.identity.as_mut().unwrap().store = None;
        assert!(
            native
                .try_bounds(app.settings_view(), "updates-store")
                .is_none()
        );
        native.capture(
            app.settings_view(),
            &theme,
            &format!("updates-missing-store-{suffix}"),
        );
        for (channel, name) in [
            (UpdateChannel::ManualDeb, "manual"),
            (UpdateChannel::Source, "source"),
            (UpdateChannel::Unknown, "unknown"),
        ] {
            let identity = app.updates.identity.as_mut().unwrap();
            identity.channel = channel;
            identity.evidence = None;
            identity.reason = match channel {
                UpdateChannel::ManualDeb => "Manual package · system updates are not enabled.",
                UpdateChannel::Source => "Local source build · update from your source checkout.",
                _ => "The running app was replaced or removed. Restart Halcyon to recheck.",
            };
            if channel == UpdateChannel::Unknown {
                identity.installed_version = Some("0.2.0-10".into());
            }
            app.updates.check = Check::NotChecked;
            assert!(
                native
                    .try_bounds(app.settings_view(), "updates-check")
                    .is_none()
            );
            drop(app.update(SettingsMessage::Updates(Message::Check)));
            assert_eq!(app.updates.check, Check::NotChecked);
            let id = if channel == UpdateChannel::Unknown {
                "updates-information"
            } else {
                "updates-releases"
            };
            assert!(!ui_click(&app, &mut native, id).is_empty());
            native.capture(
                app.settings_view(),
                &theme,
                &format!("updates-{name}-{suffix}"),
            );
        }
        app.updates.identity = Some(apt());
        let path =
            std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap())
                .join("islands-installation.json");
        let installed = |active: &str, enabled: bool| {
            std::fs::write(&path,serde_json::to_vec(&serde_json::json!({"format":"halcyon-islands-installation","version":1,"installation":"installed","provenance":"package","enabled":enabled,"compatible":true,"status_version":1,"status":"Log out to activate","installed_generation":"a".repeat(64),"active_generation":active})).unwrap()).unwrap();
            Installation::discover(&crate::desktop::DesktopStore::from_env().unwrap())
        };
        assert!(!pending_islands(&installed(&"b".repeat(64), true)));
        assert!(!pending_islands(&installed(&"a".repeat(64), false)));
        app.desktop.islands.installation = installed(&"a".repeat(64), true);
        assert!(pending_islands(&app.desktop.islands.installation));
        native.capture(
            app.settings_view(),
            &theme,
            &format!("updates-pending-login-{suffix}"),
        );
        app.desktop.islands.installation = Installation::Absent;
        assert!(!pending_islands(&app.desktop.islands.installation));
        let (_, cancel) = app.updates.begin();
        drop(app.update(SettingsMessage::Window(WindowControl::Close)));
        assert!(*cancel.borrow());
        std::fs::remove_file(path).unwrap();
    }
}

#[tokio::test]
async fn stale_paths_and_foreign_senders_cannot_finish_a_new_query() {
    let bus = Bus::new();
    let (server, client, trace) = service(
        &bus,
        vec![
            Script::default(),
            Script {
                finish: None,
                ..Default::default()
            },
        ],
    )
    .await;
    let intruder = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let (_sender, cancel) = watch::channel(false);
    let check = packagekit::check_on(
        &client,
        cancel,
        tokio::time::Instant::now() + Duration::from_millis(250),
    );
    let signals = async {
        tokio::time::timeout(Duration::from_secs(1), async {
            while !trace
                .lock()
                .unwrap()
                .calls
                .iter()
                .any(|c| c == "GetUpdates(2)")
            {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        server
            .emit_signal(None::<&str>, "/1_fixture", TX, "Finished", &(1u32, 0u32))
            .await
            .unwrap();
        intruder
            .emit_signal(None::<&str>, "/2_fixture", TX, "Finished", &(1u32, 0u32))
            .await
            .unwrap();
    };
    let (result, _) = tokio::join!(check, signals);
    assert_eq!(result, Err(Failure::Timeout));
}

#[tokio::test]
async fn cancellation_before_start_and_isolated_app_never_contact_the_host() {
    let bus = Bus::new();
    let (_server, client, trace) = service(&bus, vec![Script::default()]).await;
    let (_sender, cancel) = watch::channel(true);
    assert_eq!(
        packagekit::check_on(
            &client,
            cancel,
            tokio::time::Instant::now() + Duration::from_secs(1)
        )
        .await,
        Err(Failure::Cancelled)
    );
    assert_eq!(trace.lock().unwrap().paths, 0);
    let (_sender, cancel) = watch::channel(false);
    assert_eq!(
        check_installation(apt(), cancel).await,
        Err(Failure::InstallationChanged)
    );
}

#[test]
fn successful_check_timestamp_survives_failure_and_notes_use_stable_tags() {
    let mut updates = Updates::default();
    updates.identity = Some(apt());
    let (token, _) = updates.begin();
    updates.finish(token, Ok(Some("0.3.0-1".into())));
    let checked = updates.last_checked;
    assert!(checked.is_some());
    assert_eq!(
        updates.notes_url().as_deref(),
        Some("https://github.com/profilaz3/halcyon/releases/tag/v0.3.0")
    );
    let (token, _) = updates.begin();
    updates.finish(token, Err(Failure::Offline));
    assert_eq!(updates.last_checked, checked);
}

#[tokio::test]
async fn bundled_updates_only_offer_the_main_package() {
    let mut installed = apt();
    installed.installed_version = Some("0.5.0".into());
    installed.islands_version = Some("0.5.0".into());
    let main = "cosmic-ext-halcyon;0.5.1;amd64;repo".to_owned();
    let bridge = "cosmic-ext-halcyon-islands;0.5.0;amd64;repo".to_owned();
    for packages in [vec![main.clone()], vec![main, bridge.clone()]] {
        assert_eq!(
            packagekit::candidate(&installed, &packages).await.unwrap(),
            Some("0.5.1".into())
        );
    }
    assert_eq!(
        packagekit::candidate(&installed, &[bridge]).await.unwrap(),
        None
    );
    assert_eq!(packagekit::candidate(&installed, &[]).await.unwrap(), None);
}
