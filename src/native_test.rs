//! Native Iced widget-tree driver used by layout regressions. No compositor,
//! accounts or live preferences are touched; screenshots use tiny-skia.
use cosmic::iced::advanced::{
    clipboard,
    renderer::{Headless, Style},
    widget::{Operation, operation::Focusable},
};
use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
use cosmic::iced::{Event, Point, Rectangle, Size, mouse};

pub struct Native {
    renderer: cosmic::Renderer,
    cache: Cache,
    pub size: Size,
}
impl Native {
    pub fn new(size: Size) -> Self {
        Self::with_backend(size, "tiny-skia")
    }
    pub fn with_backend(size: Size, backend: &str) -> Self {
        Self::with_text_size(size, backend, 14.)
    }
    pub fn with_text_size(size: Size, backend: &str, points: f32) -> Self {
        Self {
            renderer: cosmic::iced::futures::executor::block_on(
                <cosmic::Renderer as Headless>::new(
                    cosmic::font::default(),
                    cosmic::iced::Pixels(points),
                    Some(backend),
                ),
            )
            .unwrap(),
            cache: Cache::default(),
            size,
        }
    }
    pub fn renderer_name(&self) -> String {
        self.renderer.name()
    }
    pub fn bounds<M>(&mut self, view: cosmic::Element<'_, M>, id: &str) -> Rectangle {
        self.try_bounds(view, id)
            .unwrap_or_else(|| panic!("native control exists: {id}"))
    }
    /// Like `bounds`, for a control that may be absent.
    pub fn try_bounds<M>(&mut self, view: cosmic::Element<'_, M>, id: &str) -> Option<Rectangle> {
        self.find(view, Some(cosmic::widget::Id::new(id.to_string())))
    }
    /// Where keyboard focus is, on screen, if anything has it.
    pub fn focused<M>(&mut self, view: cosmic::Element<'_, M>) -> Option<Rectangle> {
        self.find(view, None)
    }
    /// The widget named `id`, or the focused one when `id` is `None`.
    fn find<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        id: Option<cosmic::widget::Id>,
    ) -> Option<Rectangle> {
        struct Find {
            id: Option<cosmic::widget::Id>,
            found: Option<Rectangle>,
            offset: cosmic::iced::Vector,
            pending: cosmic::iced::Vector,
        }
        impl Operation for Find {
            fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
                let previous = self.offset;
                self.offset += std::mem::take(&mut self.pending);
                f(self);
                self.offset = previous;
            }
            fn scrollable(
                &mut self,
                _: Option<&cosmic::widget::Id>,
                _: Rectangle,
                _: Rectangle,
                translation: cosmic::iced::Vector,
                _: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
            ) {
                self.pending = translation;
            }
            fn container(&mut self, id: Option<&cosmic::widget::Id>, bounds: Rectangle) {
                if self.id.is_some() && id == self.id.as_ref() {
                    self.found = Some(bounds - self.offset);
                }
            }
            fn focusable(
                &mut self,
                id: Option<&cosmic::widget::Id>,
                bounds: Rectangle,
                state: &mut dyn Focusable,
            ) {
                let hit = match &self.id {
                    Some(target) => id == Some(target),
                    None => state.is_focused(),
                };
                if hit {
                    self.found = Some(bounds - self.offset);
                }
            }
        }
        let mut ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut find = Find {
            id,
            found: None,
            offset: Default::default(),
            pending: Default::default(),
        };
        ui.operate(&self.renderer, &mut find);
        self.cache = ui.into_cache();
        find.found
    }
    /// What a screen reader is told about every node: its role, name and
    /// bounds, in window coordinates.
    pub fn a11y<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
    ) -> Vec<(
        iced_accessibility::accesskit::Role,
        Option<String>,
        Rectangle,
    )> {
        let ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let tree = ui.a11y_nodes(mouse::Cursor::Unavailable);
        let nodes = tree
            .root()
            .iter()
            .chain(tree.children())
            .map(|node| {
                let node = node.node();
                let bounds = node.bounds().map_or(Rectangle::default(), |b| Rectangle {
                    x: b.x0 as f32,
                    y: b.y0 as f32,
                    width: (b.x1 - b.x0) as f32,
                    height: (b.y1 - b.y0) as f32,
                });
                (node.role(), node.label().map(str::to_owned), bounds)
            })
            .collect();
        self.cache = ui.into_cache();
        nodes
    }
    pub fn operate<M>(&mut self, view: cosmic::Element<'_, M>, mut operation: Box<dyn Operation>) {
        let mut ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        loop {
            ui.operate(&self.renderer, operation.as_mut());
            match operation.finish() {
                cosmic::iced::advanced::widget::operation::Outcome::Chain(next) => operation = next,
                _ => break,
            }
        }
        self.cache = ui.into_cache();
    }
    pub fn event<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        event: Event,
        point: Point,
    ) -> (Vec<M>, cosmic::iced::event::Status) {
        let mut ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            &[event],
            mouse::Cursor::Available(point),
            &mut self.renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        (messages, statuses[0])
    }
    pub fn render<M>(&mut self, view: cosmic::Element<'_, M>, theme: &cosmic::Theme) -> Vec<u8> {
        self.render_with_cursor(view, theme, mouse::Cursor::Unavailable)
    }
    /// Keep layout in logical pixels; exercise the native renderer's physical
    /// scale independently, as on a scaled output.
    pub fn render_scaled<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        theme: &cosmic::Theme,
        scale: f64,
    ) -> Vec<u8> {
        self.render_at_scale(view, theme, mouse::Cursor::Unavailable, scale)
    }
    /// A hovered frame must update and draw the same UI: Iced rebuilds
    /// overlay layout during update, and it is not part of the widget cache.
    pub fn render_hover<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        theme: &cosmic::Theme,
        point: Point,
    ) -> Vec<u8> {
        self.render_with_cursor(view, theme, mouse::Cursor::Available(point))
    }
    fn render_with_cursor<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        theme: &cosmic::Theme,
        cursor: mouse::Cursor,
    ) -> Vec<u8> {
        self.render_at_scale(view, theme, cursor, 1.)
    }
    fn render_at_scale<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        theme: &cosmic::Theme,
        cursor: mouse::Cursor,
        scale: f64,
    ) -> Vec<u8> {
        assert!(scale.is_finite() && (0.5..=4.).contains(&scale));
        let mut ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        if let Some(position) = cursor.position() {
            let mut messages = Vec::new();
            ui.update(
                &[Event::Mouse(mouse::Event::CursorMoved { position })],
                cursor,
                &mut self.renderer,
                &mut clipboard::Null,
                &mut messages,
            );
        }
        ui.draw(&mut self.renderer, theme, &Style::default(), cursor);
        let pixels = self.renderer.screenshot(
            Size::new(
                (f64::from(self.size.width) * scale).round() as u32,
                (f64::from(self.size.height) * scale).round() as u32,
            ),
            scale as f32,
            cosmic::iced::Color::TRANSPARENT,
        );
        self.cache = ui.into_cache();
        pixels
    }
    pub fn capture_hover<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        theme: &cosmic::Theme,
        point: Point,
        name: &str,
    ) {
        let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pixels = self.render_hover(view, theme, point);
        image::save_buffer(
            dir.join(format!("{name}.png")),
            &pixels,
            self.size.width.round() as u32,
            self.size.height.round() as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    /// Scrolls the scrollable `id` to an absolute offset; the offset is kept
    /// in the widget cache for the next render, as after a real wheel event.
    pub fn scroll<M>(&mut self, view: cosmic::Element<'_, M>, id: &str, y: f32) {
        use cosmic::iced::advanced::widget::operation::{Outcome, scrollable};
        let mut ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut operation: Box<dyn Operation> = Box::new(scrollable::scroll_to(
            cosmic::widget::Id::new(id.to_string()),
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        ));
        loop {
            ui.operate(&self.renderer, operation.as_mut());
            match operation.finish() {
                Outcome::Chain(next) => operation = next,
                _ => break,
            }
        }
        self.cache = ui.into_cache();
    }
    pub fn capture_scaled<M>(
        &mut self,
        view: cosmic::Element<'_, M>,
        theme: &cosmic::Theme,
        scale: f32,
        name: &str,
    ) {
        let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") else {
            return;
        };
        let _ = self.render(view, theme);
        let size = Size::new(
            (self.size.width * scale) as u32,
            (self.size.height * scale) as u32,
        );
        let pixels = self
            .renderer
            .screenshot(size, scale, cosmic::iced::Color::TRANSPARENT);
        image::save_buffer(
            std::path::PathBuf::from(dir).join(format!("{name}.png")),
            &pixels,
            size.width,
            size.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    pub fn capture<M>(&mut self, view: cosmic::Element<'_, M>, theme: &cosmic::Theme, name: &str) {
        let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pixels = self.render(view, theme);
        image::save_buffer(
            dir.join(format!("{name}.png")),
            &pixels,
            self.size.width.round() as u32,
            self.size.height.round() as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    /// Named widgets that appear more than once. libcosmic reuses widget state by
    /// name, so a repeated name loses its state and panics on the next frame.
    pub fn duplicate_ids<M>(&mut self, view: cosmic::Element<'_, M>) -> Vec<String> {
        #[derive(Default)]
        struct Ids(Vec<String>, Vec<String>);
        impl Ids {
            fn add(&mut self, id: Option<&cosmic::widget::Id>) {
                if let Some(id) = id {
                    // `Id(Custom(serial, "name"))`: the serial differs per call.
                    let text = format!("{id:?}");
                    if text.contains("Custom")
                        && let Some(name) = text.split('"').nth(1)
                    {
                        self.0.push(name.to_owned());
                    }
                }
            }
        }
        impl Operation for Ids {
            fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
                f(self);
            }
            fn container(&mut self, id: Option<&cosmic::widget::Id>, _: Rectangle) {
                // Text inputs report themselves here and as focusable, once each.
                let before = self.0.len();
                self.add(id);
                let added: Vec<String> = self.0.drain(before..).collect();
                self.1.extend(added);
            }
            fn scrollable(
                &mut self,
                id: Option<&cosmic::widget::Id>,
                _: Rectangle,
                _: Rectangle,
                _: cosmic::iced::Vector,
                _: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
            ) {
                self.add(id);
            }
            fn focusable(
                &mut self,
                id: Option<&cosmic::widget::Id>,
                _: Rectangle,
                _: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
            ) {
                self.add(id);
            }
        }
        let mut ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut ids = Ids::default();
        ui.operate(&self.renderer, &mut ids);
        self.cache = ui.into_cache();
        let mut repeated = Vec::new();
        for list in [&mut ids.0, &mut ids.1] {
            list.sort();
            repeated.extend(
                list.windows(2)
                    .filter(|pair| pair[0] == pair[1])
                    .map(|pair| pair[0].clone()),
            );
        }
        repeated.dedup();
        repeated
    }
    /// Locate visible labels in the real widget tree for pointer interaction tests.
    pub fn text_bounds<M>(&mut self, view: cosmic::Element<'_, M>, text: &str) -> Rectangle {
        let found = self.text_all(view, text);
        assert_eq!(
            found.len(),
            1,
            "unique real-view text {text}: {found:?}"
        );
        found[0]
    }
    /// Every place `text` is drawn in the real widget tree.
    pub fn text_all<M>(&mut self, view: cosmic::Element<'_, M>, text: &str) -> Vec<Rectangle> {
        self.texts(view)
            .into_iter()
            .filter(|found| found.0 == text)
            .map(|found| found.1)
            .collect()
    }
    /// Text that runs past the window's right edge, outside any area that
    /// scrolls sideways.
    pub fn overflowing_text<M>(&mut self, view: cosmic::Element<'_, M>) -> Vec<(String, Rectangle)> {
        let width = self.size.width;
        self.texts(view)
            .into_iter()
            .filter(|(_, bounds, sideways)| !sideways && bounds.x + bounds.width > width + 0.5)
            .map(|(text, bounds, _)| (text, bounds))
            .collect()
    }
    /// Every text in the real widget tree with its bounds on screen, and
    /// whether it sits in an area that scrolls sideways.
    fn texts<M>(&mut self, view: cosmic::Element<'_, M>) -> Vec<(String, Rectangle, bool)> {
        #[derive(Default)]
        struct Find {
            found: Vec<(String, Rectangle, bool)>,
            offset: cosmic::iced::Vector,
            pending: cosmic::iced::Vector,
            sideways: bool,
            pending_sideways: bool,
        }
        impl Operation for Find {
            fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
                let previous = (self.offset, self.sideways);
                self.offset += std::mem::take(&mut self.pending);
                self.sideways |= std::mem::take(&mut self.pending_sideways);
                f(self);
                (self.offset, self.sideways) = previous;
            }
            fn scrollable(
                &mut self,
                _: Option<&cosmic::widget::Id>,
                bounds: Rectangle,
                content: Rectangle,
                translation: cosmic::iced::Vector,
                _: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
            ) {
                self.pending = translation;
                self.pending_sideways = content.width > bounds.width + 0.5;
            }
            fn text(&mut self, _: Option<&cosmic::widget::Id>, bounds: Rectangle, text: &str) {
                self.found
                    .push((text.to_owned(), bounds - self.offset, self.sideways));
            }
        }
        let mut ui = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut find = Find::default();
        ui.operate(&self.renderer, &mut find);
        self.cache = ui.into_cache();
        find.found
    }
}
