//! Keyboard focus for the control on the right of a settings row.
//!
//! COSMIC's switch, dropdown and the native slider take no keyboard focus,
//! so Tab skipped them. This wrapper takes focus for the row when its
//! control has none of its own: Space or Enter flips a switch or opens a
//! dropdown, the arrow keys step a slider, and a ring shows where focus is.
//! Keys reach the control as the pointer events it already understands, at
//! the control's centre, so nothing in libcosmic or iced changes.
//!
//! It also names the control for screen readers with the row's title.
use super::Message;
use cosmic::iced::advanced::widget::{Operation, Tree, operation, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Background, Border, Color, Event, Length, Point, Rectangle, Size, Vector};
use cosmic::iced::{keyboard, mouse};
use cosmic::{Element, Renderer, Theme};
use iced_accessibility::A11yTree;
use iced_accessibility::accesskit::{Node, Rect, Role, Toggled};
use std::cell::Cell;

/// Wraps a row's control. `name` is what a screen reader calls it;
/// `checked` is a switch's state when the caller knows it.
pub(in crate::settings) fn control<'a>(
    name: Option<String>,
    checked: Option<bool>,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    Element::new(Control {
        name,
        checked,
        content,
    })
}

struct Control<'a> {
    name: Option<String>,
    checked: Option<bool>,
    content: Element<'a, Message>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Pressed to act: a switch.
    Press,
    /// Opens a list on press.
    Dropdown,
    /// Stepped with the arrow keys.
    Slider,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Target {
    kind: Kind,
    bounds: Rectangle,
}

struct State {
    focused: bool,
    /// The control's list is open.
    open: bool,
    /// The option highlighted from the keyboard in an open list.
    option: Option<usize>,
    /// The target found at layout, relative to the control, for the
    /// accessibility tree, which is built without a renderer.
    target: Cell<Option<Target>>,
    id: cosmic::widget::Id,
}
impl Default for State {
    fn default() -> Self {
        Self {
            focused: false,
            open: false,
            option: None,
            target: Cell::new(None),
            id: cosmic::widget::Id::unique(),
        }
    }
}
impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
        self.option = None;
    }
}

/// Counts the focusable widgets inside the control.
struct Count(usize);
impl Operation for Count {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(
        &mut self,
        _id: Option<&cosmic::widget::Id>,
        _bounds: Rectangle,
        _state: &mut dyn operation::Focusable,
    ) {
        self.0 += 1;
    }
}

fn leaves(layout: Layout<'_>, out: &mut Vec<Rectangle>) {
    let mut children = layout.children().peekable();
    // A part with no area (an empty label) can't be pointed at or seen.
    if children.peek().is_none() && layout.bounds().width >= 1. && layout.bounds().height >= 1. {
        out.push(layout.bounds());
    }
    for child in children {
        leaves(child, out);
    }
}

fn has_dropdown(tree: &Tree) -> bool {
    tree.tag == tree::Tag::of::<cosmic::widget::dropdown::State>()
        || tree.children.iter().any(has_dropdown)
}

/// Whether a key press carries Ctrl, Alt or Super, which belong to
/// shortcuts rather than to the control.
fn chorded(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. })
            if modifiers.control() || modifiers.alt() || modifiers.logo()
    )
}

/// The keys the wrapper answers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    /// Space or Enter.
    Activate,
    Escape,
    Up,
    Down,
    Left,
    Right,
}

fn key(event: &Event) -> Option<Key> {
    use keyboard::key::Named;
    let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event else {
        return None;
    };
    match key.as_ref() {
        keyboard::Key::Named(Named::Enter) | keyboard::Key::Character(" ") => Some(Key::Activate),
        keyboard::Key::Named(Named::Escape) => Some(Key::Escape),
        keyboard::Key::Named(Named::ArrowUp) => Some(Key::Up),
        keyboard::Key::Named(Named::ArrowDown) => Some(Key::Down),
        keyboard::Key::Named(Named::ArrowLeft) => Some(Key::Left),
        keyboard::Key::Named(Named::ArrowRight) => Some(Key::Right),
        _ => None,
    }
}

fn press(button: bool) -> Event {
    Event::Mouse(if button {
        mouse::Event::ButtonPressed(mouse::Button::Left)
    } else {
        mouse::Event::ButtonReleased(mouse::Button::Left)
    })
}

impl Control<'_> {
    /// The control's first leaf that reacts to the pointer.
    fn target(&self, tree: &Tree, layout: Layout<'_>, renderer: &Renderer) -> Option<Target> {
        let mut found = Vec::new();
        leaves(layout, &mut found);
        let dropdown = has_dropdown(&tree.children[0]);
        found.into_iter().find_map(|bounds| {
            let interaction = self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                mouse::Cursor::Available(bounds.center()),
                &layout.bounds(),
                renderer,
            );
            let kind = match interaction {
                mouse::Interaction::Grab | mouse::Interaction::Grabbing => Kind::Slider,
                mouse::Interaction::Pointer if dropdown => Kind::Dropdown,
                mouse::Interaction::Pointer => Kind::Press,
                _ => return None,
            };
            Some(Target { kind, bounds })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn send(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        at: Point,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            mouse::Cursor::Available(at),
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
}

impl Widget<Message, Theme, Renderer> for Control<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let target = self.target(tree, Layout::new(&node), renderer);
        tree.state.downcast_ref::<State>().target.set(target);
        node
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut count = Count(0);
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut count);
        let target = self.target(tree, layout, renderer);
        let state = tree.state.downcast_mut::<State>();
        // A control with focusable parts of its own keeps them; adding
        // the row as well would make Tab stop twice.
        if count.0 == 0 {
            if let Some(target) = target {
                operation.focusable(None, target.bounds, state);
            } else {
                state.focused = false;
            }
        }
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        use keyboard::key::Named;
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event {
            operation::Focusable::unfocus(tree.state.downcast_mut::<State>());
        }
        let (focused, open) = {
            let state = tree.state.downcast_ref::<State>();
            (state.focused, state.open)
        };
        let key = key(event).filter(|_| focused && !chorded(event));
        let target = key.and_then(|_| self.target(tree, layout, renderer));
        if let (Some(key), Some(target)) = (key, target) {
            let at = target.bounds.center();
            match (target.kind, key) {
                (Kind::Press | Kind::Dropdown, Key::Activate) if !open => {
                    for down in [true, false] {
                        self.send(
                            tree,
                            &press(down),
                            layout,
                            at,
                            renderer,
                            clipboard,
                            shell,
                            viewport,
                        );
                    }
                    shell.capture_event();
                    return;
                }
                // The list itself takes keys first; an Escape it leaves
                // closes it the way a click outside does.
                (Kind::Dropdown, Key::Escape) if open => {
                    let outside = Point::new(at.x, target.bounds.y - 1.);
                    self.send(
                        tree,
                        &press(true),
                        layout,
                        outside,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    );
                    self.send(
                        tree,
                        &press(false),
                        layout,
                        outside,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    );
                    tree.state.downcast_mut::<State>().option = None;
                    shell.capture_event();
                    return;
                }
                (Kind::Slider, Key::Left | Key::Right | Key::Up | Key::Down) => {
                    let step = if matches!(key, Key::Left | Key::Down) {
                        Named::ArrowDown
                    } else {
                        Named::ArrowUp
                    };
                    // Keeps the physical key, so a completion waiting for
                    // its release still sees it.
                    let mut stepped = event.clone();
                    if let Event::Keyboard(keyboard::Event::KeyPressed {
                        key, modified_key, ..
                    }) = &mut stepped
                    {
                        *key = keyboard::Key::Named(step);
                        *modified_key = keyboard::Key::Named(step);
                    }
                    self.send(
                        tree, &stepped, layout, at, renderer, clipboard, shell, viewport,
                    );
                    shell.capture_event();
                    return;
                }
                _ => {}
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let state = tree.state.downcast_ref::<State>();
        if !state.focused {
            return;
        }
        let Some(target) = self.target(tree, layout, renderer) else {
            return;
        };
        use cosmic::iced::advanced::Renderer as _;
        let accent: Color = theme.cosmic().accent.base.into();
        let bounds = target.bounds.expand(3.);
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    color: accent,
                    width: 2.,
                    radius: (bounds.height / 2.).min(10.).into(),
                },
                ..Default::default()
            },
            Background::Color(Color::TRANSPARENT),
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn drag_destinations(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        self.content
            .as_widget()
            .drag_destinations(&tree.children[0], layout, renderer, rectangles);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        let inner = self.content.as_widget_mut().overlay(
            &mut children[0],
            layout,
            renderer,
            viewport,
            translation,
        );
        state.open = inner.is_some();
        if !state.open {
            state.option = None;
        }
        let inner = inner?;
        if !state.focused {
            return Some(inner);
        }
        Some(overlay::Element::new(Box::new(Keys { inner, state })))
    }
    fn a11y_nodes(&self, layout: Layout<'_>, tree: &Tree, cursor: mouse::Cursor) -> A11yTree {
        let mut child = self
            .content
            .as_widget()
            .a11y_nodes(layout, &tree.children[0], cursor);
        // The row's title names its control. A row of several controls
        // (buttons with their own words) keeps each one's own name.
        let found = child
            .root()
            .iter()
            .chain(child.children())
            .filter(|node| interactive(node.node()))
            .count();
        let named = found > 0;
        if found == 1
            && let Some(name) = &self.name
        {
            let label = |node: &mut iced_accessibility::A11yNode| {
                if interactive(node.node()) && node.node().label().is_none() {
                    node.node_mut().set_label(name.clone());
                }
            };
            child.root_mut().iter_mut().for_each(label);
            child.children_mut().iter_mut().for_each(label);
        }
        let state = tree.state.downcast_ref::<State>();
        let Some(target) = state.target.get().filter(|_| !named) else {
            return child;
        };
        let mut node = Node::new(match target.kind {
            Kind::Dropdown => Role::ComboBox,
            Kind::Slider => Role::Slider,
            Kind::Press => Role::Switch,
        });
        let b = target.bounds + (layout.position() - Point::ORIGIN);
        node.set_bounds(Rect::new(
            b.x as f64,
            b.y as f64,
            (b.x + b.width) as f64,
            (b.y + b.height) as f64,
        ));
        if let Some(name) = &self.name {
            node.set_label(name.clone());
        }
        if target.kind == Kind::Press
            && let Some(checked) = self.checked
        {
            node.set_toggled(Toggled::from(checked));
        }
        A11yTree::join([A11yTree::leaf(node, state.id.clone()), child].into_iter())
    }
}

/// Whether a screen reader treats `node` as something to operate.
fn interactive(node: &Node) -> bool {
    matches!(
        node.role(),
        Role::Slider
            | Role::Button
            | Role::CheckBox
            | Role::Switch
            | Role::ComboBox
            | Role::TextInput
            | Role::SpinButton
            | Role::RadioButton
    )
}

/// Names the one unnamed control in `control` after the first words in
/// `title`, as a sighted person reads the row.
pub(super) fn name_from_title(title: &A11yTree, control: &mut A11yTree) {
    let nodes = |tree: &A11yTree| -> Vec<Node> {
        tree.root()
            .iter()
            .chain(tree.children())
            .map(|node| node.node().clone())
            .collect()
    };
    let controls: Vec<_> = nodes(control).into_iter().filter(interactive).collect();
    let [only] = &controls[..] else {
        return;
    };
    if only.label().is_some_and(|label| !label.is_empty()) {
        return;
    }
    let Some(name) = nodes(title)
        .iter()
        .filter(|node| !interactive(node))
        .find_map(|node| {
            node.label()
                .filter(|label| !label.trim().is_empty())
                .map(str::to_owned)
        })
    else {
        return;
    };
    let label = |node: &mut iced_accessibility::A11yNode| {
        if interactive(node.node()) {
            node.node_mut().set_label(name.clone());
        }
    };
    control.root_mut().iter_mut().for_each(label);
    control.children_mut().iter_mut().for_each(label);
}

/// An open dropdown list driven from the keyboard: Up and Down move the
/// highlight, Enter or Space picks it.
struct Keys<'b> {
    inner: overlay::Element<'b, Message, Theme, Renderer>,
    state: &'b mut State,
}

/// The height of one option in a COSMIC dropdown list at its default text
/// size and padding: line height 1.4 × 14 px plus 8 px above and below.
const OPTION_HEIGHT: f32 = 1.4 * 14. + 16.;

/// The list inside the menu: the innermost first child of its layout.
fn list(layout: Layout<'_>) -> Layout<'_> {
    match layout.children().next() {
        Some(child) => list(child),
        None => layout,
    }
}

impl overlay::Overlay<Message, Theme, Renderer> for Keys<'_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.inner.as_overlay_mut().layout(renderer, bounds)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.inner
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.inner
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let key = key(event).filter(|_| !chorded(event));
        let bounds = list(layout).bounds();
        let count = (bounds.height / OPTION_HEIGHT).round().max(1.) as usize;
        let at = |option: usize| {
            Point::new(
                bounds.center_x(),
                bounds.y + OPTION_HEIGHT * (option as f32 + 0.5),
            )
        };
        match key {
            Some(Key::Down | Key::Up) => {
                let next = match (self.state.option, key) {
                    (None, Some(Key::Down)) => 0,
                    (None, _) => count - 1,
                    (Some(option), Some(Key::Down)) => (option + 1).min(count - 1),
                    (Some(option), _) => option.saturating_sub(1),
                };
                self.state.option = Some(next);
                let point = at(next);
                self.inner.as_overlay_mut().update(
                    &Event::Mouse(mouse::Event::CursorMoved { position: point }),
                    layout,
                    mouse::Cursor::Available(point),
                    renderer,
                    clipboard,
                    shell,
                );
                shell.capture_event();
            }
            Some(Key::Activate) => {
                if let Some(option) = self.state.option.take() {
                    let point = at(option);
                    let cursor = mouse::Cursor::Available(point);
                    for event in [press(true), press(false)] {
                        self.inner
                            .as_overlay_mut()
                            .update(&event, layout, cursor, renderer, clipboard, shell);
                    }
                }
                shell.capture_event();
            }
            _ => self
                .inner
                .as_overlay_mut()
                .update(event, layout, cursor, renderer, clipboard, shell),
        }
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.inner
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.inner.as_overlay_mut().overlay(layout, renderer)
    }
    fn index(&self) -> f32 {
        self.inner.as_overlay().index()
    }
}

/// COSMIC's switch, taking keyboard focus wherever it is placed.
pub(in crate::settings) struct Toggler<'a> {
    toggler: cosmic::widget::Toggler<'a, Message>,
    value: bool,
}
pub(in crate::settings) fn toggler<'a>(value: bool) -> Toggler<'a> {
    Toggler {
        toggler: cosmic::widget::toggler(value),
        value,
    }
}
impl<'a> Toggler<'a> {
    pub(in crate::settings) fn on_toggle(self, message: impl Fn(bool) -> Message + 'a) -> Self {
        Self {
            toggler: self.toggler.on_toggle(message),
            ..self
        }
    }
    pub(in crate::settings) fn on_toggle_maybe(
        self,
        message: Option<impl Fn(bool) -> Message + 'a>,
    ) -> Self {
        Self {
            toggler: self.toggler.on_toggle_maybe(message),
            ..self
        }
    }
    pub(in crate::settings) fn width(self, width: impl Into<Length>) -> Self {
        Self {
            toggler: self.toggler.width(width),
            ..self
        }
    }
}
impl<'a> From<Toggler<'a>> for Element<'a, Message> {
    fn from(toggler: Toggler<'a>) -> Self {
        control(None, Some(toggler.value), toggler.toggler.into())
    }
}

/// The native slider, stepped with the arrow keys once it has focus.
pub(in crate::settings) struct Slider<'a, T>(cosmic::iced::widget::Slider<'a, T, Message, Theme>);
pub(in crate::settings) fn slider<'a, T>(
    range: std::ops::RangeInclusive<T>,
    value: T,
    on_change: impl Fn(T) -> Message + 'a,
) -> Slider<'a, T>
where
    T: Copy + From<u8> + PartialOrd,
{
    Slider(cosmic::widget::slider(range, value, on_change))
}
impl<'a, T> Slider<'a, T>
where
    T: Copy + From<u8> + PartialOrd,
{
    pub(in crate::settings) fn on_release(self, message: Message) -> Self {
        Self(self.0.on_release(message))
    }
    pub(in crate::settings) fn step(self, step: impl Into<T>) -> Self {
        Self(self.0.step(step))
    }
    pub(in crate::settings) fn width(self, width: impl Into<Length>) -> Self {
        Self(self.0.width(width))
    }
    /// What a screen reader calls it, when no row title names it.
    pub(in crate::settings) fn name(self, name: impl Into<String>) -> Self {
        Self(self.0.name(name.into()))
    }
}
impl<'a, T> From<Slider<'a, T>> for Element<'a, Message>
where
    cosmic::iced::widget::Slider<'a, T, Message, Theme>: Into<Element<'a, Message>>,
{
    fn from(slider: Slider<'a, T>) -> Self {
        control(None, None, slider.0.into())
    }
}
