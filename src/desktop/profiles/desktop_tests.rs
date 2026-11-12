//! Profiles version 3: the Wallpaper and Desktop icons parts. Transport
//! (shared vs local), capture classification, planning through the page
//! drafts, member preservation, asset checks at plan and again at Apply,
//! and arrangement replacement. Every test writes only below its own
//! temporary fixture root.
use super::*;
use crate::desktop::desktop_icons::{self, DesktopIcons};
use crate::desktop::store::tests::Fixture;
use crate::desktop::store::{DesktopError, RawValue};
use crate::desktop::wallpaper::{
    self, BgColor, BgEntry, BgSource, FilterMethod, SamplingMethod, ScalingMode,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const BG: &str = wallpaper::NAMESPACE;
const FILES: &str = desktop_icons::NAMESPACE;

fn desk(name: &str, outputs: &[&str]) -> Fixture {
    let fixture = Fixture::new(&format!("profile-v3-{name}"));
    fs::write(fixture.0.join("outputs"), outputs.join("\n")).unwrap();
    fixture
}
/// A file (any bytes: checks look at existence and type, not content).
fn picture(fixture: &Fixture, relative: &str) -> PathBuf {
    let path = fixture.0.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"picture").unwrap();
    path
}
fn bundled(fixture: &Fixture, name: &str, size: &str) -> PathBuf {
    picture(
        fixture,
        &format!("pictures/Desktop Cards/{name}-{size}.png"),
    )
}
/// An entry whose unused members differ from every default.
fn entry(output: &str, source: &str) -> String {
    format!(
        "(output: \"{output}\", source: {source}, filter_by_theme: true, rotation_frequency: 1234, filter_method: Linear, scaling_mode: Zoom, sampling_method: Random)"
    )
}
fn path(path: &Path) -> String {
    format!("Path(\"{}\")", path.display())
}
fn color(rgb: [f32; 3]) -> String {
    format!("Color(Single(({}, {}, {})))", rgb[0], rgb[1], rgb[2])
}
fn read_entry(fixture: &Fixture, key: &str) -> BgEntry {
    RawValue::parse(fixture.read(BG, key).unwrap())
        .unwrap()
        .decode()
        .unwrap()
}
fn read_list(fixture: &Fixture) -> BTreeSet<String> {
    RawValue::parse(fixture.read(BG, "backgrounds").unwrap())
        .unwrap()
        .decode::<Vec<String>>()
        .unwrap()
        .into_iter()
        .collect()
}
/// The COSMIC user layer, byte for byte.
fn settings(fixture: &Fixture) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let config = fixture.0.join("config");
    fixture
        .tree()
        .into_iter()
        .filter(|(path, _)| path.starts_with(&config))
        .collect()
}
fn parts(profile: &Profile) -> BTreeSet<Domain> {
    profile.domains().into_iter().collect()
}
fn part(plan: &ProfilePlan, domain: Domain) -> &DomainPlan {
    plan.domains
        .iter()
        .find(|part| part.domain == domain)
        .unwrap()
}
fn with_wallpaper(name: &str, look: WallpaperLook) -> Profile {
    let mut profile = Profile::new(name).unwrap();
    profile.wallpaper = Some(look);
    profile.version = profile.written_version();
    profile
}
fn shared(background: BackgroundLook) -> WallpaperLook {
    WallpaperLook {
        background: Some(background),
        ..Default::default()
    }
}
fn source(source: SourceLook) -> BackgroundLook {
    BackgroundLook {
        source: Some(source),
        ..Default::default()
    }
}
fn record(value: serde_json::Value) -> Vec<u8> {
    let mut object = json!({"format": "desktop-cards-profile", "version": 3, "name": "X"});
    for (key, member) in value.as_object().unwrap() {
        object[key] = member.clone();
    }
    serde_json::to_vec(&object).unwrap()
}

/// A desktop with a personal shared picture, separate displays (DP-1 has
/// the bundled Teal) and desktop icons between the page's slider steps.
fn personal_desk(name: &str) -> (Fixture, PathBuf) {
    let fixture = desk(name, &["DP-1", "eDP-1"]);
    let beach = picture(&fixture, "home/me/beach.png");
    let teal = bundled(&fixture, "teal", "4k");
    fixture.user(BG, "all", &entry("all", &path(&beach)));
    fixture.user(BG, "same-on-all", "false");
    fixture.user(BG, "backgrounds", "[\"DP-1\"]");
    fixture.user(BG, "output.DP-1", &entry("DP-1", &path(&teal)));
    fixture.user(
        FILES,
        "desktop",
        "(grid_spacing: 80, icon_size: 130, show_content: false, show_mounted_drives: true, show_trash: true)",
    );
    (fixture, beach)
}

#[test]
fn export_omits_local_sources_and_displays_and_reports_them() {
    let (fixture, beach) = personal_desk("export");
    let captured = capture(&fixture.store(), "Mine").unwrap();
    let look = captured.profile.wallpaper.clone().unwrap();
    assert_eq!(look.displays, Some(DisplaysName::Separate));
    assert_eq!(
        look.background.as_ref().unwrap().source,
        Some(SourceLook::Image(beach))
    );
    assert_eq!(
        look.per_display.as_ref().unwrap()["DP-1"].source,
        Some(SourceLook::Bundled("teal".into()))
    );
    assert_eq!(
        captured.profile.desktop_icons,
        Some(DesktopIconsLook {
            icon_size: Some(130),
            spacing: Some(80),
            show_folder: Some(false),
            show_drives: Some(true),
            show_trash: Some(true),
        }),
        "values between slider steps are captured exactly"
    );

    // Shared: no picture, display names or arrangement, and no orphan
    // fit or slideshow from the omitted picture; the notice names both.
    let exported = export(&captured.profile).unwrap();
    assert_eq!(exported.omitted, [OWN_WALLPAPER, DISPLAY_BACKGROUNDS]);
    let text = String::from_utf8(exported.bytes.clone()).unwrap();
    for local in [
        "beach",
        "home",
        "DP-1",
        "per_display",
        "displays",
        "\"image\"",
        "teal",
        "wallpaper",
        "fill",
    ] {
        assert!(!text.contains(local), "{local} in {text}");
    }
    assert!(text.contains("\"version\": 3") && text.contains("\"icon_size\": 130"));
    let received = import(&exported.bytes).unwrap();
    assert_eq!(received.wallpaper, None, "no wallpaper part is left behind");
    assert_eq!(received.desktop_icons, captured.profile.desktop_icons);

    // Only local version 3 members: the shared file is version 2.
    let mut only_local = captured.profile.clone();
    only_local.desktop_icons = None;
    let text = String::from_utf8(export(&only_local).unwrap().bytes).unwrap();
    assert!(text.contains("\"version\": 2") && !text.contains("wallpaper"));

    // A personal folder takes its slideshow and fit with it.
    let folder = with_wallpaper(
        "Folder",
        shared(BackgroundLook {
            source: Some(SourceLook::Folder("/home/me/Trips".into())),
            fit: Some(FitName::Stretch),
            slideshow: Some(SlideshowLook {
                every: Some(600),
                order: Some(OrderName::Shuffle),
            }),
            ..Default::default()
        }),
    );
    let exported = export(&folder).unwrap();
    assert_eq!(exported.omitted, [OWN_WALLPAPER]);
    let text = String::from_utf8(exported.bytes).unwrap();
    assert!(!text.contains("slideshow") && !text.contains("stretch") && !text.contains("Trips"));

    // Built-in, COSMIC and colour backgrounds travel whole; so does an
    // intentional control without a source; an arrangement alone is named.
    for look in [
        shared(BackgroundLook {
            source: Some(SourceLook::Bundled("ocean".into())),
            fit: Some(FitName::Fit),
            fit_color: Some(Rgb::Hex("#102030".into())),
            slideshow: None,
        }),
        shared(BackgroundLook {
            source: Some(SourceLook::System("cosmic/nebula.jpg".into())),
            ..Default::default()
        }),
        shared(source(SourceLook::Gradient(GradientLook {
            stops: vec![Rgb::Hex("#336699".into()), Rgb::Exact([0.5, 0.1, 0.0])],
            angle: 90.,
        }))),
        shared(BackgroundLook {
            fit: Some(FitName::Fill),
            ..Default::default()
        }),
    ] {
        let profile = with_wallpaper("Shared", look);
        let exported = export(&profile).unwrap();
        assert!(exported.omitted.is_empty());
        assert_eq!(import(&exported.bytes).unwrap(), profile);
    }
    let arrangement = with_wallpaper(
        "Arrangement",
        WallpaperLook {
            displays: Some(DisplaysName::Same),
            background: Some(source(SourceLook::Color(Rgb::Hex("#336699".into())))),
            per_display: None,
        },
    );
    let exported = export(&arrangement).unwrap();
    assert_eq!(exported.omitted, [DISPLAY_ARRANGEMENT]);
    assert_eq!(
        import(&exported.bytes)
            .unwrap()
            .wallpaper
            .unwrap()
            .background,
        arrangement.wallpaper.unwrap().background
    );
}

#[test]
fn import_refuses_local_wallpaper_members() {
    for (case, wallpaper) in [
        (
            "image",
            json!({"background": {"source": {"image": "/home/me/beach.png"}}}),
        ),
        (
            "folder",
            json!({"background": {"source": {"folder": "/home/me/Trips"}}}),
        ),
        (
            "per_display",
            json!({"displays": "separate", "per_display": {"DP-1": {"source": {"bundled": "teal"}}}}),
        ),
        ("displays", json!({"displays": "same"})),
    ] {
        let bytes = record(json!({ "wallpaper": wallpaper }));
        assert_eq!(
            import(&bytes),
            Err(ProfileError::Invalid(LOCAL_ONLY.into())),
            "{case}"
        );
        assert!(import_local(&bytes).is_ok(), "{case}: the library keeps it");
    }
    // Members that are wrong anywhere.
    for (case, value) in [
        ("v2 wallpaper", json!({"version": 2, "wallpaper": {}})),
        ("v2 icons", json!({"version": 2, "desktop_icons": {}})),
        ("v1 icons", json!({"version": 1, "desktop_icons": {}})),
        (
            "map without mode",
            json!({"wallpaper": {"per_display": {"DP-1": {}}}}),
        ),
        (
            "same with own",
            json!({"wallpaper": {"displays": "same", "per_display": {"DP-1": {}}}}),
        ),
        (
            "bad display",
            json!({"wallpaper": {"displays": "separate", "per_display": {"../x": {}}}}),
        ),
        (
            "letterbox without fit",
            json!({"wallpaper": {"background": {"fit_color": "#000000"}}}),
        ),
        (
            "slideshow of a color",
            json!({"wallpaper": {"background": {"source": {"color": "#336699"}, "slideshow": {"every": 600}}}}),
        ),
        (
            "week",
            json!({"wallpaper": {"background": {"source": {"system": "cosmic"}, "slideshow": {"every": 700000}}}}),
        ),
        (
            "unknown bundled",
            json!({"wallpaper": {"background": {"source": {"bundled": "neon"}}}}),
        ),
        (
            "escaping system",
            json!({"wallpaper": {"background": {"source": {"system": "cosmic/../../etc/passwd"}}}}),
        ),
        (
            "absolute system",
            json!({"wallpaper": {"background": {"source": {"system": "/etc/x"}}}}),
        ),
        (
            "relative picture",
            json!({"wallpaper": {"background": {"source": {"image": "beach.png"}}}}),
        ),
        (
            "bad hex",
            json!({"wallpaper": {"background": {"source": {"color": "#33669"}}}}),
        ),
        (
            "one stop",
            json!({"wallpaper": {"background": {"source": {"gradient": {"stops": ["#000000"], "angle": 90}}}}}),
        ),
        (
            "angle",
            json!({"wallpaper": {"background": {"source": {"gradient": {"stops": ["#000000", "#ffffff"], "angle": 400}}}}}),
        ),
        (
            "linear out of range",
            json!({"wallpaper": {"background": {"source": {"gradient": {"stops": [[0, 0, 2], "#ffffff"], "angle": 0}}}}}),
        ),
        (
            "native member",
            json!({"wallpaper": {"background": {"fit": "fill", "filter_method": "Linear"}}}),
        ),
        (
            "two sources",
            json!({"wallpaper": {"background": {"source": {"color": "#000000", "bundled": "teal"}}}}),
        ),
        ("icon size 0", json!({"desktop_icons": {"icon_size": 0}})),
        ("icon size 501", json!({"desktop_icons": {"spacing": 501}})),
        (
            "icon member",
            json!({"desktop_icons": {"grid_spacing": 100}}),
        ),
    ] {
        match import_local(&record(value)) {
            Err(ProfileError::Invalid(_)) => {}
            other => panic!("{case}: {other:?}"),
        }
    }
    // The native range, not the slider's steps.
    let edges = import(&record(
        json!({"desktop_icons": {"icon_size": 1, "spacing": 500}}),
    ));
    assert!(edges.is_ok(), "{edges:?}");
}

#[test]
fn library_round_trips_local_wallpaper_members() {
    let (fixture, _) = personal_desk("library");
    let store = fixture.store();
    let profile = capture(&store, "Mine").unwrap().profile;
    let library = ProfileLibrary::new(&store);
    let id = library.create(&profile).unwrap();
    assert_eq!(library.load(&id).unwrap(), profile);
    let file = fs::read_to_string(
        fixture
            .0
            .join("state/profiles")
            .join(format!("{}.json", id.as_str())),
    )
    .unwrap();
    assert!(file.contains("\"per_display\"") && file.contains("\"version\": 3"));
    // A copy keeps them too.
    let copy = library.duplicate(&id, "Copy").unwrap();
    assert_eq!(library.load(&copy).unwrap().wallpaper, profile.wallpaper);
}

#[test]
fn capture_classifies_bundled_system_and_personal_sources() {
    let fixture = desk("classify", &["DP-1", "DP-2", "HDMI-1"]);
    let nebula = picture(&fixture, "backgrounds/cosmic/nebula.jpg");
    let outside = picture(&fixture, "elsewhere/secret.jpg");
    let escape = fixture.0.join("backgrounds/cosmic/escape.jpg");
    std::os::unix::fs::symlink(&outside, &escape).unwrap();
    let rose = bundled(&fixture, "rose", "1080p");
    let trips = fixture.0.join("home/me/Trips");
    picture(&fixture, "home/me/Trips/a.png");
    fixture.user(
        BG,
        "all",
        &entry("all", &path(&nebula))
            .replace("scaling_mode: Zoom", "scaling_mode: Fit((0.1, 0.2, 0.3))"),
    );
    fixture.user(BG, "same-on-all", "false");
    fixture.user(BG, "backgrounds", "[\"DP-1\", \"DP-2\", \"HDMI-1\"]");
    fixture.user(BG, "output.DP-1", &entry("DP-1", &path(&escape)));
    fixture.user(BG, "output.DP-2", &entry("DP-2", &path(&rose)));
    fixture.user(BG, "output.HDMI-1", &entry("HDMI-1", &path(&trips)));
    let store = fixture.store();
    let before = fixture.tree();
    let profile = capture(&store, "Mixed").unwrap().profile;
    assert_eq!(fixture.tree(), before, "capture writes nothing");
    let look = profile.wallpaper.clone().unwrap();
    let background = look.background.unwrap();
    assert_eq!(
        background.source,
        Some(SourceLook::System("cosmic/nebula.jpg".into()))
    );
    assert_eq!(background.fit, Some(FitName::Fit));
    assert_eq!(
        background.fit_color,
        Some(Rgb::Exact([0.1, 0.2, 0.3])),
        "not 8-bit exact: exact components"
    );
    assert_eq!(background.slideshow, None, "a picture has no slideshow");
    let map = look.per_display.unwrap();
    assert_eq!(
        map["DP-1"].source,
        Some(SourceLook::Image(escape.clone())),
        "a symlink escaping the system folder is personal"
    );
    assert_eq!(map["DP-2"].source, Some(SourceLook::Bundled("rose".into())));
    assert_eq!(map["HDMI-1"].source, Some(SourceLook::Folder(trips)));
    assert_eq!(
        map["HDMI-1"].slideshow,
        Some(SlideshowLook {
            every: Some(1234),
            order: Some(OrderName::Shuffle),
        })
    );

    // Applied where it came from (1080p, not-connected, exact colours):
    // nothing changes and nothing blocks.
    let installed = Installed::discover(&store);
    let same = plan(&store, &installed, &profile, &parts(&profile));
    assert!(
        same.blocking_reasons().is_empty(),
        "{:?}",
        same.blocking_reasons()
    );
    assert!(same.is_empty(), "{:?}", same.domains);

    // The escaping link is never exported as a system ID, and a profile
    // naming it as one doesn't resolve.
    let exported = export(&profile).unwrap();
    assert!(
        !String::from_utf8(exported.bytes)
            .unwrap()
            .contains("escape")
    );
    let named = with_wallpaper(
        "Named",
        shared(source(SourceLook::System("cosmic/escape.jpg".into()))),
    );
    let blocked = plan(&store, &installed, &named, &parts(&named));
    assert!(
        part(&blocked, Domain::Wallpaper).blocked[0].contains("isn’t on this computer"),
        "{:?}",
        blocked.domains
    );

    // Colours: hex where 8-bit exact, components otherwise; gradient
    // stops round-trip in linear light.
    // (The same 8-bit values the Wallpaper page writes for #336699.)
    let srgb = Rgb::Hex("#336699".into()).srgb().unwrap();
    assert_eq!(Rgb::of_srgb(srgb), Rgb::Hex("#336699".into()));
    assert_eq!(Rgb::of_srgb([0.1, 0.2, 0.3]), Rgb::Exact([0.1, 0.2, 0.3]));
    let linear = srgb.map(wallpaper::srgb_to_linear);
    assert_eq!(Rgb::of_linear(linear), Rgb::Hex("#336699".into()));
    assert_eq!(Rgb::of_linear(linear).linear(), Ok(linear));
    assert_eq!(
        Rgb::of_linear([0.580, 0.922, 0.922]),
        Rgb::Exact([0.580, 0.922, 0.922])
    );
}

#[test]
fn profile_apply_keeps_target_unused_bg_members() {
    let fixture = desk("preserve", &["eDP-1"]);
    let old = picture(&fixture, "home/me/old.png");
    fixture.user(BG, "all", &entry("all", &path(&old)));
    let store = fixture.store();
    let before = settings(&fixture);
    let profile = with_wallpaper(
        "Blue",
        shared(source(SourceLook::Color(Rgb::Hex("#336699".into())))),
    );
    let plan = plan(
        &store,
        &Installed::discover(&store),
        &profile,
        &parts(&profile),
    );
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    assert_eq!(
        part(&plan, Domain::Wallpaper).changes,
        ["Shared background: color #336699"]
    );
    apply(&store, &plan).unwrap();
    let written = read_entry(&fixture, "all");
    assert_eq!(
        written,
        BgEntry {
            output: "all".into(),
            source: BgSource::Color(BgColor::Single(Rgb::Hex("#336699".into()).srgb().unwrap())),
            filter_by_theme: true,
            rotation_frequency: 1234,
            transition_duration: None,
            filter_method: FilterMethod::Linear,
            scaling_mode: ScalingMode::Zoom,
            sampling_method: SamplingMethod::Random,
        },
        "only the source changes"
    );
    assert_eq!(
        fixture.read(BG, "same-on-all"),
        None,
        "the mode is left alone"
    );
    // Undo restores the target exactly.
    let latest = store.latest_change().unwrap().unwrap();
    store.undo_latest(&latest.id).unwrap();
    assert_eq!(settings(&fixture), before);
}

#[test]
fn missing_wallpaper_asset_blocks_only_its_part() {
    let fixture = desk("missing", &["eDP-1"]);
    let store = fixture.store();
    let mut profile = with_wallpaper(
        "Amber",
        shared(BackgroundLook {
            source: Some(SourceLook::Bundled("amber".into())),
            fit: Some(FitName::Fill),
            ..Default::default()
        }),
    );
    profile.desktop_icons = Some(DesktopIconsLook {
        icon_size: Some(150),
        ..Default::default()
    });
    let installed = Installed::discover(&store);
    let before = fixture.tree();
    let blocked = plan(&store, &installed, &profile, &parts(&profile));
    let wallpaper_part = part(&blocked, Domain::Wallpaper);
    assert_eq!(
        wallpaper_part.blocked,
        ["The built-in Amber wallpaper isn’t installed. Reinstall Halcyon to restore it."]
    );
    let icons = part(&blocked, Domain::DesktopIcons);
    assert!(icons.blocked.is_empty());
    assert_eq!(icons.changes, ["Icon size"]);
    assert!(!blocked.can_apply());
    assert_eq!(blocked.background(), None, "a blocked part isn't previewed");
    assert!(apply(&store, &blocked).is_err());
    assert_eq!(fixture.tree(), before, "nothing is written");

    // Leaving Wallpaper out applies the rest unchanged.
    let rest = plan(&store, &installed, &profile, &[Domain::DesktopIcons].into());
    assert!(rest.can_apply(), "{:?}", rest.blocking_reasons());
    assert_eq!(part(&rest, Domain::DesktopIcons), icons);
    apply(&store, &rest).unwrap();
    assert_eq!(fixture.read(BG, "all"), None, "the wallpaper is untouched");
    let config: DesktopIcons = RawValue::parse(fixture.read(FILES, "desktop").unwrap())
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(config.icon_size.get(), 150);

    // Once installed, the part applies and previews its picture.
    let amber = picture(&fixture, "pictures/Desktop Cards/amber.jpg");
    let ready = plan(&store, &installed, &profile, &parts(&profile));
    assert!(ready.can_apply(), "{:?}", ready.blocking_reasons());
    assert_eq!(
        ready.background().map(|entry| &entry.source),
        Some(&BgSource::Path(amber))
    );
    assert_eq!(
        part(&ready, Domain::Wallpaper).changes,
        ["Shared background: Amber"]
    );
}

/// Separate displays: HDMI-1 (not connected) and eDP-1 listed with their
/// own colours; DP-1 connected without one.
fn separate_desk(name: &str) -> Fixture {
    let fixture = desk(name, &["DP-1", "eDP-1"]);
    fixture.user(BG, "all", &entry("all", &color([0.8, 0.1, 0.1])));
    fixture.user(BG, "same-on-all", "false");
    fixture.user(BG, "backgrounds", "[\"HDMI-1\", \"eDP-1\"]");
    fixture.user(
        BG,
        "output.HDMI-1",
        &entry("HDMI-1", &color([0.1, 0.1, 0.8])),
    );
    fixture.user(BG, "output.eDP-1", &entry("eDP-1", &color([0.1, 0.8, 0.1])));
    fixture
}

fn applied(fixture: &Fixture, look: WallpaperLook) -> ProfilePlan {
    let store = fixture.store();
    let profile = with_wallpaper("Arrangement", look);
    let plan = plan(
        &store,
        &Installed::discover(&store),
        &profile,
        &parts(&profile),
    );
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    apply(&store, &plan).unwrap();
    plan
}

#[test]
fn separate_arrangement_replaces_list_and_warns_when_absent() {
    let blue = || source(SourceLook::Color(Rgb::Hex("#336699".into())));
    // An explicit map replaces the list: exactly its displays, also one
    // whose look equals where it starts; others keep their files.
    let fixture = separate_desk("replace");
    let hdmi = fixture.read(BG, "output.HDMI-1");
    let plan = applied(
        &fixture,
        WallpaperLook {
            displays: Some(DisplaysName::Separate),
            background: None,
            per_display: Some(
                [
                    ("DP-1".to_string(), blue()),
                    ("eDP-1".to_string(), BackgroundLook::default()),
                ]
                .into(),
            ),
        },
    );
    assert!(part(&plan, Domain::Wallpaper).warnings.is_empty());
    assert_eq!(read_list(&fixture), ["DP-1".into(), "eDP-1".into()].into());
    let dp1 = read_entry(&fixture, "output.DP-1");
    assert_eq!(dp1.output, "DP-1", "named after its display");
    assert_eq!(
        dp1.filter_method,
        FilterMethod::Linear,
        "started from the shared entry"
    );
    assert_eq!(fixture.read(BG, "output.HDMI-1"), hdmi, "left untouched");
    assert_eq!(
        read_entry(&fixture, "output.eDP-1").source,
        BgSource::Color(BgColor::Single([0.1, 0.8, 0.1]))
    );

    // A display whose look matches the shared one is still given its own.
    let fixture = separate_desk("assign");
    applied(
        &fixture,
        WallpaperLook {
            displays: Some(DisplaysName::Separate),
            background: None,
            per_display: Some([("DP-1".to_string(), BackgroundLook::default())].into()),
        },
    );
    assert_eq!(read_list(&fixture), ["DP-1".into()].into());
    assert_eq!(
        read_entry(&fixture, "output.DP-1").source,
        BgSource::Color(BgColor::Single([0.8, 0.1, 0.1]))
    );

    // A mode without a map is an empty map: nobody keeps their own.
    let fixture = separate_desk("empty-map");
    applied(
        &fixture,
        WallpaperLook {
            displays: Some(DisplaysName::Separate),
            ..Default::default()
        },
    );
    assert!(read_list(&fixture).is_empty());
    assert_eq!(fixture.read(BG, "same-on-all").as_deref(), Some("false"));

    // Same: the mode and an empty list.
    let fixture = separate_desk("same");
    applied(
        &fixture,
        WallpaperLook {
            displays: Some(DisplaysName::Same),
            ..Default::default()
        },
    );
    assert!(read_list(&fixture).is_empty());
    assert_eq!(fixture.read(BG, "same-on-all").as_deref(), Some("true"));

    // Absent arrangement: left alone, with a note; a not-connected display
    // in a map is named.
    let fixture = separate_desk("absent");
    let plan = applied(&fixture, shared(blue()));
    assert_eq!(
        part(&plan, Domain::Wallpaper).warnings,
        ["Displays with their own background keep it."]
    );
    assert_eq!(
        read_list(&fixture),
        ["HDMI-1".into(), "eDP-1".into()].into()
    );
    let fixture = separate_desk("away");
    let plan = applied(
        &fixture,
        WallpaperLook {
            displays: Some(DisplaysName::Separate),
            background: None,
            per_display: Some([("HDMI-1".to_string(), blue())].into()),
        },
    );
    assert_eq!(
        part(&plan, Domain::Wallpaper).warnings,
        ["HDMI-1 isn’t connected; its background is kept for when it is."]
    );
}

#[test]
fn profile_rechecks_wallpaper_assets_at_apply() {
    let fixture = desk("recheck", &["eDP-1"]);
    let nebula = picture(&fixture, "backgrounds/cosmic/nebula.jpg");
    let outside = picture(&fixture, "elsewhere/other.jpg");
    let trips = fixture.0.join("home/me/Trips");
    let first = picture(&fixture, "home/me/Trips/a.png");
    let store = fixture.store();
    let installed = Installed::discover(&store);
    let mut profile = with_wallpaper(
        "Nebula",
        shared(source(SourceLook::System("cosmic/nebula.jpg".into()))),
    );
    profile.desktop_icons = Some(DesktopIconsLook {
        show_trash: Some(true),
        ..Default::default()
    });
    let refused = |plan: &ProfilePlan, expected: &str| {
        let before = fixture.tree();
        match apply(&store, plan) {
            Err(DesktopError::InvalidValue { message, .. }) => {
                assert!(message.starts_with("Wallpaper: "), "{message}");
                assert!(message.contains(expected), "{message}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(fixture.tree(), before, "no selected part was written");
    };

    // Removed after the plan was made.
    let prepared = plan(&store, &installed, &profile, &parts(&profile));
    assert!(prepared.can_apply(), "{:?}", prepared.blocking_reasons());
    assert_eq!(prepared.assets().len(), 1);
    fs::remove_file(&nebula).unwrap();
    assert_eq!(prepared.recheck().len(), 1);
    refused(&prepared, "isn’t on this computer");

    // Swapped for a link out of the trusted folder: the same path no longer
    // resolves inside it, and no other path is chosen.
    std::os::unix::fs::symlink(&outside, &nebula).unwrap();
    refused(
        &prepared,
        "no longer one of this computer’s COSMIC wallpapers",
    );
    fs::remove_file(&nebula).unwrap();
    picture(&fixture, "backgrounds/cosmic/nebula.jpg");

    // A personal folder emptied after the plan.
    profile.wallpaper = Some(shared(source(SourceLook::Folder(trips))));
    let prepared = plan(&store, &installed, &profile, &parts(&profile));
    assert!(prepared.can_apply(), "{:?}", prepared.blocking_reasons());
    fs::remove_file(&first).unwrap();
    refused(&prepared, "has no pictures");
    // Planned again it shows why; without Wallpaper the rest applies.
    let replanned = plan(&store, &installed, &profile, &parts(&profile));
    assert_eq!(
        part(&replanned, Domain::Wallpaper).blocked,
        ["“Trips” has no pictures."]
    );
    let rest = plan(&store, &installed, &profile, &[Domain::DesktopIcons].into());
    apply(&store, &rest).unwrap();
    assert!(
        fixture
            .read(FILES, "desktop")
            .unwrap()
            .contains("show_trash: true")
    );
    assert_eq!(fixture.read(BG, "all"), None);
}

#[test]
fn desktop_icons_apply_exact_values_and_unknown_members_block_only_icons() {
    let fixture = desk("icons", &["eDP-1"]);
    let store = fixture.store();
    let installed = Installed::discover(&store);
    let mut profile = Profile::new("Icons").unwrap();
    profile.desktop_icons = Some(DesktopIconsLook {
        icon_size: Some(130),
        spacing: Some(37),
        show_drives: Some(true),
        ..Default::default()
    });
    profile.wallpaper = Some(shared(source(SourceLook::Color(Rgb::Hex(
        "#336699".into(),
    )))));
    let plan_all = plan(&store, &installed, &profile, &parts(&profile));
    let icons = part(&plan_all, Domain::DesktopIcons);
    assert_eq!(
        icons.changes,
        ["Icon size", "Icon spacing", "Mounted drives"]
    );
    assert_eq!(icons.notes, [desktop_icons::REFRESH_NOTE]);
    apply(&store, &plan_all).unwrap();
    let config: DesktopIcons = RawValue::parse(fixture.read(FILES, "desktop").unwrap())
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(
        (config.icon_size.get(), config.grid_spacing.get()),
        (130, 37),
        "between the page's 25 % steps, exactly"
    );
    assert!(
        config.show_mounted_drives && config.show_content,
        "others kept"
    );

    // A struct this version doesn't recognize is kept; only Desktop icons
    // is blocked.
    let other = desk("icons-unknown", &["eDP-1"]);
    other.user(
        FILES,
        "desktop",
        "(grid_spacing: 100, icon_size: 100, show_content: true, show_mounted_drives: false, show_trash: false, future: 1)",
    );
    let store = other.store();
    let blocked = plan(
        &store,
        &Installed::discover(&store),
        &profile,
        &parts(&profile),
    );
    assert!(!part(&blocked, Domain::DesktopIcons).blocked.is_empty());
    assert!(part(&blocked, Domain::Wallpaper).blocked.is_empty());
}

#[test]
fn auditfix_set03_offline_unlisted_wallpaper_applies_and_undo_restores_retained_file() {
    for same in [false, true] {
        let fixture = desk(&format!("auditfix-offline-{same}"), &["eDP-1"]);
        let retained = entry("DP-2", &color([0.7, 0.2, 0.1]));
        fixture.user(BG, "all", &entry("all", &color([0.1, 0.1, 0.1])));
        fixture.user(BG, "same-on-all", if same { "true" } else { "false" });
        fixture.user(BG, "backgrounds", "[]");
        fixture.user(BG, "output.DP-2", &retained);
        let before = settings(&fixture);
        let profile = with_wallpaper(
            "Offline display",
            WallpaperLook {
                displays: Some(DisplaysName::Separate),
                per_display: Some(
                    [(
                        "DP-2".into(),
                        source(SourceLook::Color(Rgb::Hex("#336699".into()))),
                    )]
                    .into(),
                ),
                ..Default::default()
            },
        );
        let store = fixture.store();
        let installed = Installed::discover(&store);
        let prepared = plan(&store, &installed, &profile, &parts(&profile));
        assert!(prepared.can_apply(), "{:?}", prepared.blocking_reasons());
        let latest = apply(&store, &prepared).unwrap();
        assert_eq!(read_list(&fixture), ["DP-2".into()].into());
        let BgSource::Color(BgColor::Single(actual)) = read_entry(&fixture, "output.DP-2").source
        else {
            panic!("expected the requested solid color");
        };
        for (actual, expected) in actual.into_iter().zip([0.2, 0.4, 0.6]) {
            assert!((actual - expected).abs() < 0.000001);
        }
        assert!(plan(&store, &installed, &profile, &parts(&profile)).is_empty());
        store.undo_latest(&latest.id).unwrap();
        assert_eq!(settings(&fixture), before);

        let prepared = plan(&store, &installed, &profile, &parts(&profile));
        fixture.user(BG, "output.DP-2", &entry("DP-2", &color([0.2, 0.8, 0.1])));
        let changed = settings(&fixture);
        assert!(matches!(
            apply(&store, &prepared),
            Err(DesktopError::Conflict(_))
        ));
        assert_eq!(
            settings(&fixture),
            changed,
            "outside edits must still conflict"
        );
    }
}

#[test]
fn auditfix_set03_unlisted_unreadable_or_unsupported_wallpaper_blocks_atomically() {
    for unreadable in [false, true] {
        let fixture = desk(
            &format!("auditfix-offline-blocked-{unreadable}"),
            &["eDP-1"],
        );
        fixture.user(BG, "same-on-all", "true");
        fixture.user(BG, "backgrounds", "[]");
        let store = fixture.store();
        if unreadable {
            fs::create_dir_all(store.user_path(&wallpaper::output_key("DP-2").unwrap())).unwrap();
        } else {
            fixture.user(BG, "output.DP-2", "(output: \"DP-2\", source: FutureVariant, filter_by_theme: false, rotation_frequency: 300)");
        }
        let mut profile = with_wallpaper(
            "Offline display",
            WallpaperLook {
                displays: Some(DisplaysName::Separate),
                per_display: Some(
                    [(
                        "DP-2".into(),
                        source(SourceLook::Color(Rgb::Hex("#336699".into()))),
                    )]
                    .into(),
                ),
                ..Default::default()
            },
        );
        profile.desktop_icons = Some(DesktopIconsLook {
            show_trash: Some(true),
            ..Default::default()
        });
        profile.version = profile.written_version();
        let before = settings(&fixture);
        let prepared = plan(
            &store,
            &Installed::discover(&store),
            &profile,
            &parts(&profile),
        );
        assert!(!part(&prepared, Domain::Wallpaper).blocked.is_empty());
        assert!(apply(&store, &prepared).is_err());
        assert_eq!(settings(&fixture), before);
    }
}
