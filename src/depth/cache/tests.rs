use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "halcyon-scenes-{name}-{}-{}",
        std::process::id(),
        nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn png(image: image::DynamicImage) -> Vec<u8> {
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn files(source: [u32; 2]) -> Files {
    let [w, h] = super::super::scene::work_size(source);
    Files {
        stats: Stats {
            source,
            eta: 0.9,
            coverage: 0.4,
            top_touch: 0.,
            flat: false,
        },
        mask: png(GrayImage::from_pixel(w, h, image::Luma([255])).into()),
        edges: png(GrayImage::new(w, h).into()),
        picture: png(RgbImage::new(w, h).into()),
    }
}

#[test]
fn own_wallpaper_cache_keys_by_content_only() {
    let a = key(b"one picture");
    assert_eq!(a, key(b"one picture"), "a renamed copy is the same picture");
    assert_ne!(a, key(b"one picture."));
    assert_eq!(a.len(), 64);
    let root = PathBuf::from("/x");
    assert_eq!(dir(&root, &a), Some(root.join(format!("{a}-v{VERSION}"))));
    // Nothing but a key names a folder.
    assert_eq!(dir(&root, "../../etc"), None);
    assert_eq!(dir(&root, &a.to_uppercase()), None);
}

#[test]
fn own_wallpaper_cache_stores_whole_entries_and_checks_them() {
    let root = temp("store");
    let k = key(b"a");
    store(&root, &k, &files([64, 48])).unwrap();
    let entry = load(&root, &k).unwrap();
    assert_eq!(entry.mask.dimensions(), (64, 48));
    assert_eq!(entry.stats.source, [64, 48]);
    // Parts that do not belong together are not used.
    std::fs::write(
        dir(&root, &k).unwrap().join("edges.png"),
        png(GrayImage::new(10, 10).into()),
    )
    .unwrap();
    assert!(load(&root, &k).is_none());
    // A part larger than any entry can hold is refused before writing.
    let mut wrong = files([64, 48]);
    wrong.mask = vec![0; (MAX_FILE + 1) as usize];
    assert!(store(&root, &key(b"b"), &wrong).is_err());
    assert!(!exists(&root, &key(b"b")));
    // No leftovers.
    assert!(
        std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().starts_with('.'))
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn own_wallpaper_cache_stays_within_its_count_and_size() {
    let root = temp("prune");
    let keys: Vec<String> = (0..6u8).map(|i| key(&[i])).collect();
    for (i, k) in keys.iter().enumerate() {
        store(&root, k, &files([32, 32])).unwrap();
        // Older use first.
        let file = std::fs::File::open(dir(&root, k).unwrap().join("scene.json")).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i as u64))
            .unwrap();
    }
    // The oldest is used again, so it stays.
    touch(&root, &keys[0]);
    // Old versions and crashed stores go too.
    std::fs::create_dir_all(root.join(format!("{}-v0", keys[1]))).unwrap();
    let crashed = root.join(".leftover");
    std::fs::create_dir_all(&crashed).unwrap();
    std::fs::File::open(&crashed)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH)
        .unwrap();
    prune(&root, &keys[5], 3, u64::MAX);
    let left: Vec<bool> = keys.iter().map(|k| exists(&root, k)).collect();
    assert_eq!(left, [true, false, false, false, true, true]);
    assert!(!root.join(format!("{}-v0", keys[1])).exists());
    assert!(!crashed.exists());
    // A byte budget smaller than two entries keeps only the newest store.
    prune(&root, &keys[5], 32, 1);
    let left: Vec<bool> = keys.iter().map(|k| exists(&root, k)).collect();
    assert_eq!(left, [false, false, false, false, false, true]);
    clear(&root);
    assert!(!root.exists());
}

#[test]
fn own_wallpaper_cache_claims_one_analysis_at_a_time() {
    let root = temp("claim");
    let k = key(b"claimed");
    let first = claim(&root, &k).expect("free");
    assert!(
        claim(&root, &k).is_none(),
        "another process is analysing it"
    );
    // A claim survives pruning while it is fresh.
    prune(&root, &k, MAX_ENTRIES, MAX_BYTES);
    assert!(claim(&root, &k).is_none());
    drop(first);
    let second = claim(&root, &k).expect("given up when dropped");
    // A claim left by a process that stopped is taken over.
    std::fs::File::open(root.join(format!(".{k}.claim")))
        .unwrap()
        .set_modified(std::time::SystemTime::now() - STALE)
        .unwrap();
    std::mem::forget(second);
    assert!(claim(&root, &k).is_some());
    assert!(claim(&root, "../x").is_none());
    clear(&root);
}
