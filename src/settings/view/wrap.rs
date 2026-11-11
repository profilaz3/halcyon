//! `row::with_children(..)` for Settings' pages. It builds the same row as
//! iced's, and `.wrap()` gives the same wrapping layout, but the wrapped row
//! also tells screen readers about what it holds: iced's wrapping row
//! reports nothing, so every title, button and switch inside one vanished.
use cosmic::iced::advanced::widget::{Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{
    Alignment, Event, Length, Padding, Pixels, Rectangle, Size, Vector, alignment, mouse,
};
use cosmic::{Element, Renderer, Theme};

pub(in crate::settings) mod row {
    use super::*;
    pub(in crate::settings) fn with_children<'a, M: 'a>(
        children: impl IntoIterator<Item = Element<'a, M>>,
    ) -> Row<'a, M> {
        children.into_iter().fold(
            Row {
                children: Vec::new(),
                spacing: 0.,
                padding: Padding::ZERO,
                width: Length::Shrink,
                height: Length::Shrink,
                align: alignment::Vertical::Top,
                clip: false,
            },
            Row::push,
        )
    }
}

pub(in crate::settings) struct Row<'a, M> {
    children: Vec<Element<'a, M>>,
    spacing: f32,
    padding: Padding,
    width: Length,
    height: Length,
    align: alignment::Vertical,
    clip: bool,
}

impl<'a, M: 'a> Row<'a, M> {
    pub(in crate::settings) fn spacing(mut self, amount: impl Into<Pixels>) -> Self {
        self.spacing = amount.into().0;
        self
    }
    pub(in crate::settings) fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }
    pub(in crate::settings) fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
    pub(in crate::settings) fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
    pub(in crate::settings) fn align_y(mut self, align: impl Into<alignment::Vertical>) -> Self {
        self.align = align.into();
        self
    }
    #[allow(dead_code)]
    pub(in crate::settings) fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }
    /// Like iced's: an empty child is left out, and a filling one makes the
    /// row fill.
    pub(in crate::settings) fn push(mut self, child: impl Into<Element<'a, M>>) -> Self {
        let child = child.into();
        let size = child.as_widget().size_hint();
        if !size.is_void() {
            self.width = self.width.enclose(size.width);
            self.height = self.height.enclose(size.height);
            self.children.push(child);
        }
        self
    }
    #[allow(dead_code)]
    pub(in crate::settings) fn extend(
        self,
        children: impl IntoIterator<Item = Element<'a, M>>,
    ) -> Self {
        children.into_iter().fold(self, Self::push)
    }
    /// Lines up the children and starts a new line when one doesn't fit.
    pub(in crate::settings) fn wrap(self) -> Wrap<'a, M> {
        Wrap {
            row: self,
            vertical_spacing: None,
            align_x: alignment::Horizontal::Left,
        }
    }
}

impl<'a, M: 'a> From<Row<'a, M>> for Element<'a, M> {
    fn from(row: Row<'a, M>) -> Self {
        cosmic::iced::widget::Row::with_children(row.children)
            .spacing(row.spacing)
            .padding(row.padding)
            .width(row.width)
            .height(row.height)
            .align_y(row.align)
            .clip(row.clip)
            .into()
    }
}

pub(in crate::settings) struct Wrap<'a, M> {
    row: Row<'a, M>,
    vertical_spacing: Option<f32>,
    align_x: alignment::Horizontal,
}

impl<M> Wrap<'_, M> {
    /// The space between lines.
    pub(in crate::settings) fn vertical_spacing(mut self, amount: impl Into<Pixels>) -> Self {
        self.vertical_spacing = Some(amount.into().0);
        self
    }
    #[allow(dead_code)]
    pub(in crate::settings) fn align_x(mut self, align: impl Into<alignment::Horizontal>) -> Self {
        self.align_x = align.into();
        self
    }
}

impl<'a, M: 'a> From<Wrap<'a, M>> for Element<'a, M> {
    fn from(wrap: Wrap<'a, M>) -> Self {
        Element::new(wrap)
    }
}

impl<M> Widget<M, Theme, Renderer> for Wrap<'_, M> {
    fn children(&self) -> Vec<Tree> {
        self.row.children.iter().map(Tree::new).collect()
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.row.children);
    }
    fn size(&self) -> Size<Length> {
        Size::new(self.row.width, self.row.height)
    }
    /// iced's wrapping row layout, unchanged.
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let row = &mut self.row;
        let limits = limits
            .width(row.width)
            .height(row.height)
            .shrink(row.padding);
        let child_limits = limits.loose();
        let spacing = row.spacing;
        let vertical_spacing = self.vertical_spacing.unwrap_or(spacing);
        let max_width = limits.max().width;

        let mut children: Vec<layout::Node> = Vec::new();
        let mut intrinsic_size = Size::ZERO;
        let mut row_start = 0;
        let mut row_height = 0.0;
        let mut x = 0.0;
        let mut y = 0.0;

        let align_factor = match Alignment::from(row.align) {
            Alignment::Start => 0.0,
            Alignment::Center => 2.0,
            Alignment::End => 1.0,
        };
        let align_y =
            |range: std::ops::Range<usize>, row_height: f32, children: &mut Vec<layout::Node>| {
                if align_factor != 0.0 {
                    for node in &mut children[range] {
                        let height = node.size().height;
                        node.translate_mut(Vector::new(0.0, (row_height - height) / align_factor));
                    }
                }
            };

        for (i, child) in row.children.iter_mut().enumerate() {
            let node = child
                .as_widget_mut()
                .layout(&mut tree.children[i], renderer, &child_limits);
            let child_size = node.size();
            if x != 0.0 && x + child_size.width > max_width {
                intrinsic_size.width = intrinsic_size.width.max(x - spacing);
                align_y(row_start..i, row_height, &mut children);
                y += row_height + vertical_spacing;
                x = 0.0;
                row_start = i;
                row_height = 0.0;
            }
            row_height = f32::max(row_height, child_size.height);
            children.push(node.move_to((x + row.padding.left, y + row.padding.top)));
            x += child_size.width + spacing;
        }
        if x != 0.0 {
            intrinsic_size.width = intrinsic_size.width.max(x - spacing);
        }
        intrinsic_size.height = y + row_height;
        align_y(row_start..children.len(), row_height, &mut children);

        let align_factor = match self.align_x {
            alignment::Horizontal::Left => 0.0,
            alignment::Horizontal::Center => 2.0,
            alignment::Horizontal::Right => 1.0,
        };
        if align_factor != 0.0 {
            let total_width = intrinsic_size.width;
            let mut row_start = 0;
            for i in 0..children.len() {
                let bounds = children[i].bounds();
                let row_width = bounds.x + bounds.width;
                let next_x = children
                    .get(i + 1)
                    .map(|node| node.bounds().x)
                    .unwrap_or_default();
                if next_x == 0.0 {
                    let translation = Vector::new((total_width - row_width) / align_factor, 0.0);
                    for node in &mut children[row_start..=i] {
                        node.translate_mut(translation);
                    }
                    row_start = i + 1;
                }
            }
        }
        let size = limits.resolve(row.width, row.height, intrinsic_size);
        layout::Node::with_children(size.expand(row.padding), children)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), c_layout) in self
                .row
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child.as_widget_mut().operate(
                    state,
                    c_layout.with_virtual_offset(layout.virtual_offset()),
                    renderer,
                    operation,
                );
            }
        });
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        for ((child, state), c_layout) in self
            .row
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state,
                event,
                c_layout.with_virtual_offset(layout.virtual_offset()),
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
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
        self.row
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), c_layout)| {
                child.as_widget().mouse_interaction(
                    state,
                    c_layout.with_virtual_offset(layout.virtual_offset()),
                    cursor,
                    viewport,
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
        let Some(clipped) = layout.bounds().intersection(viewport) else {
            return;
        };
        let viewport = if self.row.clip { &clipped } else { viewport };
        for ((child, state), c_layout) in self
            .row
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            child.as_widget().draw(
                state,
                renderer,
                theme,
                style,
                c_layout.with_virtual_offset(layout.virtual_offset()),
                cursor,
                viewport,
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
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        overlay::from_children(
            &mut self.row.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        iced_accessibility::A11yTree::join(
            self.row
                .children
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
                .map(|((child, state), c_layout)| {
                    child.as_widget().a11y_nodes(
                        c_layout.with_virtual_offset(layout.virtual_offset()),
                        state,
                        cursor,
                    )
                }),
        )
    }
    fn drag_destinations(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        for ((child, state), c_layout) in self
            .row
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child.as_widget().drag_destinations(
                state,
                c_layout.with_virtual_offset(layout.virtual_offset()),
                renderer,
                rectangles,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_test::Native;

    /// The wrapped row places its children where iced's does.
    #[test]
    fn wraps_like_iced() {
        fn chips<'a>() -> Vec<Element<'a, ()>> {
            (0..7)
                .map(|i| {
                    cosmic::widget::container(cosmic::widget::text(format!("chip number {i}")))
                        .id(cosmic::widget::Id::new(format!("chip-{i}")))
                        .padding(6)
                        .into()
                })
                .collect()
        }
        let ours = || -> Element<'static, ()> {
            row::with_children(chips())
                .spacing(8)
                .padding(4)
                .align_y(Alignment::Center)
                .wrap()
                .vertical_spacing(5)
                .into()
        };
        let iced = || -> Element<'static, ()> {
            cosmic::iced::widget::Row::with_children(chips())
                .spacing(8)
                .padding(4)
                .align_y(Alignment::Center)
                .wrap()
                .vertical_spacing(5)
                .into()
        };
        let mut native = Native::new(Size::new(300., 400.));
        let mut lines = std::collections::BTreeSet::new();
        for i in 0..7 {
            let id = format!("chip-{i}");
            let a = native.bounds(ours(), &id);
            assert_eq!(a, native.bounds(iced(), &id), "{id}");
            lines.insert(a.y as i32);
        }
        assert!(lines.len() > 1, "it wraps at 300 px");
    }
}
