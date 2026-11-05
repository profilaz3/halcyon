//! Immutable, bounded compiled-in gallery pixels. Views only clone handles.
use cosmic::widget::image::Handle;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const LIMIT: usize = 16 * 1024 * 1024;
static CACHE: OnceLock<Catalog> = OnceLock::new();
struct Catalog {
    looks: BTreeMap<(&'static str, bool), Handle>,
    bundled: BTreeMap<&'static str, Handle>,
    bytes: usize,
}
impl Catalog {
    fn handle(&mut self, pixels: crate::desktop::wallpaper::media::Pixels) -> Option<Handle> {
        let next = self.bytes.checked_add(pixels.rgba.len())?;
        if next > LIMIT {
            return None;
        }
        self.bytes = next;
        Some(Handle::from_rgba(pixels.width, pixels.height, pixels.rgba))
    }
}
pub(in crate::settings) fn ready() -> bool {
    CACHE.get().is_some()
}
pub(in crate::settings) async fn load() {
    if !ready() {
        crate::desktop::wallpaper::media::worker::prepare_static(prepare).await;
    }
}
pub(in crate::settings) fn prepare() {
    CACHE.get_or_init(|| {
        let mut cache = Catalog { looks: BTreeMap::new(), bundled: BTreeMap::new(), bytes: 0 };
        macro_rules! pictures {
            ($($id:literal),*) => {
                [$(($id, false, &include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/looks/", $id, ".jpg"))[..]),
                   ($id, true, &include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/looks/", $id, "-wallpaper.jpg"))[..])),*]
            };
        }
        for (id, wallpaper, bytes) in pictures!("halcyon", "calm", "evening", "focus", "paper", "rose") {
            let Ok(image) = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg) else { continue; };
            let bounds = if wallpaper { (640,360) } else { (320,180) };
            let pixels = image.thumbnail(bounds.0,bounds.1).to_rgba8();
            let pixels = crate::desktop::wallpaper::media::Pixels { width:pixels.width(),height:pixels.height(),rgba:pixels.into_raw() };
            if let Some(handle) = cache.handle(pixels) { cache.looks.insert((id,wallpaper),handle); }
        }
        for (name, _) in crate::desktop::wallpaper::BUNDLED {
            if let Some(pixels) = crate::desktop::wallpaper::media::bundled_thumbnail(name) {
                if let Some(handle) = cache.handle(pixels) { cache.bundled.insert(name,handle); }
            }
        }
        cache
    });
}
pub(super) fn look(id: &str, wallpaper: bool) -> Option<Handle> {
    let cache = CACHE.get()?;
    let id = match id {
        "tidal" => "calm",
        "paper-studio" => "paper",
        other => other,
    };
    if wallpaper {
        if let Some(name) = match id {
            "carbon" => Some("graphite"),
            "rain" => Some("halcyon-rain"),
            "snow" => Some("halcyon-snow"),
            "fog" => Some("halcyon-fog"),
            "spine" => Some("halcyon-workbench"),
            "ridge" => Some("halcyon-ridge"),
            _ => None,
        } {
            return cache.bundled.get(name).cloned();
        }
    }
    cache.looks.get(&(id, wallpaper)).cloned()
}
pub(super) fn bundled(name: &str) -> Option<Handle> {
    CACHE.get()?.bundled.get(name).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fb10_gallery_pixels_are_bounded_shared_and_cover_new_presets() {
        prepare();
        let cache = CACHE.get().unwrap();
        assert!(cache.bytes <= LIMIT);
        for id in [
            "tidal",
            "paper-studio",
            "carbon",
            "rain",
            "snow",
            "fog",
            "halcyon",
            "calm",
            "evening",
            "focus",
            "paper",
            "rose",
        ] {
            let a = look(id, true).unwrap();
            let b = look(id, true).unwrap();
            assert_eq!(a.id(), b.id());
        }
        assert_eq!(
            look("tidal", true).unwrap().id(),
            look("calm", true).unwrap().id()
        );
        assert_eq!(
            look("paper-studio", true).unwrap().id(),
            look("paper", true).unwrap().id()
        );
        assert_eq!(
            cache.bundled.len(),
            crate::desktop::wallpaper::BUNDLED.len()
        );
        assert!(look("unknown", true).is_none());
    }
}
