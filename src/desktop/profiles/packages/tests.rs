use super::super::{BackgroundLook, Domain, ProfileLibrary, WallpaperLook};
use super::*;
use crate::desktop::store::tests::Fixture;
use std::fs;
#[test]
#[ignore = "owned bounded image child entrypoint"]
fn worker_entry() {
    assets::main();
}

#[test]
fn fb18_image_child_cancellation_drop_and_deadline_reap_the_exact_worker() {
    if crate::displays::test_support::isolated(
        "desktop::profiles::packages::tests::fb18_image_child_cancellation_drop_and_deadline_reap_the_exact_worker",
    ) {
        return;
    }
    // This test runs alone in its private process; the marker is inherited only
    // by its owned child and the ready callback proves that child actually started.
    unsafe {
        std::env::set_var("HALCYON_PACKAGE_WORKER_STALL", "1");
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for mode in ["cancel", "drop", "deadline"] {
                let (sender, cancel) = tokio::sync::watch::channel(false);
                let (ready, pid) = tokio::sync::oneshot::channel();
                let job = tokio::spawn(crate::desktop::wallpaper::media::worker::package_image(
                    br#"{"Normalize":"/dev/null"}"#.to_vec(),
                    cancel,
                    move |pid| {
                        let _ = ready.send(pid);
                    },
                ));
                let pid = tokio::time::timeout(std::time::Duration::from_secs(2), pid)
                    .await
                    .unwrap()
                    .unwrap();
                let start = std::time::Instant::now();
                if mode == "drop" {
                    job.abort();
                    let _ = job.await;
                } else {
                    if mode == "cancel" {
                        sender.send(true).unwrap();
                    }
                    assert!(job.await.unwrap().is_none());
                }
                tokio::time::timeout(std::time::Duration::from_millis(100), async {
                    while std::path::Path::new(&format!("/proc/{pid}")).exists() {
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                })
                .await
                .unwrap();
                if mode != "deadline" {
                    assert!(start.elapsed() < std::time::Duration::from_millis(100));
                } else {
                    assert!(start.elapsed() < std::time::Duration::from_secs(5));
                }
            }
        });
}

#[test]
fn fb18_asset_publication_without_reference_is_reused_after_interruption() {
    let f = Fixture::new("fb18-after-publication");
    let store = f.store();
    let (package, _) = Package::build(&source(&f), true, credits()).unwrap();
    fs::write(f.0.join("interrupt-package-publish"), b"fixture").unwrap();
    assert!(package.import(&store).is_err());
    assert!(ProfileLibrary::new(&store).list().unwrap().is_empty());
    let pool = store.state_subpath(ASSET_DIR, None).unwrap();
    assert_eq!(fs::read_dir(&pool).unwrap().count(), 1);
    fs::remove_file(f.0.join("interrupt-package-publish")).unwrap();
    let id = package.import(&store).unwrap();
    assert!(ProfileLibrary::new(&store).load(&id).is_ok());
    assert_eq!(fs::read_dir(&pool).unwrap().count(), 1);
}
fn credits() -> Credits {
    Credits {
        author: "Example artist".into(),
        license: "CC0-1.0".into(),
        source: "https://example.org/art".into(),
    }
}
fn source(f: &Fixture) -> Profile {
    let path = f.0.join("original.png");
    image::RgbaImage::from_pixel(32, 24, image::Rgba([30, 80, 90, 255]))
        .save(&path)
        .unwrap();
    let mut profile = Profile::new("Portable morning").unwrap();
    profile.wallpaper = Some(WallpaperLook {
        background: Some(BackgroundLook {
            source: Some(SourceLook::Image(path)),
            ..Default::default()
        }),
        ..Default::default()
    });
    profile.widget_style = Some(serde_json::json!({"clock":{"face":"analog","surface":"panel"}}));
    profile
}
fn imported_path(profile: &Profile) -> &std::path::Path {
    match profile
        .wallpaper
        .as_ref()
        .unwrap()
        .background
        .as_ref()
        .unwrap()
        .source
        .as_ref()
        .unwrap()
    {
        SourceLook::Image(p) => p,
        _ => panic!("not a local immutable asset"),
    }
}
#[test]
fn fb18_package_roundtrip_is_independent_of_source_and_keeps_credits_through_undo() {
    let source_root = Fixture::new("fb18-source");
    let target = Fixture::new("fb18-target");
    let profile = source(&source_root);
    let (package, omitted) = Package::build(&profile, true, credits()).unwrap();
    assert!(omitted.is_empty());
    let bytes = package.encode().unwrap();
    assert!(
        !String::from_utf8_lossy(&bytes).contains(&source_root.0.to_string_lossy().to_string())
    );
    fs::remove_file(imported_path(&profile)).unwrap();
    let store = target.store();
    let id = Package::parse(&bytes).unwrap().import(&store).unwrap();
    let local = ProfileLibrary::new(&store).load(&id).unwrap();
    assert_eq!(local.package.as_ref().unwrap(), &package.origin);
    assert!(imported_path(&local).starts_with(store.state_dir()));
    assert_eq!(
        digest(&fs::read(imported_path(&local)).unwrap()),
        package.wallpaper.as_ref().unwrap().sha256
    );
    let original = target.tree();
    let installed = super::super::Installed::discover(&store);
    let proposal = super::super::plan(&store, &installed, &local, &[Domain::Wallpaper].into());
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    store.inject(Some(crate::desktop::store::Fault::InterruptBeforeRecord));
    assert!(super::super::apply(&store, &proposal).is_err());
    store.inject(None);
    assert!(store.pending_recovery().unwrap().is_some());
    store.recover().unwrap();
    assert!(imported_path(&local).exists());
    let change = super::super::apply(&store, &proposal).unwrap();
    store.undo_latest(&change.id).unwrap();
    assert!(imported_path(&local).exists());
    ProfileLibrary::new(&store).delete(&id).unwrap();
    assert!(imported_path(&local).exists());
    let key = crate::desktop::wallpaper::shared_key();
    assert_eq!(
        target.tree().get(&store.user_path(&key)),
        original.get(&store.user_path(&key))
    );
}
#[test]
fn fb18_package_asset_changed_after_review_blocks_apply_without_desktop_writes() {
    let f = Fixture::new("fb18-recheck");
    let store = f.store();
    let (package, _) = Package::build(&source(&f), true, credits()).unwrap();
    let id = package.import(&store).unwrap();
    let local = ProfileLibrary::new(&store).load(&id).unwrap();
    let proposal = super::super::plan(
        &store,
        &super::super::Installed::discover(&store),
        &local,
        &[Domain::Wallpaper].into(),
    );
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    fs::write(imported_path(&local), b"changed").unwrap();
    let before = f.tree();
    assert!(super::super::apply(&store, &proposal).is_err());
    assert_eq!(f.tree(), before);
    let refreshed = super::super::plan(
        &store,
        &super::super::Installed::discover(&store),
        &local,
        &[Domain::Wallpaper].into(),
    );
    assert!(!refreshed.can_apply());
}
#[test]
fn fb18_package_requires_picture_permission_and_strips_local_assignments() {
    let f = Fixture::new("fb18-portability");
    let mut profile = source(&f);
    let wall = profile.wallpaper.as_mut().unwrap();
    wall.displays = Some(super::super::DisplaysName::Separate);
    wall.per_display
        .get_or_insert_with(Default::default)
        .insert("PRIVATE-DISPLAY".into(), BackgroundLook::default());
    assert!(Package::build(&profile, true, Credits::default()).is_err());
    let (package, omitted) = Package::build(&profile, false, Credits::default()).unwrap();
    assert!(package.wallpaper.is_none());
    assert!(omitted.contains(&super::super::OWN_WALLPAPER));
    let bytes = package.encode().unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE-DISPLAY"));
    assert!(!String::from_utf8_lossy(&bytes).contains("original.png"));
    for source in [
        "file:///etc/passwd",
        "https://user:password@example.org",
        "http://example.org",
    ] {
        let mut credit = credits();
        credit.source = source.into();
        assert!(credit.validate().is_err());
    }
}
#[test]
fn fb18_package_rejects_unknown_fields_versions_paths_and_tampering_before_writes() {
    let f = Fixture::new("fb18-admission");
    let (package, _) = Package::build(&source(&f), true, credits()).unwrap();
    let value = serde_json::to_value(&package).unwrap();
    for what in ["version", "path", "command", "hash", "revision", "binding"] {
        let mut v = value.clone();
        match what {
            "version" => v["version"] = 99.into(),
            "path" => v["wallpaper"]["path"] = "../../outside".into(),
            "command" => v["command"] = "sh".into(),
            "hash" => v["wallpaper"]["sha256"] = "0".repeat(64).into(),
            "revision" => v["profile"]["name"] = "Changed".into(),
            _ => {
                v["profile"]["widget_style"]["location"] =
                    serde_json::json!({"name":"SECRET","latitude":0,"longitude":0})
            }
        };
        let before = f.tree();
        assert!(
            Package::parse(&serde_json::to_vec(&v).unwrap()).is_err(),
            "{what}"
        );
        assert_eq!(before, f.tree());
    }
    assert!(Package::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
}
#[test]
fn fb18_embedded_manifest_is_bounded_before_building_json_trees() {
    let (package, _) =
        Package::build(&Profile::new("Bounded").unwrap(), false, Credits::default()).unwrap();
    let origin = serde_json::to_string(&package.origin).unwrap();
    let profile = format!(
        r#"{{"format":"desktop-cards-profile","version":14,"name":"Oversize","widget_style":{{"z":[{}]}}}}"#,
        std::iter::repeat_n("0", 150_000)
            .collect::<Vec<_>>()
            .join(",")
    );
    assert!(profile.len() > super::super::MAX_PROFILE_BYTES);
    let bytes=format!(r#"{{"format":"halcyon-setup-package","version":1,"origin":{origin},"profile":{profile},"wallpaper":null}}"#).into_bytes();
    assert!(bytes.len() < MAX_BYTES);
    assert!(
        Package::parse(&bytes)
            .unwrap_err()
            .contains("The look in this file is too large")
    );
}
#[test]
fn fb18_package_profile_version_and_parent_revision_are_explicit() {
    let f = Fixture::new("fb18-revision");
    let (package, _) = Package::build(&source(&f), true, credits()).unwrap();
    let store = f.store();
    let id = package.import(&store).unwrap();
    let mut local = ProfileLibrary::new(&store).load(&id).unwrap();
    assert_eq!(local.written_version(), 14);
    local.version = 13;
    assert!(local.validate().is_err());
    local.version = 14;
    let (copy, _) = Package::build(
        &local,
        true,
        local.package.as_ref().unwrap().credits.clone(),
    )
    .unwrap();
    assert_ne!(copy.origin.identity.id, package.origin.identity.id);
    assert_eq!(copy.origin.parent, Some(package.origin.identity.clone()));
    assert_eq!(copy.origin.credits, package.origin.credits);
    let plain = super::super::export(&local).unwrap();
    assert!(plain.omitted.contains(&super::super::OWN_WALLPAPER));
    assert_eq!(
        super::super::import(&plain.bytes).unwrap().package,
        local.package
    );
    let mut old = Profile::new("Still old").unwrap();
    old.widget_style = Some(serde_json::json!({"clock":{"face":"digital"}}));
    assert_eq!(old.written_version(), 9);
}
#[test]
fn fb18_asset_bounds_reject_devices_symlinks_and_large_dimensions() {
    let f = Fixture::new("fb18-image-bounds");
    assert!(assets::normalize(std::path::Path::new("/dev/zero")).is_err());
    let link = f.0.join("link.png");
    std::os::unix::fs::symlink("/dev/zero", &link).unwrap();
    assert!(assets::normalize(&link).is_err());
    let huge = f.0.join("huge.png");
    image::RgbaImage::new(8193, 1).save(&huge).unwrap();
    assert!(assets::normalize(&huge).is_err());
    assert!(assets::check_png(b"<svg/>").is_err());
    let profile = source(&f);
    let bytes = assets::normalize(imported_path(&profile)).unwrap();
    assert!(image::load_from_memory(&bytes).is_ok());
}
#[test]
fn fb18_export_removes_png_text_metadata() {
    let f = Fixture::new("fb18-metadata");
    let profile = source(&f);
    let path = imported_path(&profile);
    let mut bytes = fs::read(path).unwrap();
    let iend = bytes.split_off(bytes.len() - 12);
    let data = b"Description\0PRIVATE-METADATA";
    let mut chunk = b"tEXt".to_vec();
    chunk.extend_from_slice(data);
    let mut crc = 0xffff_ffffu32;
    for b in &chunk {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&chunk);
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes.extend_from_slice(&iend);
    fs::write(path, &bytes).unwrap();
    assert!(image::load_from_memory(&bytes).is_ok());
    let normalized = assets::normalize(path).unwrap();
    assert!(!normalized.windows(16).any(|s| s == b"PRIVATE-METADATA"));
    assert_eq!(
        image::load_from_memory(&normalized).unwrap().into_rgba8(),
        image::load_from_memory(&bytes).unwrap().into_rgba8()
    );
}
#[test]
fn fb18_asset_interrupted_staging_recovers_without_dangling_reference_or_replacement() {
    let f = Fixture::new("fb18-staging");
    let (package, _) = Package::build(&source(&f), true, credits()).unwrap();
    let store = f.store();
    fs::write(f.0.join("interrupt-package-stage"), b"fixture").unwrap();
    let failure = package.import(&store);
    fs::remove_file(f.0.join("interrupt-package-stage")).unwrap();
    assert!(failure.is_err());
    assert!(ProfileLibrary::new(&store).list().unwrap().is_empty());
    let dir = store.state_subpath(ASSET_DIR, None).unwrap();
    fs::write(dir.join("staged-picture.tmp"), b"interrupted").unwrap();
    let id = package.import(&store).unwrap();
    assert!(!dir.join("staged-picture.tmp").exists());
    let local = ProfileLibrary::new(&store).load(&id).unwrap();
    let path = imported_path(&local);
    fs::write(path, b"external edit").unwrap();
    assert!(package.import(&store).is_err());
    assert_eq!(fs::read(path).unwrap(), b"external edit");
    assert_eq!(ProfileLibrary::new(&store).list().unwrap().len(), 1);
}
#[test]
fn fb18_asset_pool_is_bounded_and_symlink_directory_is_refused() {
    let f = Fixture::new("fb18-pool");
    let (package, _) = Package::build(&source(&f), true, credits()).unwrap();
    let store = f.store();
    let dir = store.state_subpath(ASSET_DIR, None).unwrap();
    fs::create_dir_all(&dir).unwrap();
    for i in 0..64 {
        fs::write(dir.join(format!("{i:064x}.png")), b"retained").unwrap();
    }
    assert!(package.import(&store).is_err());
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 64);
    let other = Fixture::new("fb18-pool-symlink");
    fs::create_dir_all(other.store().state_dir()).unwrap();
    std::os::unix::fs::symlink(&dir, other.store().state_dir().join(ASSET_DIR)).unwrap();
    assert!(package.import(&other.store()).is_err());
}
