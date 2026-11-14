use super::*;
use crate::activity_island::test_bus::Bus;
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

/// UPower's display device, changing as a test says.
struct Display {
    percentage: f64,
    /// Full reads: only GetAll asks for IsPresent.
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Display {
    #[zbus(property)]
    fn is_present(&self) -> bool {
        self.reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        true
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn warning_level(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        self.percentage
    }
    #[zbus(property)]
    fn battery_level(&self) -> u32 {
        0
    }
}

/// The battery is read once; its changes then arrive as signals carrying
/// the new values, and other devices' signals are not listened to. Before,
/// any UPower signal (any device) read the display device again, and a
/// 30 s timer read it regardless.
#[tokio::test]
async fn power_is_read_once_and_then_follows_its_own_changes() {
    use std::sync::atomic::Ordering::Relaxed;
    let bus = Bus::new();
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.UPower")
        .unwrap()
        .serve_at(
            POWER_PATH,
            Display {
                percentage: 9.0,
                reads: reads.clone(),
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
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let task =
        tokio::spawn(
            async move { follow(&client, Source::Power, "org.freedesktop.UPower", &tx).await },
        );
    let next = async |rx: &mut tokio::sync::mpsc::Receiver<(Source, Option<Activity>)>| {
        tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .ok()
            .flatten()
            .and_then(|(_, a)| a)
    };
    let low = next(&mut rx).await.unwrap();
    assert_eq!(
        (low.title.as_str(), low.cue.as_str()),
        ("Battery low", "9%")
    );
    let read = reads.load(Relaxed);
    assert_eq!(read, 1);

    // Another device's change is not listened to.
    service
        .emit_signal(
            None::<()>,
            "/org/freedesktop/UPower/devices/mouse_0",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                "org.freedesktop.UPower.Device",
                HashMap::from([("Percentage", zbus::zvariant::Value::from(50.0))]),
                Vec::<&str>::new(),
            ),
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .is_err()
    );

    // The display device's own change updates the island without a read.
    let device = service
        .object_server()
        .interface::<_, Display>(POWER_PATH)
        .await
        .unwrap();
    device.get_mut().await.percentage = 8.0;
    device
        .get()
        .await
        .percentage_changed(device.signal_emitter())
        .await
        .unwrap();
    assert_eq!(next(&mut rx).await.unwrap().cue, "8%");
    assert_eq!(reads.load(Relaxed), read, "a change caused a full read");

    // Losing UPower clears it; its return is read again.
    service
        .release_name("org.freedesktop.UPower")
        .await
        .unwrap();
    assert!(next(&mut rx).await.is_none());
    service
        .request_name("org.freedesktop.UPower")
        .await
        .unwrap();
    assert_eq!(next(&mut rx).await.unwrap().cue, "8%");
    assert_eq!(reads.load(Relaxed), read + 1);
    task.abort();
}

fn changed(
    path: &str,
    interface: &str,
    values: &[(&str, zbus::zvariant::Value<'_>)],
) -> zbus::Message {
    zbus::Message::signal(path, "org.freedesktop.DBus.Properties", "PropertiesChanged")
        .unwrap()
        .build(&(
            interface,
            values
                .iter()
                .map(|(k, v)| (*k, v.try_clone().unwrap()))
                .collect::<HashMap<_, _>>(),
            Vec::<&str>::new(),
        ))
        .unwrap()
}

/// Network and update signals change the model in place; only a new list
/// of connections or transactions reads again.
#[test]
fn network_and_update_signals_update_the_model_in_place() {
    use zbus::zvariant::Value;
    let props = |values: &[(&str, Value<'_>)]| -> Props {
        values
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    OwnedValue::try_from(v.try_clone().unwrap()).unwrap(),
                )
            })
            .collect()
    };
    let vpn = "/org/freedesktop/NetworkManager/ActiveConnection/2";
    let mut network = Model {
        root: props(&[("State", Value::from(70u32))]),
        children: vec![(
            vpn.into(),
            props(&[
                ("Type", Value::from("vpn")),
                ("Id", Value::from("Office")),
                ("State", Value::from(1u32)),
            ]),
        )],
    };
    assert_eq!(
        raw(Source::Network, &network),
        Some(Raw::Network {
            state: 70,
            vpn: String::new()
        })
    );
    // Signal strength and other devices are not news.
    let strength = changed(
        "/org/freedesktop/NetworkManager/AccessPoint/7",
        "org.freedesktop.NetworkManager.AccessPoint",
        &[("Strength", Value::from(40u8))],
    );
    assert_eq!(apply(Source::Network, &mut network, &strength), Step::Same);
    let state = changed(NM_PATH, NM, &[("State", Value::from(70u32))]);
    assert_eq!(apply(Source::Network, &mut network, &state), Step::Same);
    let up = changed(vpn, NM_ACTIVE, &[("State", Value::from(2u32))]);
    assert_eq!(apply(Source::Network, &mut network, &up), Step::Changed);
    assert_eq!(
        raw(Source::Network, &network),
        Some(Raw::Network {
            state: 70,
            vpn: "Office".into()
        })
    );
    let list = changed(
        NM_PATH,
        NM,
        &[(
            "ActiveConnections",
            Value::from(vec![zbus::zvariant::ObjectPath::try_from(vpn).unwrap()]),
        )],
    );
    assert_eq!(apply(Source::Network, &mut network, &list), Step::Reload);

    let job = "/org/freedesktop/PackageKit/transaction/1";
    let mut updates = Model {
        root: Props::new(),
        children: vec![(
            job.into(),
            props(&[
                ("Uid", Value::from(0u32)),
                ("Role", Value::from(10u32)),
                ("Status", Value::from(8u32)),
                ("Percentage", Value::from(10u32)),
            ]),
        )],
    };
    let progress = changed(job, PK_TRANSACTION, &[("Percentage", Value::from(55u32))]);
    assert_eq!(
        apply(Source::Updates, &mut updates, &progress),
        Step::Changed
    );
    assert!(matches!(
        raw(Source::Updates, &updates),
        Some(Raw::Updates {
            progress: Some(55),
            ..
        })
    ));
    let listed = zbus::Message::signal(PK_PATH, PK, "TransactionListChanged")
        .unwrap()
        .build(&(vec![job],))
        .unwrap();
    assert_eq!(apply(Source::Updates, &mut updates, &listed), Step::Reload);
}
