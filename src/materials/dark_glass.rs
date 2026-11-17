//! Grain and an edge glint behind widget content. No wallpaper capture or idle clock.
use cosmic::iced::widget::shader::{self, Action, Viewport};
use cosmic::iced::{Color, Event, Rectangle, mouse, wgpu, window};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const GLINT: Duration = Duration::from_millis(700);
const FRAME: Duration = Duration::from_millis(33);
pub struct Program {
    pub accent: Color,
    pub radius: f32,
    pub animate: bool,
}
#[derive(Default)]
pub struct State {
    hover: bool,
    start: Option<Instant>,
    resources: Arc<Mutex<Option<Resources>>>,
}
impl State {
    fn phase(&self, now: Instant) -> Option<f32> {
        self.start.and_then(|start| {
            let elapsed = now.saturating_duration_since(start);
            (elapsed < GLINT).then_some(elapsed.as_secs_f32() / GLINT.as_secs_f32())
        })
    }
}
impl<M> shader::Program<M> for Program {
    type State = State;
    type Primitive = Primitive;
    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        if !self.animate {
            state.start = None;
            state.hover = cursor.is_over(bounds);
            return None;
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
        ) {
            let hover =
                cursor.is_over(bounds) && !matches!(event, Event::Mouse(mouse::Event::CursorLeft));
            let entered = hover && !state.hover;
            state.hover = hover;
            if entered {
                state.start = Some(Instant::now());
                return Some(Action::request_redraw());
            }
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            if state.phase(*now).is_some() {
                return Some(Action::request_redraw_at(*now + FRAME));
            }
            state.start = None;
        }
        None
    }
    fn draw(&self, state: &State, _: mouse::Cursor, bounds: Rectangle) -> Primitive {
        let phase = if self.animate {
            state.phase(Instant::now()).unwrap_or(-1.)
        } else {
            -1.
        };
        let accent = self.accent.into_linear();
        Primitive {
            params: [
                bounds.width,
                bounds.height,
                self.radius,
                phase,
                accent[0],
                accent[1],
                accent[2],
                0.,
            ],
            resources: state.resources.clone(),
        }
    }
}
#[derive(Debug)]
pub struct Primitive {
    params: [f32; 8],
    resources: Arc<Mutex<Option<Resources>>>,
}
#[derive(Debug)]
struct Resources {
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
}
pub struct Pipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}
impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Dark glass widget finish"),
            source: wgpu::ShaderSource::Wgsl(include_str!("dark_glass.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Dark glass parameters"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Dark glass layout"),
            bind_group_layouts: &[&layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Dark glass"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { layout, pipeline }
    }
}
impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;
    fn prepare(
        &self,
        pipeline: &mut Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &Rectangle,
        _: &Viewport,
    ) {
        let mut slot = self.resources.lock().unwrap();
        let resources = slot.get_or_insert_with(|| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Widget glass uniforms"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Widget glass"),
                layout: &pipeline.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            Resources { buffer, bind }
        });
        let bytes = self
            .params
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect::<Vec<_>>();
        queue.write_buffer(&resources.buffer, 0, &bytes);
    }
    fn draw(&self, pipeline: &Pipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let resources = self.resources.lock().unwrap();
        if let Some(resources) = resources.as_ref() {
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &resources.bind, &[]);
            pass.draw(0..3, 0..1);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glass_is_idle_until_hover_and_obeys_reduced_motion() {
        let mut program = Program {
            accent: Color::WHITE,
            radius: 24.,
            animate: true,
        };
        let mut state = State::default();
        let bounds = Rectangle::with_size(cosmic::iced::Size::new(300., 200.));
        let cursor = mouse::Cursor::Available(bounds.center());
        let redraw = Event::Window(window::Event::RedrawRequested(Instant::now()));
        assert!(
            <Program as shader::Program<()>>::update(&program, &mut state, &redraw, bounds, cursor)
                .is_none()
        );
        let enter = Event::Mouse(mouse::Event::CursorMoved {
            position: bounds.center(),
        });
        assert!(
            <Program as shader::Program<()>>::update(&program, &mut state, &enter, bounds, cursor)
                .is_some()
        );
        state.start = Some(Instant::now() - GLINT - FRAME);
        assert!(
            <Program as shader::Program<()>>::update(&program, &mut state, &redraw, bounds, cursor)
                .is_none()
        );
        program.animate = false;
        state.hover = false;
        assert!(
            <Program as shader::Program<()>>::update(&program, &mut state, &enter, bounds, cursor)
                .is_none()
        );
        let p: Primitive = <Program as shader::Program<()>>::draw(&program, &state, cursor, bounds);
        assert_eq!(p.params[3], -1.);
    }
    #[test]
    fn dark_glass_style_keeps_its_opacity_and_opaque_ink() {
        let mut config = crate::config::Config::default();
        config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
        config.appearance.opacity = 40;
        let palette = config.palette();
        assert!((palette.background.a - 0.4).abs() < 0.001);
        assert_eq!(palette.foreground.a, 1.);
        assert_eq!(palette.muted.a, 1.);
        assert!(crate::widget_style::luminance(palette.background) < 0.02);
        assert!(crate::widget_style::luminance(palette.foreground) > 0.7);
        let restored: crate::config::Config =
            serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
        assert_eq!(restored.appearance.family, config.appearance.family);
        assert_eq!(restored.palette(), palette);
        config.materials.widgets = Some(crate::materials::Finish {
            kind: crate::materials::Kind::Clear,
            opacity: Some(25),
            tint: 20,
        });
        assert!((config.palette().background.a - 0.25).abs() < 0.001);
        config.appearance.contrast_assist = true;
        assert!(config.palette().background.a > 0.25);
        config.appearance.contrast_assist = false;
        let controls = config.surface_palette();
        config.appearance.accent = Some(crate::desktop_theme::Family::DarkGlass.accent());
        config.appearance.family = Some(crate::desktop_theme::Family::Soft);
        config.appearance.opacity = crate::desktop_theme::Family::Soft.geometry().0;
        assert_eq!(config.surface_palette(), controls);
        config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
        config.appearance.surface = Some(crate::desktop_theme::Surface::Transparent);
        assert!(config.theme_roles().unwrap().dark_glass.is_none());
    }

    #[test]
    fn dark_glass_gpu_and_software_render_without_changing_input() {
        use cosmic::iced::{Point, Size};
        let mut config = crate::config::Config::default();
        config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
        config.appearance.opacity = 40;
        config.appearance.corner_radius = 24;
        config.material_motion = false;
        let size = Size::new(300., 200.);
        for backend in ["tiny-skia", "wgpu"] {
            let mut ui = crate::native_test::Native::with_backend(size, backend);
            let view = || {
                crate::tools::cards::frame(
                    cosmic::widget::button::text("Open")
                        .id(cosmic::widget::Id::new("glass-test-open"))
                        .on_press(())
                        .into(),
                    &config,
                    200.,
                )
            };
            let pixels = ui.render_scaled(view(), &cosmic::Theme::dark(), 1.);
            let alpha = |x: usize, y: usize| pixels[(y * 300 + x) * 4 + 3];
            assert!(alpha(0, 0) < 20, "{backend}: rounded corner stays clear");
            assert!(
                (95..=130).contains(&alpha(150, 2)) && alpha(150, 150) > alpha(150, 2) + 20,
                "{backend}: {}",
                alpha(150, 150)
            );
            let pixel = (150 * 300 + 150) * 4;
            let opacity = f32::from(pixels[pixel + 3]) / 255.;
            let over_white = |channel: usize| 1. - opacity + f32::from(pixels[pixel + channel]) / 255. * opacity;
            let background = Color::from_rgb(over_white(0), over_white(1), over_white(2));
            let palette = config.palette();
            for ink in [palette.foreground, palette.muted, palette.accent] {
                assert!(crate::widget_style::ratio(ink, background) >= 4.5, "{backend}: {ink:?} on {background:?}");
            }
            let at = ui.bounds(view(), "glass-test-open").center();
            let _ = ui.event(
                view(),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                at,
            );
            let (messages, _) = ui.event(
                view(),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                at,
            );
            assert_eq!(messages, vec![()]);
            if let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
                let path = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&path).unwrap();
                ui.capture(
                    view(),
                    &cosmic::Theme::dark(),
                    &format!("dark-glass-{backend}"),
                );
                // A real widget, using the same native rendering path.
                let sample = crate::messages::sample();
                let mut ui =
                    crate::native_test::Native::with_backend(Size::new(340., 280.), backend);
                ui.capture(
                    crate::tools::cards::messages::<()>(&sample, &config, None, Some([340., 280.])),
                    &cosmic::Theme::dark(),
                    &format!("dark-glass-messages-{backend}"),
                );
                let paper = ::image::open(concat!(
                    env!("CARGO_MANIFEST_DIR"), "/assets/wallpapers/halcyon-paper.jpg"
                )).unwrap().thumbnail(840, 560).into_rgba8();
                let wallpaper = cosmic::widget::image::Handle::from_rgba(
                    paper.width(), paper.height(), paper.into_raw()
                );
                let mut scene = crate::native_test::Native::with_backend(Size::new(480., 360.), backend);
                let card = crate::tools::cards::messages::<()>(&sample, &config, None, Some([340., 280.]));
                let scene_widget = cosmic::iced::widget::Stack::new().push(
                    cosmic::widget::image(wallpaper)
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .width(480).height(360),
                ).push(cosmic::widget::container(cosmic::widget::container(card).width(340))
                    .center_x(480).center_y(360)).width(480).height(360).clip(true);
                scene.capture(scene_widget.into(), &cosmic::Theme::dark(), &format!("dark-glass-paper-{backend}"));
                let mut clear = config.clone();
                clear.materials.widgets = Some(crate::materials::Finish {
                    kind: crate::materials::Kind::Clear,
                    opacity: Some(25),
                    tint: 20,
                });
                ui.capture(
                    crate::tools::cards::messages::<()>(&sample, &clear, None, Some([340., 280.])),
                    &cosmic::Theme::dark(),
                    &format!("dark-glass-messages-clear-{backend}"),
                );
            }
            // Changing style in an existing tree drops the shader state cleanly.
            let _ = ui.event(
                view(),
                Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::ORIGIN,
                }),
                Point::ORIGIN,
            );
            config.appearance.family = Some(crate::desktop_theme::Family::Soft);
            let plain = crate::tools::cards::frame::<()>(
                cosmic::widget::text("Plain").into(),
                &config,
                200.,
            );
            assert!(!ui.render(plain, &cosmic::Theme::dark()).is_empty());
            config.appearance.family = Some(crate::desktop_theme::Family::DarkGlass);
        }
    }
}
