//! Positions native controls within one animated capsule. No copied title layer.
use super::{COMPACT_WIDTH, HEIGHT, Message, Palette, WIDTH, alpha};
use cosmic::iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget, layout, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use cosmic::iced::{Border, Color, Event, Length, Point, Rectangle, Size, Vector, mouse, window};
use cosmic::{Element, Renderer, Theme};
const TITLE_HEIGHT: f32 = 28.;
const PIN_WIDTH: f32 = 28.;
/// The icon column, so details line up with the title's text.
const TEXT_INDENT: f32 = 24.;
const META_Y: f32 = 32.;
const ACTIONS_Y: f32 = 76.;
const ACTIONS_HEIGHT: f32 = 44.;
/// Without a progress line the actions move up and the card ends sooner.
const TIGHT: f32 = 18.;
/// Space between a Standard bar's lower edge and the first detail line.
const CARD_PAD: f32 = 10.;
/// An attached card holds everything below the title line.
pub(super) const CARD_HEIGHT: f32 = HEIGHT - META_Y + CARD_PAD;
/// An attached card keeps the title's text column: the resting capsule plus
/// its own margin on each side.
pub(super) const ATTACHED_WIDTH: f32 = COMPACT_WIDTH + 24.;
/// Concave corners where an attached card meets the bar.
pub(super) const SHOULDER: f32 = 12.;
const CARD_RADIUS: f32 = 20.;
/// A very clear bar would leave the details unreadable over the wallpaper.
const MIN_OPACITY: f32 = 0.7;

#[derive(Default)]
struct State {
    hovered: bool,
    pointer_left: bool,
    last_frame: Option<std::time::Instant>,
    /// Vertical touchpad travel in the current gesture.
    swipe: f32,
    swipe_at: Option<std::time::Instant>,
    /// This gesture has already asked for a dismissal.
    swiped: bool,
}
/// Touchpad travel that counts as a swipe, in either direction.
const SWIPE: f32 = 40.;
/// A pause this long starts a new gesture.
const SWIPE_GAP: std::time::Duration = std::time::Duration::from_millis(250);
pub(super) struct Surface<'a> {
    pub palette: Palette,
    pub frame: super::host::Frame,
    pub show_content: bool,
    pub slot_width: f32,
    pub chooser: bool,
    /// The badge or icon row beside the title, 0 when there is none.
    pub others_width: f32,
    /// How much of it the resting title mirrors on its left to stay centred.
    pub balance: f32,
    /// The details may wrap onto a second line (a notification's message).
    pub wrap_meta: bool,
    /// In reading and keyboard order: title, pin, details, actions, activities.
    pub children: Vec<Element<'a, Message>>,
    pub progress: f32,
    pub popup: bool,
    pub animate: bool,
    /// Report pointer motion, only needed while waiting for it to rest.
    pub waiting: bool,
    /// Elapsed share and color of an optional line beneath the details.
    pub bar: Option<(f32, Color)>,
    /// Nothing sits between the details and the actions.
    pub tight: bool,
}
impl Surface<'_> {
    /// The resting slot. On a Standard bar it is also where the title stays.
    fn slot(&self, b: Rectangle) -> Rectangle {
        Rectangle {
            x: b.x + (b.width - self.slot_width) / 2.,
            y: b.y,
            width: self.slot_width,
            height: if self.popup {
                self.frame.bar()
            } else {
                self.frame.height
            },
        }
    }
    /// An attached card: it widens from the slot and drops from the bar.
    fn card(&self, b: Rectangle) -> Rectangle {
        let width = self.slot_width + (ATTACHED_WIDTH - self.slot_width) * self.progress;
        Rectangle {
            x: (b.x + (b.width - width) / 2.).round(),
            y: b.y + self.frame.bar(),
            width: width.round(),
            height: (CARD_HEIGHT - self.lift()) * self.progress,
        }
    }
    fn contains(&self, b: Rectangle, cursor: mouse::Cursor) -> bool {
        if self.frame.attached && self.popup {
            cursor.is_over(self.slot(b)) || cursor.is_over(self.card(b))
        } else {
            cursor.is_over(self.rect(b))
        }
    }
    fn rect(&self, b: Rectangle) -> Rectangle {
        let start = self.frame.height
            + if self.popup {
                2. * self.frame.padding
            } else {
                0.
            };
        let width = if self.popup {
            self.slot_width + 2. * self.frame.padding + (WIDTH - self.slot_width) * self.progress
        } else {
            self.slot_width
        };
        Rectangle {
            x: b.x + (b.width - width) / 2.,
            y: b.y,
            width,
            height: start + (self.frame.popup_height() - self.lift() - start) * self.progress,
        }
    }
}
impl Surface<'_> {
    fn lift(&self) -> f32 {
        if self.tight && !self.chooser { TIGHT } else { 0. }
    }
}
impl Widget<Message, Theme, Renderer> for Surface<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn diff(&mut self, t: &mut Tree) {
        t.diff_children(&mut self.children)
    }
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(if self.popup {
                self.frame.popup_width()
            } else {
                self.slot_width
            }),
            Length::Fixed(if self.popup {
                self.frame.popup_height()
            } else {
                self.frame.height
            }),
        )
    }
    fn layout(&mut self, t: &mut Tree, r: &Renderer, _: &layout::Limits) -> layout::Node {
        let p = self.progress;
        let outer_width = if self.popup {
            self.frame.popup_width()
        } else {
            self.slot_width
        };
        let top = self.frame.inset() + if self.popup { self.frame.padding } else { 0. };
        let attached = self.frame.attached;
        let compact = COMPACT_WIDTH.min(self.slot_width);
        // On a Standard bar the title line keeps its resting width and place;
        // only the card beneath it grows.
        let w = if attached {
            compact
        } else {
            compact + (WIDTH - compact) * p
        };
        let inset = if attached { 12. } else { 12. + 2. * p };
        let (left, right) = (
            (outer_width - w) / 2. + inset,
            (outer_width + w) / 2. - inset,
        );
        let inner = right - left;
        // Details share the title's text column; actions may use the card's
        // full width, centred.
        let (card_left, card_inner, action_left, action_inner, drop) = if attached {
            let w = self.slot_width + (ATTACHED_WIDTH - self.slot_width) * p;
            let edge = (outer_width - w) / 2.;
            (
                edge + 24.,
                w - 48.,
                edge + 12.,
                w - 24.,
                self.frame.bar() + CARD_PAD - META_Y - top,
            )
        } else {
            (left, inner, left, inner, 0.)
        };
        let rise = if attached { 0. } else { 6. * p };
        let lift = self.lift();
        let mut place = |index: usize, x: f32, y: f32, width: f32, height: f32| {
            self.children[index]
                .as_widget_mut()
                .layout(
                    &mut t.children[index],
                    r,
                    &layout::Limits::new(Size::ZERO, Size::new(width.max(0.), height)),
                )
                .move_to(Point::new(x, y + top))
        };
        let actions_y = drop + if self.chooser { 40. } else { ACTIONS_Y - lift };
        let actions = place(
            3,
            0.,
            actions_y,
            action_inner,
            if self.chooser {
                HEIGHT - 50.
            } else {
                ACTIONS_HEIGHT
            },
        );
        let width = actions.size().width;
        let (others, balance) = (self.others_width, self.balance);
        let nodes = vec![
            place(
                0,
                left + balance * (1. - p),
                2. + rise,
                inner - (PIN_WIDTH + 4.) * p - balance * (1. - p) - others,
                TITLE_HEIGHT,
            ),
            place(
                1,
                right - PIN_WIDTH,
                if attached { 2. } else { 8. },
                PIN_WIDTH,
                28.,
            ),
            place(
                2,
                card_left + TEXT_INDENT,
                META_Y + drop,
                card_inner - TEXT_INDENT,
                if self.wrap_meta { 32. } else { 18. },
            ),
            actions.move_to(Point::new(
                action_left + (action_inner - width) / 2.,
                top + actions_y,
            )),
            place(
                4,
                right - others - (PIN_WIDTH + 4.) * p,
                // Centred on the title line.
                6. + rise,
                others,
                20.,
            ),
            place(5, card_left, 58. + drop, card_inner, 15.),
        ];
        layout::Node::with_children(
            Size::new(
                outer_width,
                if self.popup {
                    self.frame.popup_height()
                } else {
                    self.frame.height
                },
            ),
            nodes,
        )
    }
    fn operate(&mut self, t: &mut Tree, l: Layout<'_>, r: &Renderer, o: &mut dyn Operation) {
        if !self.show_content {
            return;
        }
        for (i, ((c, t), l)) in self
            .children
            .iter_mut()
            .zip(&mut t.children)
            .zip(l.children())
            .enumerate()
        {
            if self.show_content && (i == 0 || i == 4 || self.progress > 0.9) {
                c.as_widget_mut().operate(t, l, r, o)
            }
        }
    }
    fn update(
        &mut self,
        t: &mut Tree,
        event: &Event,
        l: Layout<'_>,
        cursor: mouse::Cursor,
        r: &Renderer,
        clip: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let rect = self.rect(l.bounds());
        let state = t.state.downcast_mut::<State>();
        // The panel proxy can retain an Available cursor after CursorLeft.
        // Fence that stale position until actual pointer reentry, including
        // redraws, so tooltips and pressed controls cannot stay active outside.
        match event {
            Event::Mouse(mouse::Event::CursorLeft) => state.pointer_left = true,
            Event::Mouse(mouse::Event::CursorEntered | mouse::Event::CursorMoved { .. }) => {
                state.pointer_left = false;
            }
            _ => {}
        }
        let cursor = if state.pointer_left {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        if matches!(event, Event::Mouse(_)) {
            if crate::tools::preview() && std::env::var_os("HALCYON_ACTIVITY_TRACE").is_some() {
                eprintln!(
                    "surface popup={} event={event:?} cursor={cursor:?} rect={rect:?}",
                    self.popup
                );
            }
            let entered = matches!(event, Event::Mouse(mouse::Event::CursorEntered));
            let left = matches!(event, Event::Mouse(mouse::Event::CursorLeft));
            let inside = entered || (!left && self.contains(l.bounds(), cursor));
            if inside != state.hovered || entered || left {
                state.hovered = inside;
                shell.publish(if inside {
                    Message::Enter(self.popup)
                } else {
                    Message::Leave(self.popup)
                });
            } else if self.waiting
                && inside
                && matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }))
            {
                shell.publish(Message::Moved);
            }
        }
        if let Event::Window(window::Event::RedrawRequested(at)) = event
            && self.animate
        {
            let cadence = std::time::Duration::from_micros(8_333);
            if state
                .last_frame
                .is_none_or(|last| at.saturating_duration_since(last) >= cadence)
            {
                state.last_frame = Some(*at);
                shell.publish(Message::Frame(*at));
            }
            shell.request_redraw_at(state.last_frame.unwrap_or(*at) + cadence);
        }
        for (i, ((c, t), l)) in self
            .children
            .iter_mut()
            .zip(&mut t.children)
            .zip(l.children())
            .enumerate()
        {
            if self.show_content && (i == 0 || i == 4 || self.progress > 0.9) {
                c.as_widget_mut()
                    .update(t, event, l, cursor, r, clip, shell, viewport)
            }
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && !self.contains(l.bounds(), cursor)
            && self.popup
        {
            shell.publish(Message::Close);
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(button))
                if self.contains(l.bounds(), cursor) =>
            {
                match button {
                    mouse::Button::Middle => shell.publish(Message::Middle),
                    mouse::Button::Right => shell.publish(Message::Secondary),
                    _ => {}
                }
            }
            // Touchpads scroll in pixels; a wheel's lines are left alone.
            // Natural scrolling flips the sign, so either way is a swipe.
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { y, .. },
            }) if !shell.is_event_captured() && self.contains(l.bounds(), cursor) => {
                let now = std::time::Instant::now();
                if state
                    .swipe_at
                    .is_none_or(|at| now.saturating_duration_since(at) > SWIPE_GAP)
                {
                    state.swipe = 0.;
                    state.swiped = false;
                }
                state.swipe_at = Some(now);
                state.swipe += y;
                if !state.swiped && state.swipe.abs() >= SWIPE {
                    state.swiped = true;
                    shell.publish(Message::Swipe);
                }
            }
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        t: &Tree,
        l: Layout<'_>,
        cursor: mouse::Cursor,
        v: &Rectangle,
        r: &Renderer,
    ) -> mouse::Interaction {
        if !self.show_content {
            return mouse::Interaction::default();
        }
        let cursor = if t.state.downcast_ref::<State>().pointer_left {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        self.children
            .iter()
            .zip(&t.children)
            .zip(l.children())
            .enumerate()
            .filter(|(i, _)| *i == 0 || *i == 4 || self.progress > 0.9)
            .map(|(_, ((c, t), l))| c.as_widget().mouse_interaction(t, l, cursor, v, r))
            .max()
            .unwrap_or_default()
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        if !self.show_content {
            return None;
        }
        if self.progress <= 0.9 {
            return None;
        }
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
    fn draw(
        &self,
        t: &Tree,
        r: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        l: Layout<'_>,
        cursor: mouse::Cursor,
        v: &Rectangle,
    ) {
        if !self.show_content {
            return;
        }
        let cursor = if t.state.downcast_ref::<State>().pointer_left {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        if self.frame.attached && self.popup {
            return self.draw_attached(t, r, theme, style, l, cursor, v);
        }
        let rect = self.rect(l.bounds());
        let top_radius = self
            .frame
            .radius
            .min((self.frame.height + 2. * self.frame.padding) / 2.)
            .min(rect.width / 2.);
        let bottom_radius = top_radius + (20. - top_radius) * self.progress;
        let radius = cosmic::iced::border::Radius {
            top_left: top_radius,
            top_right: top_radius,
            bottom_left: bottom_radius,
            bottom_right: bottom_radius,
        };
        r.fill_quad(
            renderer::Quad {
                bounds: rect,
                border: Border {
                    radius,
                    // Resting content belongs to the panel. Its own outline only
                    // appears as the popup grows beyond the panel.
                    width: self.progress,
                    color: alpha(self.palette.ink, 0.12 * self.progress),
                },
                ..Default::default()
            },
            alpha(self.palette.surface, self.progress),
        );
        if let Some(clip) = rect.intersection(v) {
            r.with_layer(clip, |r| {
                for (i, ((c, t), l)) in self
                    .children
                    .iter()
                    .zip(&t.children)
                    .zip(l.children())
                    .enumerate()
                {
                    if !(self.chooser && matches!(i, 2 | 5))
                        && (i == 0 || i == 4 || self.progress > 0.45)
                    {
                        c.as_widget().draw(t, r, theme, style, l, cursor, &clip)
                    }
                }
                self.draw_bar(r, l, rect);
            });
        }
    }
}
impl Surface<'_> {
    /// The elapsed line beneath the details, spanning the card symmetrically.
    fn draw_bar(&self, r: &mut Renderer, l: Layout<'_>, rect: Rectangle) {
        let Some(((fraction, color), details)) = self.bar.zip(l.children().nth(2)) else {
            return;
        };
        if self.progress <= 0.5 || self.chooser {
            return;
        }
        let reveal = ((self.progress - 0.5) / 0.4).clamp(0., 1.);
        let track = Rectangle {
            x: details.bounds().x - TEXT_INDENT,
            width: rect.width - 2. * (details.bounds().x - TEXT_INDENT - rect.x),
            y: details.bounds().y + details.bounds().height + 4.,
            height: 3.,
            ..details.bounds()
        };
        let fill = |bounds: Rectangle, color: Color, r: &mut Renderer| {
            r.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border {
                        radius: 1.5.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                color,
            );
        };
        fill(track, alpha(self.palette.ink, 0.12 * reveal), r);
        fill(
            Rectangle {
                width: track.width * fraction.clamp(0., 1.),
                ..track
            },
            alpha(color, reveal),
            r,
        );
    }
    /// A Standard bar keeps its title in place. The card hangs from the bar's
    /// lower edge in the bar's own colour and opacity, joined by concave
    /// shoulders so the two read as one shape.
    #[allow(clippy::too_many_arguments)]
    fn draw_attached(
        &self,
        t: &Tree,
        r: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        l: Layout<'_>,
        cursor: mouse::Cursor,
        v: &Rectangle,
    ) {
        let card = self.card(l.bounds());
        let fill = alpha(self.palette.surface, self.frame.opacity.max(MIN_OPACITY));
        if card.height > 0.5 {
            let radius = (CARD_RADIUS * self.progress).min(card.height / 2.);
            r.fill_quad(
                renderer::Quad {
                    bounds: card,
                    border: Border {
                        radius: cosmic::iced::border::Radius {
                            top_left: 0.,
                            top_right: 0.,
                            bottom_left: radius,
                            bottom_right: radius,
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
                fill,
            );
            let s = (SHOULDER * self.progress).min(card.height).round();
            if s >= 1. {
                for (corner, centre) in [
                    (card.x - s, card.x - s),
                    (card.x + card.width, card.x + card.width + s),
                ] {
                    // A ring whose inner edge is the fillet, clipped to the
                    // corner square, leaves only the concave fill behind.
                    let ring = Rectangle {
                        x: centre - 2. * s,
                        y: card.y + s - 2. * s,
                        width: 4. * s,
                        height: 4. * s,
                    };
                    r.with_layer(
                        Rectangle {
                            x: corner,
                            y: card.y,
                            width: s,
                            height: s,
                        },
                        |r| {
                            r.fill_quad(
                                renderer::Quad {
                                    bounds: ring,
                                    border: Border {
                                        radius: (2. * s).into(),
                                        width: s,
                                        color: fill,
                                    },
                                    ..Default::default()
                                },
                                Color::TRANSPARENT,
                            )
                        },
                    );
                }
            }
        }
        let children = self.children.iter().zip(&t.children).zip(l.children());
        for (i, ((c, t), l)) in children.enumerate() {
            match i {
                // The title line belongs to the bar and is never clipped by the card.
                0 | 1 | 4 => {
                    if i == 0 || i == 4 || self.progress > 0.45 {
                        c.as_widget().draw(t, r, theme, style, l, cursor, v)
                    }
                }
                _ => {}
            }
        }
        if let Some(clip) = card.intersection(v) {
            r.with_layer(clip, |r| {
                let children = self.children.iter().zip(&t.children).zip(l.children());
                for (i, ((c, t), l)) in children.enumerate() {
                    if matches!(i, 2 | 3 | 5)
                        && !(self.chooser && matches!(i, 2 | 5))
                        && self.progress > 0.45
                    {
                        c.as_widget().draw(t, r, theme, style, l, cursor, &clip)
                    }
                }
                self.draw_bar(r, l, card);
            });
        }
    }
}
impl<'a> From<Surface<'a>> for Element<'a, Message> {
    fn from(value: Surface<'a>) -> Self {
        Element::new(value)
    }
}
