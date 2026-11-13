use super::*;
use std::fs;
use std::io::Write;

/// A temporary directory removed when dropped.
pub(crate) struct Temp(pub PathBuf);

impl Temp {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("halcyon-icon-packs-{name}-{}", unique()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    pub fn paths(&self) -> Paths {
        let icons = self.0.join("data/icons");
        let system = self.0.join("system/icons");
        fs::create_dir_all(&icons).unwrap();
        fs::create_dir_all(&system).unwrap();
        Paths {
            icons: icons.clone(),
            search: vec![icons, system],
            data: self.0.join("data/halcyon"),
            cache: self.0.join("cache/halcyon/icon-packs"),
            config: self.0.join("config/halcyon"),
        }
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// OCS

const PAGE: &str = include_str!("testdata/ocs-page.json");

#[test]
fn ocs_page_parses_from_a_saved_catalog_answer() {
    let page = ocs::parse(PAGE.as_bytes(), 0).unwrap();
    assert_eq!(page.total, 1253);
    assert_eq!(page.packs.len(), 8);
    assert!(page.more(ocs::PAGE_SIZE));
    let papirus = page.packs.iter().find(|pack| pack.id == 1166289).unwrap();
    assert_eq!(papirus.name, "Papirus");
    assert_eq!(papirus.author, "x-varlesh-x");
    // Downloads arrive as a string, the score as a number.
    assert_eq!(papirus.downloads, 2_786_767);
    assert_eq!(papirus.score, 93);
    assert_eq!(papirus.downloads_title(), "2.8M downloads");
    assert_eq!(papirus.rating_title(), "4.7 ★");
    assert_eq!(papirus.changed.len(), 10);
    assert!(papirus.page.starts_with("https://"));
    assert!(
        papirus
            .preview
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"))
    );
    assert!(!papirus.summary.contains('<'), "{}", papirus.summary);
    assert_eq!(papirus.files[0].name, "papirus-icon-theme-20250501.tar.gz");
    assert!(
        papirus
            .files
            .iter()
            .all(|file| ocs::archive_name(&file.name))
    );
    assert!(papirus.files[0].size > 1024 * 1024);
    let tela = page.packs.iter().find(|pack| pack.id == 1279924).unwrap();
    assert_eq!(tela.files[0].variant(), "Tela");
    assert_eq!(tela.files[1].variant(), "Tela-black");
    // The curated pick matches the author, not just the name.
    assert_eq!(
        ocs::pick_curated(&page, "Tela", "vinceliuice").map(|pack| pack.id),
        Some(1279924)
    );
    assert_eq!(ocs::pick_curated(&page, "Tela", "someone-else"), None);
}

#[test]
fn ocs_items_without_a_free_https_archive_are_left_out() {
    let item = |name: &str, link: &str, price: &str| {
        serde_json::json!({
            "id": "7", "name": "<b>Pack</b> &amp; more", "personid": "me",
            "xdg_type": "icons", "downloads": 12, "score": "250",
            "downloadlink1": link, "downloadname1": name, "downloadsize1": 3,
            "downloadprice1": price, "tags": "linux,gplv3",
        })
    };
    let body = serde_json::json!({
        "status": "ok", "totalitems": "4", "data": [
            item("theme.tar.xz", "https://example.org/a", "0"),
            item("trailer.mp4", "https://example.org/b", "0"),
            item("theme.tar.xz", "http://example.org/c", "0"),
            item("theme.tar.xz", "https://example.org/d", "4.99"),
        ]
    });
    let page = ocs::parse(body.to_string().as_bytes(), 2).unwrap();
    assert_eq!(page.total, 4);
    assert_eq!(page.page, 2);
    assert_eq!(page.packs.len(), 1);
    let pack = &page.packs[0];
    assert_eq!(pack.name, "Pack & more");
    assert_eq!(pack.score, 100);
    assert_eq!(pack.license, Some("GPL-3.0"));
    assert_eq!(pack.files[0].size, 3 * 1024);
    let refused = serde_json::json!({"status": "failed", "message": "bad <i>query</i>"});
    assert!(
        ocs::parse(refused.to_string().as_bytes(), 0)
            .unwrap_err()
            .contains("bad query")
    );
    assert!(ocs::parse(b"<html>", 0).is_err());
    let query = ocs::query(ocs::Sort::TopRated, "  tela ", 3, 20);
    assert!(query.contains(&("sortmode", "high".into())));
    assert!(query.contains(&("search", "tela".into())));
    assert!(query.contains(&("page", "3".into())));
    assert!(
        !ocs::query(ocs::Sort::Newest, " ", 0, 20)
            .iter()
            .any(|(key, _)| *key == "search")
    );
}

#[test]
fn ocs_thumbnails_are_scaled_cached_and_trimmed() {
    let temp = Temp::new("thumbs");
    let mut picture = image::RgbImage::new(1200, 800);
    for pixel in picture.pixels_mut() {
        *pixel = image::Rgb([30, 90, 200]);
    }
    let mut jpeg = Vec::new();
    picture
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    let png = ocs::make_thumbnail(&jpeg).unwrap();
    let thumb = image::load_from_memory(&png).unwrap();
    assert_eq!((thumb.width(), thumb.height()), (ocs::THUMB_WIDTH, 373));
    let path = ocs::thumbnail_path(&temp.0, "https://example.org/a.png");
    assert_eq!(
        path,
        ocs::thumbnail_path(&temp.0, "https://example.org/a.png")
    );
    assert_ne!(
        path,
        ocs::thumbnail_path(&temp.0, "https://example.org/b.png")
    );
    for index in 0..5 {
        fs::write(temp.0.join(format!("{index}.png")), vec![0; 1000]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    ocs::trim_cache(&temp.0, 2500);
    let left: Vec<_> = fs::read_dir(&temp.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(temp.0.join("4.png").exists() && temp.0.join("3.png").exists());
}

// Archives

enum Item<'a> {
    File(&'a str, &'a [u8]),
    Dir(&'a str),
    Symlink(&'a str, &'a str),
    Hardlink(&'a str, &'a str),
    Device(&'a str),
}

/// A tar written header by header, so hostile names go in unchecked.
fn tar(items: &[Item]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for item in items {
        let mut header = tar::Header::new_gnu();
        let (name, kind, data, link): (&str, tar::EntryType, &[u8], Option<&str>) = match item {
            Item::File(name, data) => (name, tar::EntryType::Regular, data, None),
            Item::Dir(name) => (name, tar::EntryType::Directory, b"", None),
            Item::Symlink(name, to) => (name, tar::EntryType::Symlink, b"", Some(to)),
            Item::Hardlink(name, to) => (name, tar::EntryType::Link, b"", Some(to)),
            Item::Device(name) => (name, tar::EntryType::Char, b"", None),
        };
        let gnu = header.as_gnu_mut().unwrap();
        gnu.name[..name.len()].copy_from_slice(name.as_bytes());
        if let Some(link) = link {
            gnu.linkname[..link.len()].copy_from_slice(link.as_bytes());
        }
        header.set_entry_type(kind);
        header.set_size(data.len() as u64);
        header.set_mode(if matches!(item, Item::Dir(_)) {
            0o755
        } else {
            0o644
        });
        header.set_cksum();
        builder.append(&header, data).unwrap();
    }
    builder.into_inner().unwrap()
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn xz(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    lzma_rs::xz_compress(&mut std::io::Cursor::new(bytes), &mut out).unwrap();
    out
}

/// A stored zip; entries ending in `/` are folders, `(name, None, Some(to))`
/// a unix link.
fn zip(items: &[(&str, &[u8], Option<&str>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, data, link) in items {
        let data: &[u8] = link.map_or(*data, str::as_bytes);
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let offset = out.len() as u32;
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&crc.sum().to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        let mode: u32 = if link.is_some() {
            0o120_777
        } else if name.ends_with('/') {
            0o040_755
        } else {
            0o100_644
        };
        directory.extend_from_slice(b"PK\x01\x02");
        directory.extend_from_slice(&[20, 3, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        directory.extend_from_slice(&crc.sum().to_le_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
        directory.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        directory.extend_from_slice(&(mode << 16).to_le_bytes());
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

const INDEX: &[u8] =
    b"[Icon Theme]\nName=Test\nInherits=hicolor\nDirectories=scalable/apps\n\n[scalable/apps]\nSize=64\nMinSize=8\nMaxSize=512\nType=Scalable\n";

fn extract(
    temp: &Temp,
    name: &str,
    bytes: &[u8],
    limits: archive::Limits,
) -> Result<PathBuf, String> {
    let file = temp.0.join(format!("{name}.download"));
    fs::write(&file, bytes).unwrap();
    let dest = temp.0.join(format!("{name}-out"));
    archive::extract(&file, &dest, limits).map(|_| dest)
}

#[test]
fn archive_formats_are_sniffed_from_their_bytes() {
    let plain = tar(&[Item::File("a", b"x")]);
    assert_eq!(archive::sniff(&plain), archive::Format::Tar);
    assert_eq!(archive::sniff(&gzip(&plain)), archive::Format::TarGz);
    assert_eq!(archive::sniff(&xz(&plain)), archive::Format::TarXz);
    assert_eq!(
        archive::sniff(&zip(&[("a", b"x", None)])),
        archive::Format::Zip
    );
    assert_eq!(archive::sniff(b"BZh91AY&SY"), archive::Format::TarBz2);
    assert_eq!(archive::sniff(b"\0\0\0 ftypmp4"), archive::Format::Unknown);
    let temp = Temp::new("sniff");
    for (name, bytes) in [
        (
            "gz",
            gzip(&tar(&[Item::Dir("T"), Item::File("T/index.theme", INDEX)])),
        ),
        ("xz", xz(&tar(&[Item::File("T/index.theme", INDEX)]))),
        (
            "zip",
            zip(&[("T/", b"", None), ("T/index.theme", INDEX, None)]),
        ),
    ] {
        let dest = extract(&temp, name, &bytes, Default::default()).unwrap();
        assert_eq!(
            fs::read(dest.join("T/index.theme")).unwrap(),
            INDEX,
            "{name}"
        );
    }
    let error = extract(&temp, "bz", b"BZh91AY&SYxxxx", Default::default()).unwrap_err();
    assert!(error.contains("bzip2"), "{error}");
    // The xz temporary file is gone, and nothing else was left beside.
    let left: Vec<_> = fs::read_dir(&temp.0)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".halcyon"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn archive_paths_cannot_leave_the_destination() {
    let temp = Temp::new("escape");
    let outside = temp.0.join("outside");
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "dotdot",
            tar(&[Item::File("T/../../outside", b"x")]),
            "leaves its folder",
        ),
        (
            "absolute",
            tar(&[Item::File(outside.to_str().unwrap(), b"x")]),
            "absolute",
        ),
        (
            "link",
            tar(&[Item::Symlink("T/link", "../../outside")]),
            "leaves its folder",
        ),
        (
            "abslink",
            tar(&[Item::Symlink("T/link", "/etc")]),
            "absolute",
        ),
        (
            "rootlink",
            tar(&[Item::Symlink("T/up", "..")]),
            "own folder",
        ),
        (
            "through",
            tar(&[
                Item::Dir("T"),
                Item::Symlink("L", "T"),
                Item::File("L/x", b"x"),
            ]),
            "through a link",
        ),
        (
            "hard",
            tar(&[Item::Hardlink("T/h", "../outside")]),
            "leaves its folder",
        ),
        ("device", tar(&[Item::Device("T/tty")]), "device"),
        (
            "zipdotdot",
            zip(&[("../outside", b"x", None)]),
            "leaves its folder",
        ),
        (
            "zipslash",
            zip(&[("..\\outside", b"x", None)]),
            "leaves its folder",
        ),
        (
            "ziplink",
            zip(&[("T/l", b"", Some("../../outside"))]),
            "leaves its folder",
        ),
    ];
    for (name, bytes, reason) in cases {
        let error = extract(&temp, name, &gzip(&bytes), Default::default())
            .or_else(|_| extract(&temp, &format!("{name}-raw"), &bytes, Default::default()))
            .unwrap_err();
        assert!(error.contains(reason), "{name}: {error}");
        assert!(!outside.exists(), "{name} wrote outside");
    }
    // A link that is fine on its own but resolves outside through another
    // link is caught after the walk.
    fs::create_dir_all(temp.0.join("chain-out")).unwrap();
    std::os::unix::fs::symlink(&temp.0, temp.0.join("chain-out/escape")).unwrap();
    let error = archive::check_links(&temp.0.join("chain-out")).unwrap_err();
    assert!(error.contains("leaves its folder"), "{error}");
    // Links inside the archive survive, dangling ones are dropped.
    let dest = extract(
        &temp,
        "inside",
        &tar(&[
            Item::File("T/a.svg", b"<svg/>"),
            Item::Symlink("T/b.svg", "a.svg"),
            Item::Symlink("T/gone.svg", "missing.svg"),
            Item::Hardlink("T/c.svg", "T/a.svg"),
        ]),
        Default::default(),
    )
    .unwrap();
    assert_eq!(fs::read(dest.join("T/b.svg")).unwrap(), b"<svg/>");
    assert_eq!(fs::read(dest.join("T/c.svg")).unwrap(), b"<svg/>");
    assert!(fs::symlink_metadata(dest.join("T/gone.svg")).is_err());
}

#[test]
fn archive_size_and_count_are_capped() {
    let temp = Temp::new("caps");
    let big = vec![7u8; 4096];
    let limits = archive::Limits {
        bytes: 3000,
        entries: 100,
    };
    for (name, bytes) in [
        ("tar", gzip(&tar(&[Item::File("T/big", &big)]))),
        ("xz", xz(&tar(&[Item::File("T/big", &big)]))),
        ("zip", zip(&[("T/big", &big, None)])),
    ] {
        let error = extract(&temp, name, &bytes, limits).unwrap_err();
        assert!(error.contains("more than"), "{name}: {error}");
    }
    let many: Vec<String> = (0..6).map(|index| format!("T/{index}")).collect();
    let items: Vec<Item> = many.iter().map(|name| Item::File(name, b"x")).collect();
    let limits = archive::Limits {
        bytes: 1 << 20,
        entries: 5,
    };
    let error = extract(&temp, "many", &gzip(&tar(&items)), limits).unwrap_err();
    assert!(error.contains("more than 5 files"), "{error}");
}

// Installing

fn pack(themes: &[&str]) -> Vec<u8> {
    let mut items = Vec::new();
    let names: Vec<(String, String)> = themes
        .iter()
        .map(|theme| {
            (
                format!("pack-1.0/{theme}/index.theme"),
                format!("pack-1.0/{theme}/scalable/apps/app.svg"),
            )
        })
        .collect();
    items.push(Item::File("pack-1.0/README.md", b"readme"));
    for (index, svg) in &names {
        items.push(Item::File(index, INDEX));
        items.push(Item::File(svg, b"<svg/>"));
    }
    gzip(&tar(&items))
}

#[test]
fn install_moves_every_theme_and_keeps_a_users_own_theme() {
    let temp = Temp::new("install");
    let paths = temp.paths();
    // The user's own Mine-Dark, which Halcyon must not touch.
    let mine = paths.icons.join("Mine-Dark");
    fs::create_dir_all(&mine).unwrap();
    fs::write(
        mine.join("index.theme"),
        b"[Icon Theme]\nName=Mine\nDirectories=x\n",
    )
    .unwrap();
    let file = temp.0.join("pack.tar.gz");
    fs::write(&file, pack(&["Mine", "Mine-Dark", "Mine-Light"])).unwrap();
    let outcome = install::install_archive(&paths, &file, 42, "Mine icons").unwrap();
    assert_eq!(outcome.installed, ["Mine", "Mine-Light"]);
    assert_eq!(outcome.skipped, ["Mine-Dark"]);
    assert!(
        outcome.message().contains("Kept your own Mine-Dark"),
        "{}",
        outcome.message()
    );
    assert_eq!(
        fs::read(mine.join("index.theme")).unwrap(),
        b"[Icon Theme]\nName=Mine\nDirectories=x\n"
    );
    assert!(paths.icons.join("Mine/scalable/apps/app.svg").is_file());
    assert!(paths.icons.join("Mine-Light/index.theme").is_file());
    // No staging folder is left, and the catalog sees the new themes.
    let hidden: Vec<_> = fs::read_dir(&paths.icons)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(hidden.is_empty());
    let catalog =
        crate::desktop::appearance::catalog::IconCatalog::discover(&[paths.icons.clone()], &[]);
    assert!(catalog.get("Mine").is_some() && catalog.get("Mine-Light").is_some());
    let manifest = install::Manifest::load(&paths);
    assert_eq!(manifest.of_pack(42), ["Mine", "Mine-Light"]);
    // Installing again replaces Halcyon's own copies.
    fs::write(paths.icons.join("Mine/stale"), b"old").unwrap();
    let again = install::install_archive(&paths, &file, 42, "Mine icons").unwrap();
    assert_eq!(again.installed, ["Mine", "Mine-Light"]);
    assert!(!paths.icons.join("Mine/stale").exists());
    // A pack with only the user's theme installs nothing.
    let only = temp.0.join("only.tar.gz");
    fs::write(&only, pack(&["Mine-Dark"])).unwrap();
    let none = install::install_archive(&paths, &only, 43, "Other").unwrap();
    assert!(none.installed.is_empty());
    assert!(none.message().starts_with("Nothing was installed"));
    // An archive without a theme is an error.
    let empty = temp.0.join("empty.tar.gz");
    fs::write(&empty, gzip(&tar(&[Item::File("README", b"x")]))).unwrap();
    assert!(install::install_archive(&paths, &empty, 44, "Empty").is_err());
}

#[test]
fn remove_only_removes_what_halcyon_installed() {
    let temp = Temp::new("remove");
    let paths = temp.paths();
    let file = temp.0.join("pack.tar.gz");
    fs::write(&file, pack(&["Ours"])).unwrap();
    install::install_archive(&paths, &file, 1, "Ours").unwrap();
    let theirs = paths.icons.join("Theirs");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("index.theme"), INDEX).unwrap();
    for id in ["Theirs", "../Ours", ".", ""] {
        assert!(install::remove(&paths, id).is_err(), "{id}");
    }
    assert!(theirs.join("index.theme").is_file());
    install::remove(&paths, "Ours").unwrap();
    assert!(!paths.icons.join("Ours").exists());
    assert!(install::Manifest::load(&paths).themes.is_empty());
    // A hand edited manifest naming someone else's theme with a path is
    // ignored on load.
    fs::write(
        paths.manifest(),
        br#"{"themes":[{"id":"../Theirs","pack_id":1,"pack":"x","installed":0}]}"#,
    )
    .unwrap();
    assert!(install::Manifest::load(&paths).themes.is_empty());
}

// Lookup and the overlay

const BLUE_FOLDER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect x="4" y="12" width="56" height="44" rx="4" fill="#3584e4"/><rect x="4" y="8" width="24" height="8" rx="2" fill="#1c71d8"/><rect x="12" y="28" width="40" height="4" fill="#808080"/></svg>"##;
const GLYPH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><path d="M32 6 L58 56 L6 56 Z" fill="#e01b24"/></svg>"##;
const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#26a269"/></svg>"##;

/// A base theme "Base" (inheriting "Parent") in the system dir, and a
/// "Cosmic" with a folder only it has.
fn themes(paths: &Paths) {
    let system = &paths.search[1];
    let theme = |id: &str, inherits: &str, icons: &[(&str, &str, &str)]| {
        let root = system.join(id);
        fs::create_dir_all(root.join("scalable/apps")).unwrap();
        fs::create_dir_all(root.join("scalable/places")).unwrap();
        fs::create_dir_all(root.join("48x48/apps")).unwrap();
        fs::write(
            root.join("index.theme"),
            format!(
                "[Icon Theme]\nName={id}\nInherits={inherits}\nDirectories=48x48/apps,scalable/apps,scalable/places\n\n[48x48/apps]\nSize=48\nType=Fixed\n\n[scalable/apps]\nSize=64\nMinSize=16\nMaxSize=512\nType=Scalable\n\n[scalable/places]\nSize=64\nMinSize=16\nMaxSize=512\nType=Scalable\n"
            ),
        )
        .unwrap();
        for (dir, name, svg) in icons {
            fs::write(root.join(dir).join(format!("{name}.svg")), svg).unwrap();
        }
    };
    theme(
        "Base",
        "Parent",
        &[
            ("scalable/apps", "glyph", GLYPH),
            ("scalable/places", "folder", BLUE_FOLDER),
        ],
    );
    theme("Parent", "hicolor", &[("scalable/apps", "square", SQUARE)]);
    theme(
        "Cosmic",
        "hicolor",
        &[("scalable/places", "user-home", BLUE_FOLDER)],
    );
    theme("hicolor", "", &[("scalable/apps", "fallback", SQUARE)]);
}

#[test]
fn lookup_follows_inherits_and_prefers_the_closest_size() {
    let temp = Temp::new("lookup");
    let paths = temp.paths();
    themes(&paths);
    let png = paths.search[1].join("Base/48x48/apps/glyph.png");
    fs::write(&png, b"png").unwrap();
    let mut resolver = lookup::Resolver::new(paths.search.clone());
    assert_eq!(resolver.chain("Base"), ["Base", "Parent", "hicolor"]);
    let chain = overlay::chain(&mut resolver, "Base");
    assert_eq!(chain, ["Base", "Parent", "Cosmic", "hicolor"]);
    // At 48 both match exactly and the SVG wins the tie; a PNG-only icon
    // is found at any size.
    assert!(
        resolver
            .find(&chain, "glyph", 48)
            .unwrap()
            .1
            .ends_with("scalable/apps/glyph.svg")
    );
    assert!(png.is_file());
    fs::write(paths.search[1].join("Base/48x48/apps/raster.png"), b"png").unwrap();
    assert!(
        resolver
            .find(&chain, "raster", 256)
            .unwrap()
            .1
            .ends_with("48x48/apps/raster.png")
    );
    assert_eq!(resolver.find(&chain, "square", 64).unwrap().0, "Parent");
    assert_eq!(resolver.find(&chain, "user-home", 64).unwrap().0, "Cosmic");
    assert_eq!(resolver.find(&chain, "fallback", 64).unwrap().0, "hicolor");
    assert!(resolver.find(&chain, "../Base/index", 64).is_none());
    assert!(resolver.find(&chain, "missing", 64).is_none());
}

#[test]
fn overlay_index_lists_its_directories_and_inherits_the_base_first() {
    let text = overlay::index_theme(
        "Papirus",
        overlay::Shape::Squircle,
        Some([0x91, 0x41, 0xac]),
    );
    let line = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .unwrap()
            .to_string()
    };
    assert_eq!(line("Inherits"), "Papirus,Cosmic,hicolor");
    assert_eq!(
        line("Directories"),
        "64x64/apps,64x64/places,128x128/apps,128x128/places,256x256/apps,256x256/places"
    );
    assert_eq!(line("Hidden"), "true");
    assert_eq!(line("X-Halcyon-Base"), "Papirus");
    assert_eq!(line("X-Halcyon-Tint"), "#9141ac");
    for dir in line("Directories").split(',') {
        let size = dir.split('x').next().unwrap();
        assert!(
            text.contains(&format!("[{dir}]\nSize={size}\nScale=1\n")),
            "{dir} section in {text}"
        );
        assert!(text.contains("Type=Fixed"));
    }
    // Cosmic as the base is not repeated; shape only means apps only.
    let text = overlay::index_theme("Cosmic", overlay::Shape::Circle, None);
    assert!(text.contains("Inherits=Cosmic,hicolor\n"));
    assert!(text.contains("Directories=64x64/apps,128x128/apps,256x256/apps\n"));
    assert!(!text.contains("places"));
    // Tint only means places only.
    let text = overlay::index_theme("Pop", overlay::Shape::Original, Some([1, 2, 3]));
    assert!(text.contains("Directories=64x64/places,128x128/places,256x256/places\n"));
}

#[test]
fn masks_clear_the_corners_and_keep_the_center_with_soft_edges() {
    for shape in [
        overlay::Shape::Circle,
        overlay::Shape::Squircle,
        overlay::Shape::Rounded,
    ] {
        let size = 128;
        let mask = overlay::coverage(shape, size);
        let at = |x: u32, y: u32| mask[(y * size + x) as usize];
        for (x, y) in [
            (0, 0),
            (size - 1, 0),
            (0, size - 1),
            (size - 1, size - 1),
            (14, 14),
        ] {
            assert_eq!(at(x, y), 0., "{shape:?} corner {x},{y}");
        }
        assert_eq!(at(size / 2, size / 2), 1., "{shape:?}");
        assert_eq!(at(size / 2, 12), 1., "{shape:?} top middle");
        let partial = mask
            .iter()
            .filter(|&&cover| cover > 0. && cover < 1.)
            .count();
        assert!(partial > 40, "{shape:?} anti-aliased edge: {partial}");
        // A shaped icon has transparent corners and an opaque center,
        // whether it filled the square or was set on a tile.
        for svg in [SQUARE, GLYPH] {
            let icon = raster::svg(svg.as_bytes(), size).unwrap();
            for dark in [false, true] {
                let shaped = overlay::shape_icon(&icon, shape, dark, &mask);
                assert_eq!(shaped.get_pixel(0, 0)[3], 0);
                assert_eq!(shaped.get_pixel(10, 10)[3], 0, "{shape:?}");
                assert_eq!(shaped.get_pixel(size / 2, size / 2)[3], 255);
                assert_eq!(shaped.get_pixel(size / 2, 10)[3], 255, "{shape:?} {svg}");
            }
        }
    }
    // Squircle is between the circle and the rounded square.
    let sum = |shape| overlay::coverage(shape, 64).iter().sum::<f32>();
    let (circle, squircle, rounded) = (
        sum(overlay::Shape::Circle),
        sum(overlay::Shape::Squircle),
        sum(overlay::Shape::Rounded),
    );
    assert!(
        circle < squircle && squircle < rounded + 200.,
        "{circle} {squircle} {rounded}"
    );
    // A glyph keeps its color on a pale tile in light mode and a deep one in
    // dark mode.
    let icon = raster::svg(GLYPH.as_bytes(), 128).unwrap();
    let mask = overlay::coverage(overlay::Shape::Squircle, 128);
    let light = overlay::shape_icon(&icon, overlay::Shape::Squircle, false, &mask);
    let dark = overlay::shape_icon(&icon, overlay::Shape::Squircle, true, &mask);
    let tile = |picture: &image::RgbaImage| {
        let pixel = picture.get_pixel(64, 14);
        (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) / 3
    };
    assert!(
        tile(&light) > 200 && tile(&dark) < 90,
        "{} {}",
        tile(&light),
        tile(&dark)
    );
    let center = light.get_pixel(64, 80);
    assert!(center[0] > 200 && center[1] < 60, "{center:?}");
}

#[test]
fn tint_turns_a_blue_folder_to_the_accent_and_leaves_gray_alone() {
    let mut icon = raster::svg(BLUE_FOLDER.as_bytes(), 64).unwrap();
    let before = icon.clone();
    let accent = [0xe0, 0x1b, 0x24];
    overlay::tint(&mut icon, accent);
    let (accent_h, ..) = overlay::hue_saturation(accent);
    let hue = |pixel: &image::Rgba<u8>| overlay::hue_saturation([pixel[0], pixel[1], pixel[2]]);
    let body = icon.get_pixel(32, 44);
    let (h, s, l) = hue(body);
    let (_, _, old_l) = hue(before.get_pixel(32, 44));
    let distance = (h - accent_h)
        .rem_euclid(360.)
        .min((accent_h - h).rem_euclid(360.));
    assert!(distance < 8., "hue {h} vs accent {accent_h}");
    assert!(s > 0.5, "{s}");
    assert!((l - old_l).abs() < 0.03, "lightness kept: {l} {old_l}");
    // The gray stripe and the transparent corner stay as they were.
    assert_eq!(icon.get_pixel(32, 29), before.get_pixel(32, 29));
    assert_eq!(icon.get_pixel(32, 29)[0], icon.get_pixel(32, 29)[1]);
    assert_eq!(icon.get_pixel(0, 0), before.get_pixel(0, 0));
    // The darker tab keeps its shading relative to the body.
    let tab = hue(icon.get_pixel(16, 11));
    assert!(tab.2 < l, "tab {tab:?} body {l}");
}

#[test]
fn overlay_generates_shaped_apps_and_tinted_folders_in_alternating_slots() {
    let temp = Temp::new("overlay");
    let paths = temp.paths();
    themes(&paths);
    let request = overlay::Request {
        base: "Base".into(),
        shape: overlay::Shape::Circle,
        tint: Some([0x26, 0xa2, 0x69]),
        dark: false,
        apps: vec![
            "glyph".into(),
            "square".into(),
            "missing".into(),
            "glyph".into(),
        ],
    };
    let seen = std::sync::Mutex::new(Vec::new());
    let slot = overlay::generate(&paths, &request, Some("Base"), &|done, total| {
        seen.lock().unwrap().push((done, total));
    })
    .unwrap();
    assert_eq!(slot, "Halcyon-Shaped");
    let root = paths.icons.join(&slot);
    for size in overlay::SIZES {
        for icon in [
            "apps/glyph",
            "apps/square",
            "places/folder",
            "places/user-home",
        ] {
            let file = root.join(format!("{size}x{size}/{icon}.png"));
            let picture = image::open(&file).unwrap().to_rgba8();
            assert_eq!(picture.dimensions(), (size, size), "{file:?}");
        }
    }
    assert!(!root.join("64x64/apps/missing.png").exists());
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first(), Some(&(0, 4)));
    assert_eq!(seen.iter().map(|(done, _)| *done).max(), Some(4));
    // Desktop lookups find the overlay's icons first and fall through to
    // the base for anything else.
    let mut resolver = lookup::Resolver::new(paths.search.clone());
    let chain = resolver.chain(&slot);
    assert_eq!(
        chain,
        [slot.as_str(), "Base", "Parent", "Cosmic", "hicolor"]
    );
    assert!(
        resolver
            .find(&chain, "glyph", 64)
            .unwrap()
            .1
            .starts_with(&root)
    );
    assert!(
        resolver
            .find(&chain, "folder", 32)
            .unwrap()
            .1
            .starts_with(&root)
    );
    assert_eq!(
        overlay::recorded_base(&paths, &slot).as_deref(),
        Some("Base")
    );
    // The next build goes to the other slot; then the first is removed.
    let next = overlay::generate(&paths, &request, Some(&slot), &|_, _| {}).unwrap();
    assert_eq!(next, "Halcyon-Shaped-Alt");
    overlay::remove_all(&paths, Some(&next));
    assert!(!root.exists() && paths.icons.join(&next).exists());
    // A folder of a slot's name that Halcyon didn't write is never touched.
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("index.theme"), INDEX).unwrap();
    assert!(overlay::generate(&paths, &request, Some(&next), &|_, _| {}).is_err());
    overlay::remove_all(&paths, None);
    assert!(root.join("index.theme").exists());
    assert!(!paths.icons.join(&next).exists());
    // An overlay can't be a base.
    let bad = overlay::Request {
        base: "Halcyon-Shaped".into(),
        ..request
    };
    assert!(overlay::generate(&paths, &bad, None, &|_, _| {}).is_err());
}

#[test]
fn style_is_saved_in_halcyons_config_and_follows_the_desktop() {
    let temp = Temp::new("style");
    let paths = temp.paths();
    assert_eq!(overlay::Style::load(&paths), overlay::Style::default());
    let style = overlay::Style {
        base: Some("Papirus".into()),
        shape: overlay::Shape::Rounded,
        tint: true,
    };
    style.save(&paths).unwrap();
    assert!(paths.style().starts_with(&paths.config));
    assert_eq!(overlay::Style::load(&paths), style);
    assert!(style.active());
    // With the overlay in use, the saved style holds; with any other theme
    // (changed elsewhere), that theme is the base and nothing is shaped.
    assert_eq!(style.clone().effective("Halcyon-Shaped-Alt"), style);
    let elsewhere = style.effective("Pop");
    assert_eq!(elsewhere.base.as_deref(), Some("Pop"));
    assert!(!elsewhere.active());
    // A stored overlay or path as the base is dropped.
    fs::write(
        paths.style(),
        br#"{"base":"Halcyon-Shaped","shape":"circle","tint":false}"#,
    )
    .unwrap();
    let loaded = overlay::Style::load(&paths);
    assert_eq!(loaded.base, None);
    assert_eq!(loaded.shape, overlay::Shape::Circle);
}

/// A manual check against a real pack and the real app icons, never run by
/// default: `HALCYON_TRY_ROOT=<scratch dir> [HALCYON_TRY_PACK=<archive>]
/// cargo test --bin halcyon try_a_real_pack -- --ignored`. Everything is
/// written under the scratch dir; the system icons are only read.
#[test]
#[ignore]
fn try_a_real_pack() {
    let root = PathBuf::from(std::env::var("HALCYON_TRY_ROOT").unwrap());
    let paths = Paths {
        icons: root.join("icons"),
        search: vec![root.join("icons"), PathBuf::from("/usr/share/icons")],
        data: root.join("data"),
        cache: root.join("cache"),
        config: root.join("config"),
    };
    let mut base = std::env::var("HALCYON_TRY_BASE").unwrap_or_else(|_| "Cosmic".into());
    if let Ok(pack) = std::env::var("HALCYON_TRY_PACK") {
        let started = std::time::Instant::now();
        let outcome = install::install_archive(&paths, Path::new(&pack), 1, "Try").unwrap();
        eprintln!(
            "installed in {:?}: {}",
            started.elapsed(),
            outcome.message()
        );
        base = outcome.installed[0].clone();
    }
    let catalog = crate::desktop::apps::AppCatalog::installed();
    let apps: Vec<String> = catalog
        .sorted()
        .iter()
        .map(|app| app.icon.clone())
        .filter(|icon| !icon.is_empty() && !icon.starts_with('/'))
        .collect();
    let mut resolver = lookup::Resolver::new(paths.search.clone());
    let chain = overlay::chain(&mut resolver, &base);
    let shown: Vec<PathBuf> = apps
        .iter()
        .filter_map(|name| resolver.find(&chain, name, 256).map(|(_, path)| path))
        .take(24)
        .collect();
    let folders: Vec<PathBuf> = overlay::FOLDERS
        .iter()
        .filter_map(|name| resolver.find(&chain, name, 256).map(|(_, path)| path))
        .take(8)
        .collect();
    for dark in [false, true] {
        let cell = 72;
        let mut sheet = image::RgbaImage::from_pixel(
            cell * shown.len().max(folders.len()) as u32,
            cell * 6,
            if dark {
                image::Rgba([30, 30, 30, 255])
            } else {
                image::Rgba([245, 245, 245, 255])
            },
        );
        for (row, shape) in overlay::Shape::ALL.into_iter().enumerate() {
            for (column, source) in shown.iter().enumerate() {
                if let Some(icon) = overlay::preview(source, shape, dark, 64) {
                    image::imageops::overlay(
                        &mut sheet,
                        &icon,
                        (column as u32 * cell + 4).into(),
                        (row as u32 * cell + 4).into(),
                    );
                }
            }
        }
        for (column, source) in folders.iter().enumerate() {
            for (row, accent) in [(4, None), (5, Some([0xe0, 0x1b, 0x24]))] {
                if let Some(icon) = overlay::preview_folder(source, accent, 64) {
                    image::imageops::overlay(
                        &mut sheet,
                        &icon,
                        (column as u32 * cell + 4).into(),
                        (row * cell + 4).into(),
                    );
                }
            }
        }
        let shade = if dark { "dark" } else { "light" };
        sheet
            .save(root.join(format!("sheet-{base}-{shade}.png")))
            .unwrap();
    }
    let started = std::time::Instant::now();
    let request = overlay::Request {
        base: base.clone(),
        shape: overlay::Shape::Squircle,
        tint: Some([0x26, 0xa2, 0x69]),
        dark: false,
        apps,
    };
    let slot = overlay::generate(&paths, &request, None, &|_, _| {}).unwrap();
    eprintln!(
        "{} icons in {slot} in {:?}",
        request.apps.len(),
        started.elapsed()
    );
}
