//! Optical magnification inside the app-list's existing icon slots.
//! Icon artwork scales around its center. The button, running indicator,
//! drag target and rectangle the compositor uses to minimize a window stay put.
use cosmic::iced::{
    Event, Length, Point, Rectangle, Size, Transformation,
    advanced::{
        Clipboard, Layout, Renderer as _, Shell, Widget, layout, renderer,
        widget::{Tree, tree},
    },
    mouse, window,
};
use cosmic::{Element, Renderer, Theme, theme::Button, widget::button};
use std::time::Instant;

const REST: f32 = 0.77;
const PEAK: f32 = REST * 1.45;

fn hover_scale(bounds: Rectangle, pointer: Point, horizontal: bool) -> f32 {
    let offset = pointer - bounds.center();
    let (along, across, length, thickness) = if horizontal {
        (offset.x, offset.y, bounds.width, bounds.height)
    } else {
        (offset.y, offset.x, bounds.height, bounds.width)
    };
    if across.abs() > thickness * 0.85 {
        return REST;
    }
    let t = (1. - along.abs() / (length * 2.1).max(1.)).clamp(0., 1.);
    REST + (PEAK - REST) * t * t * (3. - 2. * t)
}

pub fn button_style() -> Button {
    // The running indicator shows the active app. Keep the native outline for
    // keyboard focus and leave the area around pointer-driven artwork clear.
    let appearance = |focused, theme: &Theme| button::Style {
        background: None,
        ..button::Catalog::active(theme, focused, false, &Button::AppletIcon)
    };
    Button::Custom {
        active: Box::new(appearance),
        hovered: Box::new(appearance),
        pressed: Box::new(appearance),
        disabled: Box::new(|theme| button::Style {
            background: None,
            ..button::Catalog::disabled(theme, &Button::AppletIcon)
        }),
    }
}

#[derive(Debug)]
struct Motion {
    scale: f32,
    target: f32,
    last: Option<Instant>,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            scale: REST,
            target: REST,
            last: None,
        }
    }
}
impl Motion {
    fn advance(&mut self, now: Instant) -> bool {
        let dt = self
            .last
            .replace(now)
            .map_or(0., |t| now.saturating_duration_since(t).as_secs_f32());
        self.scale += (self.target - self.scale) * (1. - (-dt / 0.052).exp());
        if (self.scale - self.target).abs() < 0.001 {
            self.scale = self.target;
            self.last = None;
            false
        } else {
            true
        }
    }
}

pub fn icon<'a, M: 'a>(image: Element<'a, M>, horizontal: bool, animated: bool) -> Element<'a, M> {
    Element::new(Magnify {
        image,
        horizontal,
        animated,
    })
}
struct Magnify<'a, M> {
    image: Element<'a, M>,
    horizontal: bool,
    animated: bool,
}
impl<M> Widget<M, Theme, Renderer> for Magnify<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Motion>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Motion::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.image)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.image));
    }
    fn size(&self) -> Size<Length> {
        self.image.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.image
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _: &Rectangle,
    ) {
        let motion = tree.state.downcast_mut::<Motion>();
        if !self.animated {
            *motion = Motion::default();
            return;
        }
        let now = match event {
            Event::Window(window::Event::RedrawRequested(now)) => *now,
            Event::Mouse(
                mouse::Event::CursorMoved { .. }
                | mouse::Event::CursorLeft
                | mouse::Event::CursorEntered,
            ) => Instant::now(),
            _ => return,
        };
        let target = match (event, cursor.position()) {
            (Event::Mouse(mouse::Event::CursorLeft), _) | (_, None) => REST,
            (_, Some(pointer)) => hover_scale(layout.bounds(), pointer, self.horizontal),
        };
        let changed = (target - motion.target).abs() > f32::EPSILON;
        motion.target = target;
        if !changed && motion.last.is_none() {
            return;
        }
        if !matches!(event, Event::Window(window::Event::RedrawRequested(_))) {
            // A fast pointer may send several positions before the next paint.
            // Update the destination without postponing an outstanding frame.
            if motion.last.is_none() {
                motion.last = Some(now);
                shell.request_redraw();
            }
            return;
        }
        if motion.advance(now) {
            // The panel also schedules a frame after ours. Half a 60 Hz
            // interval leaves it time to present and bounds fast nested outputs.
            shell.request_redraw_at(now + std::time::Duration::from_micros(8_333));
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        r: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let scale = if self.animated {
            tree.state.downcast_ref::<Motion>().scale
        } else {
            1.
        };
        let c = layout.bounds().center();
        let transform = Transformation::translate(c.x, c.y)
            * Transformation::scale(scale)
            * Transformation::translate(-c.x, -c.y);
        r.with_transformation(transform, |r| {
            self.image.as_widget().draw(
                &tree.children[0],
                r,
                theme,
                style,
                layout,
                cursor,
                viewport,
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::widget::button::Catalog;
    use std::time::Duration;

    #[test]
    fn hover_peaks_at_45_percent_on_either_dock_axis() {
        let slot = Rectangle::new(Point::ORIGIN, Size::new(48., 48.));
        let center = slot.center();
        for horizontal in [true, false] {
            let scale_at = |distance| {
                let pointer = if horizontal {
                    Point::new(center.x + distance, center.y)
                } else {
                    Point::new(center.x, center.y + distance)
                };
                hover_scale(slot, pointer, horizontal)
            };
            assert!((scale_at(0.) / REST - 1.45).abs() < 0.0001);
            assert!(REST < scale_at(72.) && scale_at(72.) < scale_at(0.));
            assert_eq!(scale_at(-72.), scale_at(72.));
            assert_eq!(scale_at(144.), REST);
        }
    }

    #[test]
    fn pointer_and_selection_leave_only_the_artwork_visible() {
        let style = button_style();
        for theme in [Theme::dark(), Theme::light_hc()] {
            for selected in [false, true] {
                for state in [
                    theme.active(false, selected, &style),
                    theme.hovered(false, selected, &style),
                    theme.pressed(false, selected, &style),
                    theme.disabled(&style),
                ] {
                    assert!(state.background.is_none() && state.overlay.is_none());
                    assert_eq!(state.border_width, 0.);
                    assert_eq!(state.outline_width, 0.);
                }
            }
            assert!(theme.active(true, false, &style).outline_width > 0.);
        }
    }

    #[test]
    fn growth_reversal_and_exit_settle_without_idle_frames() {
        let start = Instant::now();
        let mut state = Motion::default();
        state.target = PEAK;
        for i in 0..8 {
            let old = state.scale;
            assert!(state.advance(start + Duration::from_millis(i * 16)));
            assert!(state.scale >= old && state.scale < PEAK);
        }
        let peak = state.scale;
        state.target = REST;
        for i in 8..40 {
            let before = state.scale;
            state.advance(start + Duration::from_millis(i * 16));
            assert!(state.scale <= before);
        }
        assert!(peak > 1.);
        assert_eq!(state.scale, REST);
        assert!(state.last.is_none());
    }
}

/// Follow the desktop's active GSettings backend. The monitor sleeps between
/// changes and drops with this applet, including on panel replacement.
pub fn motion_settings() -> impl cosmic::iced::futures::Stream<Item = bool> {
    use cosmic::iced::futures::SinkExt;
    use tokio::io::{AsyncBufReadExt, BufReader};
    cosmic::iced::stream::channel(1, async |mut out| {
        const KEYS: [&str; 2] = ["org.gnome.desktop.interface", "enable-animations"];
        let monitor = tokio::process::Command::new("gsettings")
            .arg("monitor")
            .args(KEYS)
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        let initial = tokio::process::Command::new("gsettings")
            .arg("get")
            .args(KEYS)
            .output()
            .await
            .ok()
            .is_none_or(|v| !v.status.success() || v.stdout.trim_ascii() != b"false");
        if out.send(initial).await.is_err() {
            return;
        }
        if let Ok(mut process) = monitor
            && let Some(stdout) = process.stdout.take()
        {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some((_, value)) = line.rsplit_once(':')
                    && matches!(value.trim(), "true" | "false")
                    && out.send(value.trim() == "true").await.is_err()
                {
                    return;
                }
            }
        }
        std::future::pending::<()>().await;
    })
}
