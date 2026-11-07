//! One template for every widget page: the name with its on/off switch, a
//! large preview on your wallpaper, the widget's own settings in sections,
//! then one short list for placement and shared extras.
use super::*;

impl Settings {
    pub(super) fn widget_template<'a>(
        &'a self,
        widget: Widget,
        title: &'static str,
        preview: Option<Element<'a, Message>>,
        sections: Vec<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let mut parts = vec![self.widget_heading(widget, title)];
        if self.canvas_visible() {
            parts.push(self.canvas_body());
        } else if let Some(preview) = preview {
            // Every widget is shown the same way: centered, at card size.
            let preview = container(preview).max_width(420.).into();
            parts.push(
                container(self.preview_stage(preview))
                    .id(cosmic::widget::Id::new("desktop-scene"))
                    .width(Length::Fill)
                    .into(),
            );
        }
        parts.extend(sections);
        parts.push(self.widget_extras(widget));
        self.search_target(
            "widget-design",
            column::with_children(parts).spacing(20).into(),
        )
    }

    /// A row that opens something else: title, description and a chevron.
    pub(super) fn link_row<'a>(
        &self,
        title: impl Into<String>,
        description: impl Into<String>,
        message: Message,
        id: String,
    ) -> Element<'a, Message> {
        self.link(detail(title, description), message, id)
    }

    /// A [`link_row`](Self::link_row) whose line is live state, always shown.
    pub(super) fn link_row_state<'a>(
        &self,
        title: impl Into<String>,
        state: impl Into<String>,
        message: Message,
        id: String,
    ) -> Element<'a, Message> {
        self.link(detail_state(title, state), message, id)
    }

    fn link<'a>(
        &self,
        text: Element<'a, Message>,
        message: Message,
        id: String,
    ) -> Element<'a, Message> {
        button::custom(
            row::with_children(vec![
                text,
                icon::from_name("go-next-symbolic").size(16).icon().into(),
            ])
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .id(cosmic::widget::Id::new(id))
        .class(nav_style(false, self.settings_accent()))
        .padding([4, 0])
        .width(Length::Fill)
        .on_press(message)
        .into()
    }

    /// The one way every page points to COSMIC Settings for the rest.
    pub(super) fn cosmic_link<'a>(&self, page: &'static str) -> Element<'a, Message> {
        button::custom(
            row::with_children(vec![
                detail("More in COSMIC Settings", "Options this app doesn’t cover"),
                label("↗", 16., MUTED),
            ])
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .id(cosmic::widget::Id::new(format!("{page}-cosmic-settings")))
        .class(nav_style(false, self.settings_accent()))
        .padding([4, 0])
        .width(Length::Fill)
        .on_press(Message::OpenDesktop(page))
        .into()
    }

    fn widget_extras(&self, widget: Widget) -> Element<'_, Message> {
        let key = widget.key();
        let combined = self.config.glance.enabled
            && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar);
        let mut rows = Vec::new();
        if combined {
            rows.push(self.link_row_state(
                "Daily overview",
                "This widget is shown inside it, with clock, weather and calendar",
                Message::Page(Page::Glance),
                format!("{key}-open-glance"),
            ));
        } else {
            // Always the same row; it says what to do first when it can't open.
            if self.canvas_visible() {
                // The arranging canvas is already open above.
            } else if self.config.widget_selected(widget) {
                rows.push(self.link_row(
                    "Move or resize",
                    "Drag it on a picture of your desktop",
                    // Opens the canvas on this page; a bare Start stayed hidden.
                    Message::EditWidgetLayout(widget),
                    format!("{key}-arrange"),
                ));
            } else {
                rows.push(
                    // Offer the one step that makes it possible.
                    split(
                        detail_state("Move or resize", "It needs to be on the desktop first"),
                        button::standard("Show on desktop")
                            .id(cosmic::widget::Id::new(format!("{key}-show")))
                            .on_press(Message::Widget(widget, true))
                            .into(),
                    ),
                );
            }
            if self.displays.outputs().len() > 1
                || self.config.widget_monitors.contains_key(key)
            {
                rows.push(self.widget_display_controls(widget));
            }
        }
        if let Some(kind) = crate::recipes::Kind::ALL
            .into_iter()
            .find(|kind| kind.widget() == widget)
        {
            let id = format!("{key}-own-design");
            rows.push(match self.config.recipes.get(kind) {
                Some(recipe) => self.link_row_state(
                    "Your own design",
                    recipe.name.clone(),
                    Message::OpenRecipe(kind),
                    id,
                ),
                None => self.link_row(
                    "Make your own design",
                    "Build this widget from parts",
                    Message::OpenRecipe(kind),
                    id,
                ),
            });
        }
        if rows.is_empty() {
            return column::with_children(Vec::new()).into();
        }
        group(None, rows)
    }
}
