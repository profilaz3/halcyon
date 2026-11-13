//! The desktop clock behind the rain glass. While a wallpaper effect runs on
//! the clock's output, the clock is drawn into that output's rain surface,
//! between the wallpaper and the glass, so drops slide over it and refract
//! it. Its own surface stays where it is, empty, so placement and input do
//! not change. A clock with a card, a recipe, glance, contrast assist or the
//! layout editor stays crisp on its own surface, and so does a clock whose
//! place on the output cannot be told.
use super::*;
use crate::desktop::panels::{Anchor, PanelDraft, PanelKind};
use crate::desktop::wallpaper::media::{Pixels, worker};
use crate::rain::gpu::Behind;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Default)]
pub(super) struct State {
    /// The request last sent, so an unchanged clock is not drawn again.
    key: Option<Vec<u8>>,
    id: u64,
    cancel: Option<watch::Sender<bool>>,
    /// Where the next drawn clock goes: its output and place on it.
    place: Option<Place>,
    /// The drawn clock, ready for the glass, and the output it is on.
    pub picture: Option<(crate::displays::WlOutput, Arc<Behind>)>,
    /// The clock cut by wallpaper depth that `picture` was made from.
    cut: Option<Arc<Pixels>>,
    /// The edges panels keep on an output, read now and then rather than on
    /// every update.
    reserved: Option<(String, Instant, Vec<(Anchor, u32)>)>,
}
/// How long the panels' kept edges are trusted before reading them again.
const PANELS: Duration = Duration::from_secs(20);
#[derive(Clone, PartialEq)]
struct Place {
    output: crate::displays::WlOutput,
    rect: [f32; 4],
}
impl State {
    fn cancel(&mut self) {
        if let Some(sender) = self.cancel.take() {
            sender.send_replace(true);
        }
        self.id = self.id.wrapping_add(1);
        self.key = None;
    }
    /// Stop drawing the clock behind the glass.
    pub fn clear(&mut self) {
        self.cancel();
        self.place = None;
        self.picture = None;
        self.cut = None;
    }
    /// A drawn clock arrived. True when it is the one asked for last.
    pub fn complete(&mut self, id: u64, pixels: Option<Pixels>) -> bool {
        if id != self.id {
            return false;
        }
        self.cancel = None;
        self.picture = match (pixels, self.place.clone()) {
            (Some(pixels), Some(place)) => Some((
                place.output,
                Arc::new(Behind {
                    pixels,
                    rect: place.rect,
                }),
            )),
            _ => None,
        };
        true
    }
}

/// Where the area that layer surfaces may use starts on a `full` output,
/// when panels keep `screen` of it: the panels that keep space say which
/// edges they keep it on. With panels on both opposite edges, the space is
/// shared by their thickness. None when the panels do not explain it.
pub(super) fn usable_origin(
    full: [f32; 2],
    screen: [f32; 2],
    reserved: &[(Anchor, u32)],
) -> Option<[f32; 2]> {
    let axis = |kept: f32, before: Anchor, after: Anchor| -> Option<f32> {
        if kept < 1. {
            return Some(0.);
        }
        let thickness = |edge| -> f32 {
            reserved
                .iter()
                .filter(|(anchor, _)| *anchor == edge)
                .map(|(_, t)| *t as f32)
                .sum()
        };
        match (thickness(before), thickness(after)) {
            (0., 0.) => None,
            (_, 0.) => Some(kept),
            (0., _) => Some(0.),
            (a, b) => Some((kept * a / (a + b)).round()),
        }
    };
    if screen.iter().zip(full).any(|(s, f)| *s > f + 1.) {
        return None;
    }
    Some([
        axis(full[0] - screen[0], Anchor::Left, Anchor::Right)?,
        axis(full[1] - screen[1], Anchor::Top, Anchor::Bottom)?,
    ])
}

impl Cards {
    /// Where the clock lies on its whole output: the output, the output's
    /// logical size, and where the area that layer surfaces may use starts
    /// on it. Panels that keep space move that area off the output's corner;
    /// they say which edges they keep. None when nothing explains the space
    /// kept, so the clock is not placed on a guess.
    pub(super) fn clock_spot(&mut self) -> Option<(crate::displays::Output, [f32; 2], [f32; 2])> {
        let output = self.widget_output(Widget::Clock)?.clone();
        let logical = output.info.logical_size?;
        let logical = [logical.0 as f32, logical.1 as f32];
        let screen = self.widget_screen(Widget::Clock);
        let inset = if (0..2).all(|i| (logical[i] - screen[i]).abs() < 1.) {
            [0., 0.]
        } else {
            let name = output.name()?.to_owned();
            let now = Instant::now();
            let fresh = self.glass.reserved.as_ref().is_some_and(|(n, at, _)| {
                *n == name && now.saturating_duration_since(*at) < PANELS
            });
            if !fresh {
                let store = crate::desktop::DesktopStore::from_env().ok()?;
                let reserved = [PanelKind::Panel, PanelKind::Dock]
                    .into_iter()
                    .filter_map(|kind| PanelDraft::load(&store, kind).ok()?.reserves(&name))
                    .collect();
                self.glass.reserved = Some((name, now, reserved));
            }
            let (_, _, reserved) = self.glass.reserved.as_ref()?;
            usable_origin(logical, screen, reserved)?
        };
        // The measured area keeps a fractional scale's precision, which the
        // output's integer logical size rounds away: use it on every axis
        // nothing keeps space on.
        let full = [0, 1].map(|i| {
            if (logical[i] - screen[i]).abs() < 1. {
                screen[i]
            } else {
                logical[i]
            }
        });
        Some((output, full, inset))
    }
    /// Where the clock lies behind the glass, if it can: its output, and its
    /// frame on the whole output for drawing and place on it as a fraction.
    fn glass_place(&mut self) -> Option<(Place, crate::depth::Frame)> {
        if !self.output_known
            || self.layout_editor.active
            || !self.config.widget_visible(Widget::Clock)
            || !self.widget_available(Widget::Clock)
            || !crate::depth::behind_glass(&self.config)
            || !self.rain.showing()
        {
            return None;
        }
        let output = self.widget_output(Widget::Clock)?;
        if !self.rain.drawn_on(&output.handle) {
            return None;
        }
        let (output, full, inset) = self.clock_spot()?;
        let scale = (output.info.scale_factor.max(1) as f32).min(2.);
        let placed = self.placed_position(Widget::Clock).map(f32::round);
        let frame = crate::depth::Frame {
            screen: full,
            origin: [inset[0] + placed[0], inset[1] + placed[1]],
            size: self.widget_size(Widget::Clock),
            scale,
        };
        let place = Place {
            output: output.handle,
            rect: [
                frame.origin[0] / full[0],
                frame.origin[1] / full[1],
                frame.size[0] / full[0],
                frame.size[1] / full[1],
            ],
        };
        Some((place, frame))
    }
    /// Draw the clock for behind the glass when it changes, and hand the
    /// drawn clock to the rain surface it stands behind.
    pub(super) fn glass_sync(&mut self) -> Task<Message> {
        let Some((place, frame)) = self.glass_place() else {
            if self.glass.place.is_some() || self.glass.picture.is_some() {
                self.glass.clear();
                self.rain.set_behind(None);
            }
            return Task::none();
        };
        let Some(request) =
            crate::depth::Request::plain(&self.config, frame, self.clock_at.naive_local())
        else {
            self.glass.clear();
            self.rain.set_behind(None);
            return Task::none();
        };
        // While the clock cut by the scene is on its way, it stays on its
        // own surface rather than showing uncut behind the glass first.
        if crate::depth::eligible(&self.config) && self.depth.waiting() {
            if self.glass.place.is_some() || self.glass.picture.is_some() {
                self.glass.clear();
                self.rain.set_behind(None);
            }
            return Task::none();
        }
        // A clock that wallpaper depth cut stands behind the glass as it is:
        // the scene's foreground stays in front of it, and the drops bend
        // both together.
        if let Some(cut) = self.depth.pixels.clone() {
            let same = self.glass.place.as_ref() == Some(&place)
                && self.glass.picture.is_some()
                && self
                    .glass
                    .cut
                    .as_ref()
                    .is_some_and(|c| Arc::ptr_eq(c, &cut));
            if !same {
                self.glass.cancel();
                self.glass.picture = Some((
                    place.output.clone(),
                    Arc::new(Behind {
                        pixels: (*cut).clone(),
                        rect: place.rect,
                    }),
                ));
                self.glass.cut = Some(cut);
                self.glass.place = Some(place);
            }
            self.rain.set_behind(self.glass.picture.clone());
            return Task::none();
        }
        if self.glass.cut.take().is_some() {
            self.glass.picture = None;
        }
        self.glass.place = Some(place);
        self.rain.set_behind(self.glass.picture.clone());
        let Ok(bytes) = serde_json::to_vec(&request) else {
            return Task::none();
        };
        if self.glass.key.as_ref() == Some(&bytes) {
            return Task::none();
        }
        self.glass.cancel();
        self.glass.key = Some(bytes.clone());
        let id = self.glass.id;
        let (send, cancel) = watch::channel(false);
        self.glass.cancel = Some(send);
        cosmic::task::future(async move {
            let pixels = worker::render_scene(bytes, cancel)
                .await
                .and_then(|r| r.pixels);
            Message::GlassReady(id, pixels)
        })
    }
    /// A drawn clock arrived from the worker.
    pub(super) fn glass_ready(&mut self, id: u64, pixels: Option<Pixels>) -> Task<Message> {
        if self.glass.complete(id, pixels) {
            self.rain.set_behind(self.glass.picture.clone());
        }
        Task::none()
    }
    /// Whether the clock is drawn behind the glass now, so its own surface
    /// stays empty.
    pub(super) fn clock_behind_glass(&self) -> bool {
        // Every update ends with `glass_sync`, which clears the place when
        // the clock must come back to the front.
        self.glass.place.is_some()
            && !self.layout_editor.active
            && self
                .glass
                .picture
                .as_ref()
                .is_some_and(|(output, _)| self.rain.shows_behind(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glass_finds_where_the_usable_area_starts_from_the_panels() {
        let full = [1920., 1080.];
        assert_eq!(usable_origin(full, full, &[]), Some([0., 0.]));
        // A top panel alone, or a bottom dock alone.
        assert_eq!(
            usable_origin(full, [1920., 1048.], &[(Anchor::Top, 32)]),
            Some([0., 32.])
        );
        assert_eq!(
            usable_origin(full, [1920., 1016.], &[(Anchor::Bottom, 64)]),
            Some([0., 0.])
        );
        // A left dock: only x moves.
        assert_eq!(
            usable_origin(full, [1856., 1080.], &[(Anchor::Left, 64)]),
            Some([64., 0.])
        );
        // A top panel and a bottom dock share the space by thickness, even
        // when the measured strip differs from the estimate.
        assert_eq!(
            usable_origin(
                full,
                [1920., 980.],
                &[(Anchor::Top, 40), (Anchor::Bottom, 60)]
            ),
            Some([0., 40.])
        );
        assert_eq!(
            usable_origin(
                full,
                [1920., 984.],
                &[(Anchor::Top, 40), (Anchor::Bottom, 60)]
            ),
            Some([0., 38.])
        );
        // Space kept by something the panels do not explain.
        assert_eq!(usable_origin(full, [1920., 1040.], &[]), None);
        assert_eq!(
            usable_origin(full, [1880., 1080.], &[(Anchor::Top, 40)]),
            None
        );
        assert_eq!(usable_origin(full, [2000., 1080.], &[]), None);
    }
    #[test]
    fn glass_moves_the_clock_behind_the_rain_and_back() {
        if crate::displays::test_support::isolated(
            "app::glass::tests::glass_moves_the_clock_behind_the_rain_and_back",
        ) {
            return;
        }
        use crate::displays::{OutputEvent, test_support::info};
        let mut config = Config::default();
        config.rain.enabled = true;
        let (mut cards, _) = Cards::init(Core::default(), config);
        let wire = crate::displays::test_support::Wire::new();
        let handle = wire.output(1);
        cards.displays.update(
            OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
            handle.clone(),
        );
        let _ = cards.reconcile_output();
        let _ = cards.update(Message::RainLifecycle(crate::rain::lifecycle::State {
            ready: true,
            ..Default::default()
        }));
        assert!(cards.rain.showing());
        assert!(!cards.clock_behind_glass(), "no rain drawn yet");
        cards.rain.ready_for_test();
        // The layer area has been measured.
        cards.output_known = true;
        let (place, frame) = cards
            .glass_place()
            .expect("the clock can stand behind the glass");
        assert_eq!(place.output, handle);
        let size = cards.widget_size(Widget::Clock);
        assert!(
            (place.rect[2] - size[0] / 1920.).abs() < 1e-4,
            "{:?}",
            place.rect
        );
        assert!(
            (place.rect[3] - size[1] / 1080.).abs() < 1e-4,
            "{:?}",
            place.rect
        );
        assert!((place.rect[0] - frame.origin[0] / 1920.).abs() < 1e-4);
        // The worker's answer lands on the rain surface and empties the
        // clock's own surface.
        let _ = cards.glass_sync();
        let asked = cards.glass.id;
        let _ = cards.update(Message::GlassReady(
            asked,
            Some(Pixels {
                width: 4,
                height: 2,
                rgba: vec![255; 32],
            }),
        ));
        assert!(cards.clock_behind_glass());
        // The layout editor brings it back to the front at once.
        cards.layout_editor.active = true;
        assert!(!cards.clock_behind_glass());
        cards.layout_editor.active = false;
        assert!(cards.clock_behind_glass());
        // So does a clock on a card, and the rain going away.
        let mut boxed = cards.config.clone();
        boxed.appearance.contrast_assist = true;
        assert!(!crate::depth::behind_glass(&boxed));
        let _ = cards.rain.invalidate();
        assert!(!cards.clock_behind_glass());
        let _ = cards.glass_sync();
        assert!(cards.glass.picture.is_none());
    }
    #[test]
    fn glass_takes_only_the_clock_it_asked_for_last() {
        let wire = crate::displays::test_support::Wire::new();
        let output = wire.output(3);
        let mut state = State {
            place: Some(Place {
                output: output.clone(),
                rect: [0.1, 0.2, 0.3, 0.1],
            }),
            ..Default::default()
        };
        state.cancel();
        let asked = state.id;
        let pixels = || {
            Some(Pixels {
                width: 2,
                height: 1,
                rgba: vec![255; 8],
            })
        };
        assert!(!state.complete(asked.wrapping_sub(1), pixels()));
        assert!(state.picture.is_none());
        assert!(state.complete(asked, pixels()));
        let (on, behind) = state.picture.clone().expect("drawn clock");
        assert_eq!(on, output);
        assert_eq!(behind.rect, [0.1, 0.2, 0.3, 0.1]);
        assert!(state.complete(asked, None));
        assert!(
            state.picture.is_none(),
            "a failed draw shows the plain clock"
        );
        state.clear();
        assert!(state.place.is_none() && state.picture.is_none());
    }
}
