use super::*;
use crate::displays::Output;
use std::collections::BTreeMap;

/// Measures the usable area of a display that holds widgets but isn't the
/// one Halcyon is drawn on (see `Cards::selected_output`). It draws nothing,
/// so like that display's arrange strip it is made of strips a pixel thick
/// rather than a window the size of the display.
///
/// The compositor chooses a layer surface's length only along an axis where
/// the surface is anchored to both edges, so one surface that reports both
/// the usable width and height would have to cover the usable area. Instead
/// there are two: one along the top, sized to the usable width, and one
/// along the left, sized to the usable height. With an exclusive zone of
/// zero each is placed inside the area that panels and docks leave free, so
/// docks on the left or right shorten the first and panels on the top or
/// bottom shorten the second.
///
/// Arranging on this display makes it the selected one, where the arrange
/// surface covers it; a probe never needs to.
pub(super) struct Probe {
    pub output: Output,
    /// The width strip, then the height strip.
    pub ids: [window::Id; 2],
    /// The usable width and height, as each strip last reported it.
    pub work_area: [Option<f32>; 2],
}
impl Probe {
    pub fn new(output: Output) -> Self {
        Self {
            output,
            ids: [window::Id::unique(), window::Id::unique()],
            work_area: [None; 2],
        }
    }
    pub fn owns(&self, id: window::Id) -> bool {
        self.ids.contains(&id)
    }
    /// The usable area, once both strips are sized.
    pub fn size(&self) -> Option<[f32; 2]> {
        let [Some(width), Some(height)] = self.work_area else {
            return None;
        };
        Some([width, height])
    }
    /// Records what the compositor sized one of the strips to, and says
    /// whether that changed the usable area. Only a strip's long side is a
    /// measurement; across, it is a pixel.
    pub fn measured(&mut self, id: window::Id, size: cosmic::iced::Size) -> bool {
        let axis = usize::from(id == self.ids[1]);
        let length = if axis == 0 { size.width } else { size.height };
        if !length.is_finite() || length <= 0. {
            return false;
        }
        let before = self.size();
        self.work_area[axis] = Some(length);
        self.size().is_some() && self.size() != before
    }
}
impl Cards {
    pub(super) fn arrange_on_display(&mut self, display: String) -> Task<Message> {
        let Some(output) = self.displays.unique(&display) else {
            return Task::none();
        };
        if self.layout_editor.active
            || self
                .paused_layout
                .as_ref()
                .is_some_and(|e| e.exit_failed || e.pending_resume.is_some())
        {
            return Task::none();
        }
        self.pending_arrange = Some(output.handle.clone());
        self.paused_layout = None;
        self.arrange_display = Some(display);
        let migrate = self.reconcile_output();
        if self.output_known {
            Task::batch([migrate, self.complete_pending_arrange()])
        } else {
            migrate
        }
    }

    pub(super) fn complete_pending_arrange(&mut self) -> Task<Message> {
        let Some(expected) = self.pending_arrange.take() else {
            return Task::none();
        };
        if self.output_known
            && self
                .selected_output
                .as_ref()
                .is_some_and(|o| o.handle == expected)
            && self.displays.outputs().iter().any(|o| o.handle == expected)
        {
            self.arrange()
        } else {
            self.arrange_display = None;
            self.reconcile_output()
        }
    }

    pub(super) fn widget_output(&self, widget: Widget) -> Option<&Output> {
        self.displays.widget(&self.config, widget)
    }
    pub(super) fn widget_layer_output(&self, widget: Widget) -> IcedOutput {
        self.widget_output(widget)
            .map(|o| IcedOutput::Output(o.handle.clone()))
            .unwrap_or_else(|| self.layer_output())
    }
    pub(super) fn widget_screen(&self, widget: Widget) -> [f32; 2] {
        let Some(output) = self.widget_output(widget) else {
            return self.screen_size;
        };
        self.extra_probes
            .iter()
            .find(|p| p.output.handle == output.handle)
            .and_then(Probe::size)
            .unwrap_or(self.screen_size)
    }
    pub(super) fn widget_available(&self, widget: Widget) -> bool {
        let Some(output) = self.widget_output(widget) else {
            return self.displays.outputs().is_empty();
        };
        self.extra_probes
            .iter()
            .find(|p| p.output.handle == output.handle)
            .is_none_or(|p| p.size().is_some())
    }
    pub(super) fn same_widget_display(&self, a: Widget, b: Widget) -> bool {
        self.widget_output(a).map(|o| &o.handle) == self.widget_output(b).map(|o| &o.handle)
    }
    pub(super) fn layout_contexts(&self) -> crate::config::monitors::Contexts {
        self.displays.contexts(&self.config)
    }
    pub(super) fn layout_output_handles(&self) -> BTreeMap<String, crate::displays::WlOutput> {
        Widget::ALL
            .into_iter()
            .filter_map(|widget| {
                Some((
                    widget.key().into(),
                    self.widget_output(widget)?.handle.clone(),
                ))
            })
            .collect()
    }
    pub(super) fn effective_config(&self) -> Config {
        self.displays.effective_config(&self.config)
    }
    pub(super) fn refresh_display_layouts(&mut self) {
        let config = self.effective_config();
        self.display_positions = config.widget_positions;
        self.display_sizes = config.widget_sizes;
    }
    pub(super) fn effective_positions(&self) -> &BTreeMap<String, [f32; 2]> {
        if self.config.multi_monitor() {
            &self.display_positions
        } else {
            &self.config.widget_positions
        }
    }
    pub(super) fn effective_sizes(&self) -> &BTreeMap<String, [f32; 2]> {
        if self.config.multi_monitor() {
            &self.display_sizes
        } else {
            &self.config.widget_sizes
        }
    }
    pub(super) fn arranging_widget(&self, widget: Widget) -> bool {
        self.config.widget_visible(widget) && self.on_arranged_display(widget)
    }
    /// Whether `widget`, shown or not, belongs to the display being
    /// arranged and that display is measured.
    pub(super) fn on_arranged_display(&self, widget: Widget) -> bool {
        self.widget_available(widget)
            && self.widget_output(widget).map(|o| &o.handle)
                == self.selected_output.as_ref().map(|o| &o.handle)
    }
    /// The strips that measure each other display with widgets (see
    /// `Probe`): invisible, never taking the pointer or the keyboard.
    pub(super) fn extra_probe_tasks(&self) -> Task<Message> {
        let thin = layout::PROBE_THICKNESS;
        Task::batch(self.extra_probes.iter().flat_map(|p| {
            [
                (
                    p.ids[0],
                    Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                    (Some(0), Some(thin)),
                ),
                (
                    p.ids[1],
                    Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                    (Some(thin), Some(0)),
                ),
            ]
            .map(|(id, anchor, size)| {
                get_layer_surface(SctkLayerSurfaceSettings {
                    id,
                    output: IcedOutput::Output(p.output.handle.clone()),
                    layer: Layer::Bottom,
                    keyboard_interactivity: KeyboardInteractivity::None,
                    anchor,
                    size: Some(size),
                    input_zone: Some(Vec::new()),
                    exclusive_zone: 0,
                    namespace: "halcyon-display-probe".into(),
                    ..Default::default()
                })
            })
        }))
    }
    pub(super) fn identity_scan(&self) -> Task<Message> {
        if std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").is_some() {
            return Task::none();
        }
        let generation = self.displays.generation;
        cosmic::task::future(async move {
            let ids = tokio::task::spawn_blocking(crate::displays::physical_identities)
                .await
                .unwrap_or_default();
            Message::DisplayIdentities(generation, ids)
        })
    }
}
