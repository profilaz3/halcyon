//! Two stable child trees reflow with available space; resizing never replaces
//! the editor tree, focus or scroll position. All dimensions are logical pixels.
use super::*;
use cosmic::iced::advanced::widget::{Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Event, Rectangle, Size, Vector, mouse};
use cosmic::{Renderer, Theme};

/// Keep the same two child trees while changing their proportions. A scene
/// needs room to inspect objects; a settings page needs room for its controls.
#[derive(Clone, Copy)]
pub(super) enum Pattern {
    Balanced,
    Scene,
    Workspace,
    Controls,
    Form,
}

pub(super) fn page<'a>(
    left: Element<'a, Message>,
    right: Element<'a, Message>,
    minimum: [f32; 2],
    pattern: Pattern,
) -> Element<'a, Message> {
    Element::new(Columns {
        children: vec![left, right],
        minimum,
        pattern,
        field: false,
        stage_layout: None,
    })
}

pub(super) fn columns<'a>(
    left: Element<'a, Message>,
    right: Element<'a, Message>,
    minimum: [f32; 2],
    inspector: bool,
) -> Element<'a, Message> {
    page(
        left,
        right,
        minimum,
        if inspector {
            Pattern::Scene
        } else {
            Pattern::Balanced
        },
    )
}
pub(super) fn field<'a>(
    left: Element<'a, Message>,
    right: Element<'a, Message>,
) -> Element<'a, Message> {
    Element::new(Columns {
        children: vec![left, right],
        minimum: [100., 0.],
        pattern: Pattern::Balanced,
        field: true,
        stage_layout: None,
    })
}
struct Columns<'a> {
    children: Vec<Element<'a, Message>>,
    minimum: [f32; 2],
    pattern: Pattern,
    field: bool,
    stage_layout: Option<layout::Node>,
}
#[derive(Default)]
struct State {
    viewport_origin: std::cell::Cell<Option<f32>>,
}
impl Columns<'_> {
    /// In a composition the picture stays in view while its inspector scrolls
    /// with the page, without a second scroll area. Stacked, it just scrolls.
    fn pin(&self, layout: Layout<'_>, viewport: &Rectangle) -> f32 {
        if self.field || !matches!(self.pattern, Pattern::Scene | Pattern::Workspace) {
            return 0.;
        }
        let mut children = layout.children();
        let (Some(left), Some(right)) = (children.next(), children.next()) else {
            return 0.;
        };
        if right.bounds().x <= left.bounds().x {
            return 0.;
        }
        let room = (layout.bounds().height - left.bounds().height).max(0.);
        (viewport.y - layout.bounds().y).clamp(0., room)
    }
}
impl Widget<Message, Theme, Renderer> for Columns<'_> {
    fn tag(&self) -> cosmic::iced::advanced::widget::tree::Tag {
        cosmic::iced::advanced::widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> cosmic::iced::advanced::widget::tree::State {
        cosmic::iced::advanced::widget::tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        use cosmic::iced::advanced::text::Renderer as _;
        let scale = (renderer.default_size().0 / 14.).max(1.);
        let width = limits.max().width;
        let gap = if self.field { 8. } else { 16. } * scale;
        let right = if self.field {
            self.children[1]
                .as_widget_mut()
                .layout(
                    &mut tree.children[1],
                    renderer,
                    &layout::Limits::new(Size::ZERO, limits.max()),
                )
                .size()
                .width
        } else {
            right_width(self.pattern, width, self.minimum, scale, gap)
        };
        let side = if self.field {
            width >= self.minimum[0] * scale + right + gap
        } else {
            width >= (self.minimum[0] + self.minimum[1]) * scale + gap
        };
        let widths = if side {
            [width - right - gap, right]
        } else {
            [width, width]
        };
        let mut nodes = Vec::new();
        for (i, (child, state)) in self.children.iter_mut().zip(&mut tree.children).enumerate() {
            // Keep the scroll area's compression, so a fill-height child inside
            // a scrolling page takes its natural height instead of infinity.
            let child_limits = layout::Limits::with_compression(
                Size::new(widths[i], 0.),
                Size::new(widths[i], limits.max().height),
                limits.compression(),
            );
            nodes.push(child.as_widget_mut().layout(state, renderer, &child_limits));
        }
        let height = if side {
            nodes[0].size().height.max(nodes[1].size().height)
        } else {
            nodes[0].size().height + gap + nodes[1].size().height
        };
        let offset = if side {
            // A labelled row centers both sides, so a one-line label lines up
            // with its switch or choice instead of sitting at the top.
            let middle = |node: &layout::Node| {
                if self.field {
                    ((height - node.size().height) / 2.).max(0.).round()
                } else {
                    0.
                }
            };
            let left = middle(&nodes[0]);
            nodes[0] = nodes[0].clone().translate(Vector::new(0., left));
            Vector::new(widths[0] + gap, middle(&nodes[1]))
        } else {
            Vector::new(0., nodes[0].size().height + gap)
        };
        nodes[1] = nodes[1].clone().translate(offset);
        self.stage_layout = Some(nodes[0].clone());
        layout::Node::with_children(Size::new(width, height), nodes)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let pin = tree
            .state
            .downcast_ref::<State>()
            .viewport_origin
            .get()
            .map_or(0., |origin| {
                self.pin(
                    layout,
                    &Rectangle {
                        y: origin + layout.virtual_offset().y,
                        ..Rectangle::default()
                    },
                )
            });
        let offset = layout.virtual_offset();
        let parent = layout.position();
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for (index, ((child, state), child_layout)) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .enumerate()
            {
                let child_layout = if index == 0 && pin > 0. {
                    Layout::with_offset(
                        Vector::new(parent.x, parent.y + pin),
                        self.stage_layout.as_ref().expect("layout precedes operate"),
                    )
                } else {
                    child_layout
                };
                child.as_widget_mut().operate(
                    state,
                    child_layout.with_virtual_offset(offset),
                    renderer,
                    operation,
                );
            }
        });
    }

    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        let mut trees: Vec<_> = self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| child.as_widget().a11y_nodes(layout, state, cursor))
            .collect();
        // A row's title names its one control when nothing else did.
        if self.field
            && let [title, control] = &mut trees[..]
        {
            super::focus::name_from_title(title, control);
        }
        iced_accessibility::A11yTree::join(trees.into_iter())
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        tree.state
            .downcast_ref::<State>()
            .viewport_origin
            .set(Some(viewport.y - layout.virtual_offset().y));
        let offset = layout.virtual_offset();
        let pin = self.pin(layout, viewport);
        for (index, ((child, state), layout)) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .enumerate()
        {
            let shift = if index == 0 { pin } else { 0. };
            let moved = Rectangle {
                y: viewport.y - shift,
                ..*viewport
            };
            child.as_widget_mut().update(
                state,
                event,
                layout.with_virtual_offset(offset),
                cursor - Vector::new(0., shift),
                renderer,
                clipboard,
                shell,
                &moved,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let offset = layout.virtual_offset();
        let pin = self.pin(layout, viewport);
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
            .map(|(index, ((child, state), layout))| {
                let shift = if index == 0 { pin } else { 0. };
                child.as_widget().mouse_interaction(
                    state,
                    layout.with_virtual_offset(offset),
                    cursor - Vector::new(0., shift),
                    &Rectangle {
                        y: viewport.y - shift,
                        ..*viewport
                    },
                    renderer,
                )
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use cosmic::iced::advanced::Renderer as _;
        tree.state
            .downcast_ref::<State>()
            .viewport_origin
            .set(Some(viewport.y - layout.virtual_offset().y));
        let offset = layout.virtual_offset();
        let pin = self.pin(layout, viewport);
        for (index, ((child, state), layout)) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
        {
            if index == 0 && pin > 0. {
                let moved = Rectangle {
                    y: viewport.y - pin,
                    ..*viewport
                };
                renderer.with_translation(Vector::new(0., pin), |renderer| {
                    child.as_widget().draw(
                        state,
                        renderer,
                        theme,
                        style,
                        layout.with_virtual_offset(offset),
                        cursor - Vector::new(0., pin),
                        &moved,
                    );
                });
            } else {
                child.as_widget().draw(
                    state,
                    renderer,
                    theme,
                    style,
                    layout.with_virtual_offset(offset),
                    cursor,
                    viewport,
                );
            }
        }
    }
    fn drag_destinations(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        for ((child, child_layout), state) in self
            .children
            .iter()
            .zip(layout.children())
            .zip(state.children.iter())
        {
            child.as_widget().drag_destinations(
                state,
                child_layout.with_virtual_offset(layout.virtual_offset()),
                renderer,
                dnd_rectangles,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let pin = self.pin(layout, viewport);
        let overlays: Vec<_> = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .enumerate()
            .filter_map(|(index, ((child, state), layout))| {
                let shift = if index == 0 { pin } else { 0. };
                child.as_widget_mut().overlay(
                    state,
                    layout,
                    renderer,
                    viewport,
                    translation + Vector::new(0., shift),
                )
            })
            .collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

fn right_width(pattern: Pattern, width: f32, minimum: [f32; 2], scale: f32, gap: f32) -> f32 {
    match pattern {
        Pattern::Balanced => (width - gap) / 2.,
        Pattern::Workspace => {
            (width * 0.28).clamp(minimum[1] * scale, 288_f32.max(minimum[1]) * scale)
        }
        Pattern::Scene => (width * 0.40).clamp(minimum[1] * scale, 420_f32.max(minimum[1]) * scale),
        Pattern::Controls => {
            width - gap - (width * 0.34).clamp(minimum[0] * scale, 340_f32.max(minimum[0]) * scale)
        }
        Pattern::Form => (width * 0.32).clamp(minimum[1] * scale, 340_f32.max(minimum[1]) * scale),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ux_native_pinned_scene_uses_visible_bounds_for_pointer_and_keyboard() {
        use super::*;
        let view = || -> Element<'static, Message> {
            column::with_children(vec![
                cosmic::widget::Space::new().height(70).into(),
                scrollable(column::with_children(vec![
                    cosmic::widget::Space::new().height(90).into(),
                    page(
                        button::standard("Edit scene")
                            .id("pin-hit".into())
                            .on_press(Message::ToggleHelp("pin-hit"))
                            .height(120)
                            .width(Length::Fill)
                            .into(),
                        container(label("Long inspector", 14., MUTED))
                            .height(1200)
                            .width(Length::Fill)
                            .into(),
                        [320., 248.],
                        Pattern::Scene,
                    ),
                ]))
                .id("settings-content")
                .height(Length::Fill)
                .into(),
            ])
            .into()
        };
        let mut native = crate::native_test::Native::new(Size::new(820., 600.));
        let _ = native.render(view(), &Theme::dark());
        native.scroll(view(), "settings-content", 300.);
        // Operations see the new scroll offset even before the next draw.
        let bounds = native.bounds(view(), "pin-hit");
        assert!((bounds.y - 70.).abs() < 1., "{bounds:?}");
        let _ = native.render(view(), &Theme::dark());
        native.operate(
            view(),
            Box::new(super::super::super::navigation::Reveal::target("pin-hit")),
        );
        assert_eq!(native.bounds(view(), "pin-hit"), bounds);
        let mut actions = Vec::new();
        for event in [
            mouse::Event::CursorMoved {
                position: bounds.center(),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            actions.extend(native.event(view(), Event::Mouse(event), bounds.center()).0);
        }
        assert!(
            actions
                .iter()
                .any(|message| matches!(message, Message::ToggleHelp("pin-hit")))
        );
        // A narrow layout scrolls normally instead of pinning a stacked scene.
        native.size.width = 480.;
        let _ = native.render(view(), &Theme::dark());
        assert!(native.bounds(view(), "pin-hit").y < 70.);
    }

    #[test]
    fn polish050_control_pattern_prioritizes_controls_and_reflows_for_large_text() {
        use super::*;
        let view = || {
            page(
                container(label("Sample", 14., FG))
                    .id("polish-stage")
                    .width(Length::Fill)
                    .height(150)
                    .into(),
                container(
                    text_input("Title", "Unsaved")
                        .id("polish-field".into())
                        .on_input(|_| Message::Preview),
                )
                .id("polish-controls")
                .width(Length::Fill)
                .into(),
                [224., 360.],
                Pattern::Controls,
            )
        };
        for width in [616., 1056.] {
            let mut native = crate::native_test::Native::new(Size::new(width, 600.));
            let stage = native.bounds(view(), "polish-stage");
            let controls = native.bounds(view(), "polish-controls");
            assert!(controls.width >= 360. && controls.width > stage.width);
            assert!(controls.x + controls.width <= width);
        }
        let mut native =
            crate::native_test::Native::with_text_size(Size::new(616., 600.), "tiny-skia", 21.);
        let stage = native.bounds(view(), "polish-stage");
        let controls = native.bounds(view(), "polish-controls");
        assert!(controls.y >= stage.y + stage.height && controls.width == 616.);
        native.capture(
            view(),
            &cosmic::Theme::dark(),
            "polish050-control-large-text",
        );
    }
    use super::*;
    #[test]
    fn structural_columns_reflow_for_enlarged_text_and_long_controls() {
        use cosmic::iced::advanced::renderer::Headless;
        for (font_size, stacked) in [(14., false), (22., true)] {
            let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
                cosmic::font::default(),
                cosmic::iced::Pixels(font_size),
                Some("tiny-skia"),
            ))
            .unwrap();
            let mut content = view();
            let mut tree = Tree::new(&content);
            let layout = content.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(616., 1000.)),
            );
            let left = layout.children()[0].bounds();
            let right = layout.children()[1].bounds();
            assert_eq!(right.y >= left.y + left.height, stacked);
            assert!(right.x + right.width <= 616.);
        }
        let mut native = crate::native_test::Native::new(Size::new(320., 600.));
        let content = || {
            field(
                container(label("Location", 14., FG))
                    .id("field-label")
                    .into(),
                button::standard("Move the selected widgets to this display")
                    .id("field-action".into())
                    .on_press(Message::ToggleHelp("test"))
                    .into(),
            )
        };
        let label = native.bounds(content(), "field-label");
        let action = native.bounds(content(), "field-action");
        assert!(action.y >= label.y + label.height);
        assert!(action.x + action.width <= 320.);
    }
    fn view() -> Element<'static, Message> {
        columns(
            container(label("Preview", 14., FG))
                .id("stage")
                .width(Length::Fill)
                .height(180)
                .into(),
            container(
                text_input("Name", "Unsaved name")
                    .id("draft-name".into())
                    .on_input(|_| Message::ToggleHelp("test")),
            )
            .id("inspector")
            .width(Length::Fill)
            .into(),
            [320., 248.],
            true,
        )
    }
    #[test]
    fn structural_columns_reflow_without_replacing_focused_draft() {
        let mut native = crate::native_test::Native::new(Size::new(616., 450.));
        let stage = native.bounds(view(), "stage");
        let controls = native.bounds(view(), "inspector");
        assert!(controls.x >= stage.x + stage.width + 15. && controls.width >= 248.);
        native.operate(
            view(),
            Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
                cosmic::widget::Id::new("draft-name"),
            )),
        );
        native.size = Size::new(480., 450.);
        let stage = native.bounds(view(), "stage");
        let controls = native.bounds(view(), "inspector");
        assert!(controls.y >= stage.y + stage.height + 15.);
        // The same focused text input survives the orientation change.
        let (_, status) = native.event(
            view(),
            cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key: cosmic::iced::keyboard::Key::Character("x".into()),
                modified_key: cosmic::iced::keyboard::Key::Character("x".into()),
                physical_key: cosmic::iced::keyboard::key::Physical::Code(
                    cosmic::iced::keyboard::key::Code::KeyX,
                ),
                location: cosmic::iced::keyboard::Location::Standard,
                modifiers: cosmic::iced::keyboard::Modifiers::empty(),
                text: Some("x".into()),
                repeat: false,
            }),
            cosmic::iced::Point::ORIGIN,
        );
        assert_eq!(status, cosmic::iced::event::Status::Captured);
    }
}
