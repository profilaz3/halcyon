use super::*;
use crate::tools::visualizer::{self, Circle};
use cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface;
impl Cards {
    pub(super) fn visualizer_demand(&self) -> bool {
        self.music_demand()
            && self.config.material_motion
            && self.config.music.visualizer.active()
            && self
                .current_player()
                .is_some_and(|p| p.status == crate::tools::music::Playback::Playing)
    }
    pub(super) fn spectrum_view(&self) -> Element<'_, Message> {
        if !self.visualizer_demand() {
            return empty_surface();
        }
        let mut color = self.config.palette().accent;
        color.a = f32::from(self.config.music.visualizer.opacity) / 100.;
        cosmic::widget::canvas(visualizer::drawing::Visual {
            bins: self.tools.audio_bins,
            circle: Circle::Off,
            color,
            backdrop: None,
            region: Some(self.config.music.visualizer.clone()),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
    pub(super) fn sync_spectrum(&mut self) -> Task<Message> {
        if !self.visualizer_demand() {
            self.tools.audio_bins = [0.; visualizer::BINS];
        }
        let visible = self.output_known
            && self.selected_output.is_some()
            && self.visualizer_demand()
            && self.config.music.visualizer.spectrum
            && self.config.music.visualizer.opacity > 0;
        let key = visible.then(|| {
            format!(
                "{:?}:{}:{}:{:?}",
                self.ids[12],
                self.display_generation,
                self.config.music.visualizer.behind_dock,
                self.widget_output(Widget::Music).map(|o| &o.handle)
            )
        });
        if key == self.tools.spectrum_key {
            return Task::none();
        }
        self.tools.spectrum_key = key;
        let remove = destroy_layer_surface(self.ids[12]);
        if !visible {
            return remove;
        }
        // Full-output bounds opt out of panel reservations only when explicitly
        // selected. Bottom layer and empty input region keep the dock usable.
        remove.chain(get_layer_surface(SctkLayerSurfaceSettings {
            id: self.ids[12],
            output: self.widget_layer_output(Widget::Music),
            layer: Layer::Bottom,
            keyboard_interactivity: KeyboardInteractivity::None,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            size: Some((None, None)),
            exclusive_zone: if self.config.music.visualizer.behind_dock {
                -1
            } else {
                0
            },
            input_zone: Some(Vec::new()),
            namespace: "halcyon-spectrum".into(),
            ..Default::default()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visualizer_demand_stops_for_pause_hidden_motion_and_arrange() {
        let mut config = Config::default();
        config.music.enabled = true;
        config.music.visualizer.spectrum = true;
        let (mut app, _) = Cards::init(Core::default(), config);
        let _output = app.test_output_ready();
        let props = std::collections::HashMap::new();
        let mut player = crate::tools::music::parse_player(
            "org.mpris.MediaPlayer2.fixture",
            &props,
            &props,
            Instant::now(),
        );
        player.status = crate::tools::music::Playback::Playing;
        let _ = app.music_feed(crate::tools::music::Feed::Players(vec![player.clone()]));
        assert!(app.visualizer_demand());
        app.tools.audio_bins = [1.; visualizer::BINS];
        app.config.music.visualizer.opacity = 0;
        assert!(!app.visualizer_demand());
        let _ = app.sync_spectrum();
        assert_eq!(app.tools.audio_bins, [0.; visualizer::BINS]);
        app.config.music.visualizer.circle = Circle::Bars;
        assert!(app.visualizer_demand());
        app.config.material_motion = false;
        assert!(!app.visualizer_demand());
        app.config.material_motion = true;
        app.config.music.enabled = false;
        assert!(!app.visualizer_demand());
        app.config.music.enabled = true;
        app.layout_editor.active = true;
        assert!(!app.visualizer_demand());
        app.layout_editor.active = false;
        player.status = crate::tools::music::Playback::Paused;
        let _ = app.music_feed(crate::tools::music::Feed::Players(vec![player]));
        assert!(!app.visualizer_demand());
    }
}
