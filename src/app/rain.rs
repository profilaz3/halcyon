//! Per-output rain surfaces, below desktop widgets and with empty input regions.
use super::*;
use crate::desktop::wallpaper::{BgEntry, BgSource, media};
use crate::rain::{gpu, lifecycle};
use cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface;
use std::sync::Arc;
#[derive(Default)]
pub(super) struct State {
    surfaces: Vec<Surface>,
    next: u64,
    native_state: Option<(std::path::PathBuf, Option<crate::watch::FileStamp>)>,
    pub lifecycle: lifecycle::State,
}
struct Surface {
    output: crate::displays::Output,
    id: window::Id,
    generation: u64,
    image: Option<Arc<gpu::Scene>>,
    picture: Option<std::path::PathBuf>,
    stamp: Option<crate::watch::FileStamp>,
    cancel: tokio::sync::watch::Sender<bool>,
}
impl Drop for Surface {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
    }
}
impl State {
    pub fn owns(&self, id: window::Id) -> bool {
        self.surfaces.iter().any(|s| s.id == id)
    }
    pub fn invalidate(&mut self) -> Task<Message> {
        if let Some((path, stamp)) = &mut self.native_state {
            *stamp = crate::watch::file_stamp(path);
        }
        Task::batch(self.surfaces.drain(..).map(|s| destroy_layer_surface(s.id)))
    }
    pub fn paths(&self) -> Vec<(std::path::PathBuf, Option<crate::watch::FileStamp>)> {
        self.surfaces
            .iter()
            .filter_map(|s| s.picture.clone().map(|p| (p, s.stamp)))
            .chain(self.native_state.iter().cloned())
            .collect()
    }
    pub fn ready(
        &mut self,
        id: window::Id,
        generation: u64,
        entry: Option<BgEntry>,
        pixels: Option<media::Pixels>,
        stamp: Option<crate::watch::FileStamp>,
    ) {
        if let Some(surface) = self
            .surfaces
            .iter_mut()
            .find(|s| s.id == id && s.generation == generation)
        {
            surface.picture = entry.and_then(|e| {
                if let BgSource::Path(p) = e.source {
                    Some(p)
                } else {
                    None
                }
            });
            surface.image = pixels.map(gpu::Scene::new);
            surface.stamp = stamp;
        }
    }
}
fn target_size(output: &crate::displays::Output) -> [u32; 2] {
    let (w, h) = output.info.logical_size.unwrap_or((1920, 1080));
    let scale = output.info.scale_factor.max(1) as f32;
    let w = (w.max(1) as f32 * scale).min(8192.);
    let h = (h.max(1) as f32 * scale).min(8192.);
    let factor = ((3840. * 2160.) / (w * h)).sqrt().min(1.);
    [
        (w * factor).floor().max(1.) as u32,
        (h * factor).floor().max(1.) as u32,
    ]
}
async fn background(
    output: String,
    size: [u32; 2],
    cancel: tokio::sync::watch::Receiver<bool>,
) -> (
    Option<BgEntry>,
    Option<media::Pixels>,
    Option<crate::watch::FileStamp>,
) {
    let (send, receive) = tokio::sync::oneshot::channel();
    media::worker::prepare_static(move || {
        let entry = crate::depth::observe(&output).and_then(|mut entry| {
            if let BgSource::Path(path) = &entry.source
                && path.is_dir()
            {
                let store = crate::desktop::DesktopStore::from_env().ok()?;
                entry.source = BgSource::Path(media::current_slideshow_image(&store, &output)?);
            }
            Some(entry)
        });
        let _ = send.send(entry);
    })
    .await;
    let entry = receive.await.ok().flatten();
    let stamp = entry.as_ref().and_then(|e| match &e.source {
        BgSource::Path(p) => crate::watch::file_stamp(p),
        _ => None,
    });
    let pixels = if let Some(entry) = entry.clone() {
        media::worker::wallpaper(entry, size, cancel).await
    } else {
        None
    };
    (entry, pixels, stamp)
}
impl Cards {
    pub(super) fn rain_view(&self, id: window::Id) -> Option<Element<'_, Message>> {
        let surface = self.rain.surfaces.iter().find(|s| s.id == id)?;
        Some(if let Some(scene) = &surface.image {
            crate::rain::visibility::visible(
                cosmic::iced::widget::Shader::new(gpu::Program {
                    scene: scene.clone(),
                    preferences: self.config.rain.clone(),
                    running: true,
                    fixed_time: None,
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            )
        } else {
            empty_surface()
        })
    }
    pub(super) fn rain_sync(&mut self) -> Task<Message> {
        if !self.config.rain.enabled {
            self.rain.lifecycle = Default::default();
        }
        if self.layout_editor.active
            || !self
                .config
                .rain
                .active(self.config.material_motion, self.rain.lifecycle)
        {
            return self.rain.invalidate();
        }
        if self.rain.native_state.is_none()
            && let Ok(store) = crate::desktop::DesktopStore::from_env()
            && let Some(path) = media::state_path(&store)
        {
            // cosmic-bg rewrites this receipt after recreating its surfaces,
            // including restarts with the same image. Remap above its new
            // Background layer without changing its config or owning its PID.
            self.rain.native_state = Some((path.clone(), crate::watch::file_stamp(&path)));
        }
        let mut tasks = Vec::new();
        let mut kept = Vec::new();
        for surface in self.rain.surfaces.drain(..) {
            if self
                .displays
                .outputs()
                .iter()
                .any(|o| o.same_geometry(&surface.output))
            {
                kept.push(surface);
            } else {
                tasks.push(destroy_layer_surface(surface.id));
            }
        }
        self.rain.surfaces = kept;
        for output in self.displays.outputs().iter().take(8) {
            if self
                .rain
                .surfaces
                .iter()
                .any(|s| s.output.handle == output.handle)
            {
                continue;
            }
            let Some(name) = output.name().map(str::to_owned) else {
                continue;
            };
            let size = target_size(output);
            self.rain.next = self.rain.next.wrapping_add(1);
            let generation = self.rain.next;
            let id = window::Id::unique();
            let (cancel, receiver) = tokio::sync::watch::channel(false);
            self.rain.surfaces.push(Surface {
                output: output.clone(),
                id,
                generation,
                image: None,
                picture: None,
                stamp: None,
                cancel,
            });
            tasks.push(get_layer_surface(SctkLayerSurfaceSettings {
                id,
                output: IcedOutput::Output(output.handle.clone()),
                layer: Layer::Background,
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                size: Some((None, None)),
                exclusive_zone: -1,
                input_zone: Some(vec![]),
                namespace: "halcyon-rain".into(),
                ..Default::default()
            }));
            tasks.push(cosmic::task::future(async move {
                let (entry, pixels, stamp) = background(name, size, receiver).await;
                Message::RainReady(id, generation, entry, pixels, stamp)
            }));
        }
        Task::batch(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rain_resize_keeps_owned_surfaces_and_rejects_retired_generations() {
        if crate::displays::test_support::isolated(
            "app::rain::tests::rain_resize_keeps_owned_surfaces_and_rejects_retired_generations",
        ) {
            return;
        }
        let (mut app, _) = Cards::init(Core::default(), Config::default());
        let wire = crate::displays::test_support::Wire::new();
        let output = crate::displays::Output {
            handle: wire.output(7),
            info: crate::displays::test_support::info(Some("Rain-test"), (1920, 1080)),
        };
        let (cancel, _) = tokio::sync::watch::channel(false);
        let id = window::Id::unique();
        app.rain.surfaces.push(Surface {
            output,
            id,
            generation: 12,
            image: None,
            picture: None,
            stamp: None,
            cancel,
        });
        let task = app.handle_update(Message::SurfaceResized(
            id,
            cosmic::iced::Size::new(1920., 1080.),
        ));
        assert!(
            cosmic::iced::runtime::task::into_stream(task).is_none(),
            "live rain is not a stale Arrange surface"
        );
        let _ = app.rain.invalidate();
        let task = app.handle_update(Message::SurfaceResized(
            id,
            cosmic::iced::Size::new(1920., 1080.),
        ));
        assert!(
            cosmic::iced::runtime::task::into_stream(task).is_some(),
            "late retired surfaces are still destroyed"
        );
    }
    #[test]
    fn rain_surface_completion_is_fenced_and_removal_cancels_owned_work() {
        let wire = crate::displays::test_support::Wire::new();
        let output = crate::displays::Output {
            handle: wire.output(7),
            info: crate::displays::test_support::info(Some("Rain-test"), (1920, 1080)),
        };
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let id = window::Id::unique();
        let mut state = State::default();
        state.surfaces.push(Surface {
            output,
            id,
            generation: 12,
            image: None,
            picture: None,
            stamp: None,
            cancel,
        });
        let pixels = || {
            Some(media::Pixels {
                width: 1,
                height: 1,
                rgba: vec![1, 2, 3, 255],
            })
        };
        state.ready(id, 11, None, pixels(), None);
        assert!(state.surfaces[0].image.is_none());
        state.ready(id, 12, None, pixels(), None);
        assert!(state.surfaces[0].image.is_some());
        let _ = state.invalidate();
        assert!(*receiver.borrow());
        assert!(state.surfaces.is_empty());
        state.ready(id, 12, None, pixels(), None);
        assert!(state.surfaces.is_empty());
    }
}
