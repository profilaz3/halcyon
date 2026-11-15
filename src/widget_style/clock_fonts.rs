//! Typefaces Halcyon ships for the clock. Each is a variable font under the
//! SIL Open Font License 1.1 (`assets/fonts/clock/*/OFL.txt`), compiled into
//! the binary and added to the renderer's font database next to the
//! installed fonts, so the desktop, Settings and the depth worker all draw
//! the same letters without installing anything.

/// One shipped typeface: the family name the font itself declares, what
/// Settings calls it, and the weights its `wght` axis covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundled {
    pub family: &'static str,
    /// A short description for the picker.
    pub kind: &'static str,
    pub weights: std::ops::RangeInclusive<u16>,
    data: &'static [u8],
}

macro_rules! font {
    ($path:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/clock/",
            $path
        ))
    };
}

/// The shipped clock typefaces, in the order Settings offers them.
pub const ALL: [Bundled; 4] = [
    Bundled {
        family: "Outfit",
        kind: "Geometric",
        weights: 100..=900,
        data: font!("outfit/Outfit-wght.ttf"),
    },
    Bundled {
        family: "Antonio",
        kind: "Tall",
        weights: 100..=700,
        data: font!("antonio/Antonio-wght.ttf"),
    },
    Bundled {
        family: "Red Hat Mono",
        kind: "Monospaced",
        weights: 300..=700,
        data: font!("red-hat-mono/RedHatMono-wght.ttf"),
    },
    Bundled {
        family: "Bodoni Moda",
        kind: "Serif",
        weights: 400..=900,
        data: font!("bodoni-moda/BodoniModa-opsz-wght.ttf"),
    },
];

/// The shipped typeface named `family`, if it is one.
pub fn bundled(family: &str) -> Option<&'static Bundled> {
    ALL.iter().find(|font| font.family == family)
}

/// Whether a shipped `family` draws `font` exactly: upright, normal width
/// and a weight on its axis. `None` when `family` is not shipped.
pub fn draws(family: &str, font: cosmic::iced::Font) -> Option<bool> {
    let bundled = bundled(family)?;
    Some(
        font.style == cosmic::iced::font::Style::Normal
            && font.stretch == cosmic::iced::font::Stretch::Normal
            && bundled.weights.contains(&weight(font.weight)),
    )
}

fn weight(weight: cosmic::iced::font::Weight) -> u16 {
    use cosmic::iced::font::Weight;
    match weight {
        Weight::Thin => 100,
        Weight::ExtraLight => 200,
        Weight::Light => 300,
        Weight::Normal => 400,
        Weight::Medium => 500,
        Weight::Semibold => 600,
        Weight::Bold => 700,
        Weight::ExtraBold => 800,
        Weight::Black => 900,
    }
}

/// Adds the shipped typefaces to the renderer's font database. The files
/// are part of the binary, so nothing is read, and a second call adds
/// nothing (the database keeps one copy of each borrowed font).
pub fn load(system: &mut cosmic::iced::advanced::graphics::text::FontSystem) {
    for font in &ALL {
        system.load_font(std::borrow::Cow::Borrowed(font.data));
    }
}

/// Adds the shipped typefaces to the shared renderer once, waiting for the
/// font system if another thread holds it. Test renders call this first, so
/// the font database never changes between two renders being compared.
pub fn ensure_loaded() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        if let Ok(mut system) = cosmic::iced::advanced::graphics::text::font_system().write() {
            load(&mut system);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each shipped file declares the family Settings saves, is variable
    /// over the weights listed, and its license travels with it.
    #[test]
    fn shipped_fonts_declare_their_family_weights_and_license() {
        use cosmic::iced::advanced::graphics::text::cosmic_text::{self, fontdb};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts/clock");
        let mut total = 0;
        for font in &ALL {
            let mut db = fontdb::Database::new();
            db.load_font_data(font.data.to_vec());
            let faces: Vec<_> = db.faces().cloned().collect();
            assert_eq!(faces.len(), 1, "{}", font.family);
            assert!(
                faces[0]
                    .families
                    .iter()
                    .any(|(name, _)| name == font.family),
                "{}: {:?}",
                font.family,
                faces[0].families
            );
            assert_eq!(faces[0].style, fontdb::Style::Normal);
            let mut system = cosmic_text::FontSystem::new_with_locale_and_db("en-US".into(), db);
            let axes = system
                .get_font(faces[0].id, fontdb::Weight::NORMAL)
                .and_then(|face| {
                    face.as_swash()
                        .variations()
                        .find(|axis| axis.tag() == u32::from_be_bytes(*b"wght"))
                        .map(|axis| (axis.min_value() as u16, axis.max_value() as u16))
                });
            assert_eq!(
                axes,
                Some((*font.weights.start(), *font.weights.end())),
                "{}",
                font.family
            );
            total += font.data.len();
            let folder = root.join(font.family.to_lowercase().replace(' ', "-"));
            let license = std::fs::read_to_string(folder.join("OFL.txt")).unwrap();
            assert!(license.contains("SIL Open Font License, Version 1.1"));
        }
        // A lean set: well under a megabyte for all of them together.
        assert!(total < 600_000, "{total} bytes");
        let notice = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("THIRD_PARTY.md"),
        )
        .unwrap();
        for font in &ALL {
            assert!(
                notice.contains(font.family),
                "THIRD_PARTY.md names {}",
                font.family
            );
        }
    }

    #[test]
    fn shipped_fonts_draw_their_weights_and_nothing_else() {
        use cosmic::iced::Font;
        use cosmic::iced::font::{Family, Style, Weight};
        let font = |weight| Font {
            family: Family::Name("Bodoni Moda"),
            weight,
            ..Font::DEFAULT
        };
        assert_eq!(draws("Bodoni Moda", font(Weight::Bold)), Some(true));
        assert_eq!(draws("Bodoni Moda", font(Weight::Light)), Some(false));
        assert_eq!(draws("Outfit", font(Weight::Light)), Some(true));
        assert_eq!(
            draws(
                "Outfit",
                Font {
                    style: Style::Italic,
                    ..font(Weight::Normal)
                }
            ),
            Some(false)
        );
        assert_eq!(draws("DejaVu Sans", font(Weight::Normal)), None);
    }
}
