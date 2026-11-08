use crate::rain::{Preferences, gpu};
use cosmic::widget::image::Handle;
use std::{cell::RefCell, sync::Arc};
#[derive(Default)]
pub(super) struct State {
    pub preview: bool,
    pub lifecycle: crate::rain::lifecycle::State,
    /// The Weather widget's last reading for its city, from its cache. The
    /// Settings window never fetches weather itself.
    pub weather: Option<crate::rain::weather::Observation>,
    image: RefCell<Option<(Handle, Arc<gpu::Scene>)>>,
}
impl State {
    pub fn refresh_weather(&mut self, config: &crate::config::Config) {
        self.weather = config
            .location
            .as_ref()
            .and_then(crate::data::WeatherSnapshot::cached)
            .as_ref()
            .and_then(crate::rain::weather::Observation::from_snapshot);
    }
    pub fn scene(&self, handle: &Handle) -> Option<Arc<gpu::Scene>> {
        let mut cache = self.image.borrow_mut();
        if let Some((old, scene)) = &*cache
            && old.id() == handle.id()
        {
            return Some(scene.clone());
        }
        let Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } = handle
        else {
            return None;
        };
        let scene = gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
            width: *width,
            height: *height,
            rgba: pixels.to_vec(),
        });
        *cache = Some((handle.clone(), scene.clone()));
        Some(scene)
    }
    pub fn release_image(&self) {
        self.image.borrow_mut().take();
    }
    pub fn clear(&mut self) {
        self.preview = false;
        self.lifecycle = Default::default();
        self.image.get_mut().take();
    }
}
#[derive(Debug, Clone)]
pub enum Edit {
    Set(Preferences),
    Draft(Preferences),
    Commit,
    Preview(bool),
    Lifecycle(crate::rain::lifecycle::State),
}
