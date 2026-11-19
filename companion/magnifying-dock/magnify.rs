//! Optical magnification inside the app-list's existing icon slots.
//! Icon artwork scales around its center. The button, running indicator,
//! drag target and rectangle the compositor uses to minimize a window stay put.
use cosmic::iced::{
    Event, Length, Rectangle, Size, Transformation,
    advanced::{
        Clipboard, Layout, Renderer as _, Shell, Widget, layout, renderer,
        widget::{Tree, tree},
    },
    mouse, window,
};
use cosmic::{Element, Renderer, Theme};
use std::time::Instant;

const REST: f32 = 0.77;
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
        let b = layout.bounds();
        let target = if matches!(event, Event::Mouse(mouse::Event::CursorLeft)) {
            REST
        } else {
            cursor.position().map_or(REST, |p| {
                let c = b.center();
                let (along, across, extent, cross) = if self.horizontal {
                    (p.x - c.x, p.y - c.y, b.width, b.height)
                } else {
                    (p.y - c.y, p.x - c.x, b.height, b.width)
                };
                if across.abs() > cross * 0.85 {
                    return REST;
                }
                let t = (1. - along.abs() / (extent * 2.1).max(1.)).clamp(0., 1.);
                REST + (1. - REST) * t * t * (3. - 2. * t)
            })
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
    use std::time::Duration;
    #[test]
    fn growth_reversal_and_exit_settle_without_idle_frames() {
        let start = Instant::now();
        let mut state = Motion::default();
        state.target = 1.;
        for i in 0..8 {
            let old = state.scale;
            assert!(state.advance(start + Duration::from_millis(i * 16)));
            assert!(state.scale >= old && state.scale < 1.);
        }
        let peak = state.scale;
        state.target = REST;
        for i in 8..40 {
            let before = state.scale;
            state.advance(start + Duration::from_millis(i * 16));
            assert!(state.scale <= before);
        }
        assert!(peak > 0.93);
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
