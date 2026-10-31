use crate::rain::{Preferences, gpu};
use cosmic::widget::image::Handle;
use std::{cell::RefCell, sync::Arc};
#[derive(Default)]
pub(super) struct State {
    pub draft: Option<Preferences>,
    pub base: Option<Preferences>,
    pub preview: bool,
    pub lifecycle: crate::rain::lifecycle::State,
    image: RefCell<Option<(Handle, Arc<gpu::Scene>)>>,
}
impl State {
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
        self.draft = None;
        self.base = None;
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
