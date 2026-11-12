//! Persistent, per-surface GPU resources; the pipeline/device are Iced's own.
//! The effect field is smaller than the wallpaper. Static beads and background
//! mipmaps are cached; moving drops never trigger image decoding or readbacks.
//! Rain keeps one more small texture between frames: the condensation on the
//! glass, which moving drops wipe away and which slowly fogs over again.
use super::{
    Effect, Mode, Preferences,
    simulation::{INSTANCE, LIMIT, Simulation},
    weather::Level,
};
use crate::desktop::wallpaper::media::Pixels;
use cosmic::iced::widget::shader::{self, Action, Viewport};
use cosmic::iced::{Event, Rectangle, mouse, wgpu, window};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct Scene {
    pub pixels: Pixels,
    resources: Mutex<Option<Resources>>,
    trace: bool,
    frames: std::sync::atomic::AtomicU64,
}
impl std::fmt::Debug for Scene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pixels.fmt(f)
    }
}
impl Scene {
    pub fn new(pixels: Pixels) -> Arc<Self> {
        Arc::new(Self {
            pixels,
            resources: Mutex::new(None),
            trace: std::env::var_os("HALCYON_RAIN_TRACE").is_some(),
            frames: std::sync::atomic::AtomicU64::new(0),
        })
    }
}
#[derive(Clone)]
pub struct Program {
    pub scene: Arc<Scene>,
    pub preferences: Preferences,
    pub running: bool,
    /// Fixed time supports deterministic native screenshots and comparisons.
    pub fixed_time: Option<f32>,
    pub fade: Fade,
}
/// How long one half of a scene change takes on the desktop: the old scene
/// fades out, then the new one fades in.
pub const FADE: Duration = Duration::from_millis(1500);
/// Desktop scene changes fade instead of switching at once. The Settings
/// preview leaves this off, so edits show immediately; fixed-time captures and
/// a stopped program never fade.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fade {
    pub enabled: bool,
    /// The surface is about to be removed: fade the scene out.
    pub hidden: bool,
    /// The weather's level, so a lighter or heavier sky fades too.
    pub level: Option<Level>,
}
/// What a viewer would call a different scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Look {
    effect: Effect,
    hybrid: bool,
    level: Option<Level>,
}
impl Program {
    fn look(&self) -> Option<Look> {
        (!self.fade.hidden && self.preferences.has_visible_effect()).then_some(Look {
            effect: self.preferences.effect,
            hybrid: self.preferences.hybrid(),
            level: self.fade.level,
        })
    }
    fn fades(&self) -> bool {
        self.fade.enabled && self.running && self.fixed_time.is_none()
    }
    /// The scene on screen: the outgoing one until it has faded out.
    fn shown<'a>(&'a self, state: &'a State) -> &'a Preferences {
        match &state.drawn {
            Some(drawn) if self.fades() && state.shown != self.look() => drawn,
            _ => &self.preferences,
        }
    }
    /// The opacity of the scene on screen.
    fn opacity(&self, state: &State) -> f32 {
        if self.fades() {
            state.fade
        } else if self.fade.hidden {
            0.
        } else {
            1.
        }
    }
}
#[derive(Default)]
pub struct State {
    last: Option<Instant>,
    time: f32,
    simulation: Simulation,
    seeded: bool,
    density: Option<u8>,
    scene: usize,
    /// The look on screen, the preferences it was drawn with and its
    /// opacity. A new surface starts with nothing shown and fades in.
    shown: Option<Look>,
    drawn: Option<Preferences>,
    fade: f32,
}
impl State {
    /// Advance a fade by `dt` seconds. True while a change is in progress.
    fn step(&mut self, program: &Program, dt: f32) -> bool {
        let target = program.look();
        if !program.fades() {
            self.shown = target;
            self.fade = if target.is_some() { 1. } else { 0. };
            return false;
        }
        let step = dt / FADE.as_secs_f32();
        if self.shown == target {
            self.drawn = Some(program.preferences.clone());
            if target.is_some() {
                self.fade = (self.fade + step).min(1.);
            }
        } else {
            self.fade = (self.fade - step).max(0.);
            if self.fade == 0. {
                self.shown = target;
                self.drawn = Some(program.preferences.clone());
            }
        }
        self.shown != target || (target.is_some() && self.fade < 1.)
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
        _: mouse::Cursor,
    ) -> Option<Action<M>> {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let identity = Arc::as_ptr(&self.scene) as usize;
            if state.scene != identity {
                *state = State {
                    scene: identity,
                    ..Default::default()
                };
            }
            // Nothing left on screen: the surface is about to be removed.
            if self.fades() && state.shown.is_none() && self.look().is_none() {
                state.last = None;
                return None;
            }
            let changing = self.fades() && (state.shown != self.look() || state.fade < 1.);
            if !self.running
                || self.fixed_time.is_some()
                || !(changing || self.shown(state).animates())
            {
                state.step(self, 0.);
                state.last = None;
                return None;
            }
            if self.scene.trace && state.last.is_none() {
                eprintln!("Rain update: running={} bounds={bounds:?}", self.running);
            }
            let fps = self.shown(state).quality.fps();
            let cadence = Duration::from_secs_f64(1. / f64::from(fps));
            if let Some(last) = state.last
                && now.saturating_duration_since(last) < cadence
            {
                return Some(Action::request_redraw_at(last + cadence));
            }
            let dt = state
                .last
                .map(|last| now.saturating_duration_since(last).as_secs_f32())
                .unwrap_or(0.);
            state.last = Some(*now);
            // A fade follows the clock, but a long gap (a stopped frame
            // callback) finishes it rather than replaying it.
            let changing = state.step(self, dt);
            let p = self.shown(state).clone();
            // A compositor can suppress frame callbacks for hours. Resume with
            // one small step, never a catch-up loop or a wall-clock time jump.
            if p.animates() {
                state.time = (state.time + dt.min(0.05)) % 3600.;
            }
            if p.hybrid() {
                let aspect = (bounds.width / bounds.height.max(1.)).clamp(0.1, 10.);
                if state.density != Some(p.intensity) {
                    state.simulation = Simulation::default();
                    state.seeded = false;
                    state.density = Some(p.intensity);
                }
                if !state.seeded {
                    state.simulation.seed(aspect, &p);
                    state.seeded = true;
                }
                state.simulation.step(dt, aspect, &p);
            }
            if !(changing || p.animates()) {
                state.last = None;
                return None;
            }
            return Some(Action::request_redraw_at(
                *now + Duration::from_secs_f64(1. / f64::from(p.quality.fps())),
            ));
        }
        None
    }
    fn draw(&self, state: &State, _: mouse::Cursor, bounds: Rectangle) -> Primitive {
        let p = self.shown(state);
        let mut drops = if p.hybrid() {
            state.simulation.instances(p.trails)
        } else {
            Vec::new()
        };
        let time = self.fixed_time.unwrap_or(state.time);
        let mut warmup = Vec::new();
        // A fixed-time capture has no history, so replay the last seconds to
        // show the paths drops have wiped through the condensation.
        let replay = self
            .fixed_time
            .filter(|_| p.effect == Effect::Rain && p.intensity > 0)
            .map(|t| ((t.clamp(0., 60.) * 30.) as usize).min(WARMUP))
            .unwrap_or(0);
        if p.hybrid() && (!state.seeded || state.density != Some(p.intensity)) {
            let mut sim = Simulation::default();
            let aspect = (bounds.width / bounds.height.max(1.)).clamp(0.1, 10.);
            sim.seed(aspect, p);
            let steps = self
                .fixed_time
                .map(|t| 60 + (t.clamp(0., 60.) * 30.) as usize)
                .unwrap_or(0);
            for step in 0..steps {
                sim.step(1. / 30., aspect, p);
                let left = steps - 1 - step;
                if left > 0 && left <= replay {
                    warmup.push((time - left as f32 / 30., sim.instances(p.trails)));
                }
            }
            drops = sim.instances(p.trails);
        } else {
            warmup.extend(
                (1..=replay)
                    .rev()
                    .map(|left| (time - left as f32 / 30., Vec::new())),
            );
        }
        Primitive {
            scene: self.scene.clone(),
            preferences: p.clone(),
            time,
            drops,
            fade: self.opacity(state),
            warmup,
        }
    }
}
#[derive(Debug)]
pub struct Primitive {
    pub scene: Arc<Scene>,
    pub preferences: Preferences,
    pub time: f32,
    pub drops: Vec<[f32; INSTANCE]>,
    /// The scene's opacity over the plain wallpaper, from 0 to 1.
    pub fade: f32,
    /// Earlier frames (time and drops, 1/30 s apart) replayed once before a
    /// fixed-time frame, so condensation shows wiped paths. Empty when live.
    pub warmup: Vec<(f32, Vec<[f32; INSTANCE]>)>,
}
/// At most eight seconds of frames are replayed before a fixed-time capture.
const WARMUP: usize = 240;
pub struct Pipeline {
    background_format: wgpu::TextureFormat,
    params: wgpu::BindGroupLayout,
    textures: wgpu::BindGroupLayout,
    feedback: wgpu::BindGroupLayout,
    mip_layout: wgpu::BindGroupLayout,
    static_pass: wgpu::RenderPipeline,
    procedural: wgpu::RenderPipeline,
    condense: wgpu::RenderPipeline,
    atmosphere: wgpu::RenderPipeline,
    drops: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    compose_snow: wgpu::RenderPipeline,
    compose_atmosphere: wgpu::RenderPipeline,
    mipmap: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}
struct Resources {
    device: wgpu::Device,
    _background: wgpu::Texture,
    uniform: wgpu::Buffer,
    params: wgpu::BindGroup,
    instances: wgpu::Buffer,
    fields: Fields,
    rain: bool,
    effect: Effect,
    last_field: Option<[f32; 24]>,
    dynamic_dirty: bool,
    last_drops: Vec<[f32; INSTANCE]>,
    /// The time of the last frame, for how far condensation has returned.
    last_time: Option<f32>,
    /// A fixed-time replay waiting for the next render: uniforms for every
    /// step and the final frame, the steps' drops and their counts.
    warmup: Option<Warmup>,
    warm_time: Option<f32>,
    size: [u32; 2],
    bounds: Rectangle,
    static_key: [u8; 2],
    static_dirty: bool,
}
struct Warmup {
    params: wgpu::Buffer,
    drops: Option<wgpu::Buffer>,
    counts: Vec<u32>,
}
fn floats(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}
fn texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    mips: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
fn texture_layout(device: &wgpu::Device, count: u32, label: &str) -> wgpu::BindGroupLayout {
    let entries = (0..count)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: if binding == 1 {
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
            } else {
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }
            },
            count: None,
        })
        .collect::<Vec<_>>();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &entries,
    })
}
fn render_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layouts: &[&wgpu::BindGroupLayout],
    vertex: &str,
    fragment: &str,
    format: wgpu::TextureFormat,
    instanced: bool,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(fragment),
        bind_group_layouts: layouts,
        immediate_size: 0,
    });
    let attrs = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 16,
            shader_location: 1,
        },
    ];
    let buffers = [wgpu::VertexBufferLayout {
        array_stride: (INSTANCE * 4) as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &attrs,
    }];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: if instanced { &buffers } else { &[] },
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Halcyon rain"),
            // glass.wgsl is GPL-3.0-only code adapted from Cosmic Forge.
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("glass.wgsl"), include_str!("rain.wgsl")).into(),
            ),
        });
        let params = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Rain parameters"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let textures = texture_layout(device, 5, "Rain textures");
        let feedback = texture_layout(device, 4, "Rain condensation");
        let mip_layout = texture_layout(device, 2, "Wallpaper mipmaps");
        let mip_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Wallpaper downsample"),
            source: wgpu::ShaderSource::Wgsl(MIPMAP.into()),
        });
        let static_pass = render_pipeline(
            device,
            &shader,
            &[&params],
            "fullscreen",
            "static_beads",
            wgpu::TextureFormat::Rgba16Float,
            false,
            None,
        );
        // Drops are premultiplied by coverage; a later drop lies on top.
        let procedural = render_pipeline(
            device,
            &shader,
            &[&params],
            "procedural_vertex",
            "lens_fragment",
            wgpu::TextureFormat::Rgba16Float,
            false,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        let condense = render_pipeline(
            device,
            &shader,
            &[&params, &feedback],
            "fullscreen",
            "condense",
            wgpu::TextureFormat::Rgba16Float,
            false,
            None,
        );
        let atmosphere = render_pipeline(
            device,
            &shader,
            &[&params],
            "fullscreen",
            "atmosphere",
            wgpu::TextureFormat::R16Float,
            false,
            None,
        );
        let drops = render_pipeline(
            device,
            &shader,
            &[&params],
            "drop_vertex",
            "lens_fragment",
            wgpu::TextureFormat::Rgba16Float,
            true,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        // Composition is opaque, so it replaces the target rather than blending
        // over it: the pass never reads what lies underneath.
        let compose = |fragment| {
            render_pipeline(
                device,
                &shader,
                &[&params, &textures],
                "composite_vertex",
                fragment,
                format,
                false,
                None,
            )
        };
        let composite = compose("composite");
        let compose_snow = compose("compose_snow");
        let compose_atmosphere = compose("compose_atmosphere");
        let background_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let mipmap = render_pipeline(
            device,
            &mip_shader,
            &[&mip_layout],
            "vs",
            "fs",
            background_format,
            false,
            None,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Rain sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            background_format,
            params,
            textures,
            feedback,
            mip_layout,
            static_pass,
            procedural,
            condense,
            atmosphere,
            drops,
            composite,
            compose_snow,
            compose_atmosphere,
            mipmap,
            sampler,
        }
    }
}
impl Resources {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &Pipeline,
        pixels: &Pixels,
        size: [u32; 2],
        effect: Effect,
    ) -> Self {
        let rain = effect == Effect::Rain;
        let mip_count = if rain {
            (pixels.width.max(pixels.height).ilog2() + 1).min(7)
        } else {
            1
        };
        let background = texture(
            device,
            "Rain wallpaper",
            [pixels.width, pixels.height],
            pipeline.background_format,
            mip_count,
        );
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &background,
                mip_level: 0,
                origin: Default::default(),
                aspect: wgpu::TextureAspect::All,
            },
            &pixels.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pixels.width * 4),
                rows_per_image: Some(pixels.height),
            },
            wgpu::Extent3d {
                width: pixels.width,
                height: pixels.height,
                depth_or_array_layers: 1,
            },
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Prepare wallpaper once"),
        });
        for level in 1..mip_count {
            let input = background.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let output = background.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Wallpaper mip"),
                layout: &pipeline.mip_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                ],
            });
            let mut pass = pass(
                &mut encoder,
                &output,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                "Wallpaper mip",
            );
            pass.set_pipeline(&pipeline.mipmap);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        let view = background.create_view(&Default::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rain uniforms"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Rain uniforms"),
            layout: &pipeline.params,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bounded rain drops"),
            size: (LIMIT * INSTANCE * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fields = Fields::new(device, pipeline, &view, size, effect);
        Self {
            device: device.clone(),
            _background: background,
            uniform,
            params,
            instances,
            fields,
            rain,
            effect,
            last_field: None,
            dynamic_dirty: true,
            last_drops: Vec::new(),
            last_time: None,
            warmup: None,
            warm_time: None,
            size,
            bounds: Rectangle::default(),
            static_key: [255, 255],
            static_dirty: true,
        }
    }
}
// Field targets resize independently; the uploaded wallpaper and mip chain stay cached.
struct Fields {
    _static: Option<wgpu::Texture>,
    static_view: wgpu::TextureView,
    _dynamic: wgpu::Texture,
    dynamic_view: wgpu::TextureView,
    /// Rain only: condensation (alpha) and the blurred wallpaper it shows
    /// (rgb), in two textures that take turns as the previous frame and the
    /// next one.
    fog: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// Reads condensation `i` and writes the other one.
    feedback: Vec<wgpu::BindGroup>,
    /// Composes with condensation `i`.
    composite: Vec<wgpu::BindGroup>,
    current: usize,
    /// New condensation textures start fully fogged.
    fresh: bool,
}
impl Fields {
    fn new(
        device: &wgpu::Device,
        pipeline: &Pipeline,
        view: &wgpu::TextureView,
        size: [u32; 2],
        effect: Effect,
    ) -> Self {
        let rain = effect == Effect::Rain;
        let static_texture = rain.then(|| {
            texture(
                device,
                "Cached rain beads",
                size,
                wgpu::TextureFormat::Rgba16Float,
                1,
            )
        });
        let static_view = static_texture
            .as_ref()
            .map(|t| t.create_view(&Default::default()))
            .unwrap_or_else(|| view.clone());
        // Snow is drawn at full resolution during composition.
        let dynamic = texture(
            device,
            "Moving wallpaper effect",
            if effect == Effect::Snow { [1, 1] } else { size },
            if rain {
                wgpu::TextureFormat::Rgba16Float
            } else {
                wgpu::TextureFormat::R16Float
            },
            1,
        );
        let dynamic_view = dynamic.create_view(&Default::default());
        let fog: Vec<_> = if rain {
            let fog_size = [(size[0] / 4).clamp(8, 480), (size[1] / 4).clamp(8, 480)];
            (0..2)
                .map(|_| {
                    let t = texture(
                        device,
                        "Rain condensation",
                        fog_size,
                        wgpu::TextureFormat::Rgba16Float,
                        1,
                    );
                    let v = t.create_view(&Default::default());
                    (t, v)
                })
                .collect()
        } else {
            Vec::new()
        };
        // Binding 1 is always the sampler; the view given for it is unused.
        fn textures<'a>(
            sampler: &'a wgpu::Sampler,
            views: &[&'a wgpu::TextureView],
        ) -> Vec<wgpu::BindGroupEntry<'a>> {
            views
                .iter()
                .enumerate()
                .map(|(binding, view)| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: if binding == 1 {
                        wgpu::BindingResource::Sampler(sampler)
                    } else {
                        wgpu::BindingResource::TextureView(view)
                    },
                })
                .collect()
        }
        let feedback = fog
            .iter()
            .map(|(_, previous)| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Rain condensation"),
                    layout: &pipeline.feedback,
                    entries: &textures(
                        &pipeline.sampler,
                        &[previous, previous, &dynamic_view, view],
                    ),
                })
            })
            .collect();
        let condensation: Vec<_> = if rain {
            fog.iter().map(|(_, v)| v).collect()
        } else {
            vec![&static_view]
        };
        let composite = condensation
            .iter()
            .map(|condensation| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Rain composition"),
                    layout: &pipeline.textures,
                    entries: &textures(
                        &pipeline.sampler,
                        &[view, view, &dynamic_view, condensation, &static_view],
                    ),
                })
            })
            .collect();
        Self {
            _static: static_texture,
            static_view,
            _dynamic: dynamic,
            dynamic_view,
            fog,
            feedback,
            composite,
            current: 0,
            fresh: true,
        }
    }
    /// Advance condensation by one frame from the current water field.
    fn condense(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &Pipeline,
        params: &wgpu::BindGroup,
    ) {
        if self.fog.is_empty() {
            return;
        }
        let next = 1 - self.current;
        let mut draw = pass(
            encoder,
            &self.fog[next].1,
            wgpu::LoadOp::Clear(wgpu::Color::WHITE),
            "Rain condensation",
        );
        draw.set_pipeline(&pipeline.condense);
        draw.set_bind_group(0, params, &[]);
        draw.set_bind_group(1, &self.feedback[self.current], &[]);
        draw.draw(0..3, 0..1);
        drop(draw);
        self.current = next;
    }
}
fn pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
    label: &str,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;
    fn prepare(
        &self,
        pipeline: &mut Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale = viewport.scale_factor() as f32;
        let factor = self.preferences.quality.scale();
        let size = [
            (bounds.width * scale * factor).round().clamp(16., 1920.) as u32,
            (bounds.height * scale * factor).round().clamp(16., 1920.) as u32,
        ];
        let full = Rectangle {
            x: bounds.x * scale,
            y: bounds.y * scale,
            width: bounds.width * scale,
            height: bounds.height * scale,
        };
        let screen = viewport.physical_size();
        let visible = full
            .intersection(&Rectangle {
                x: 0.,
                y: 0.,
                width: screen.width as f32,
                height: screen.height as f32,
            })
            .unwrap_or_default();
        if visible.width <= 0. || visible.height <= 0. {
            if let Some(r) = self.scene.resources.lock().unwrap().as_mut() {
                r.bounds = Rectangle::default();
            }
            return;
        }
        let mut slot = self.scene.resources.lock().unwrap();
        let effect = self.preferences.effect;
        if slot
            .as_ref()
            .is_none_or(|r| r.device != *device || r.rain != (effect == Effect::Rain))
        {
            *slot = Some(Resources::new(
                device,
                queue,
                pipeline,
                &self.scene.pixels,
                size,
                effect,
            ));
        }
        let r = slot.as_mut().unwrap();
        if r.size != size || r.effect != effect {
            let view = r._background.create_view(&Default::default());
            r.fields = Fields::new(device, pipeline, &view, size, effect);
            r.effect = effect;
            r.size = size;
            r.warm_time = None;
            r.static_dirty = true;
            r.dynamic_dirty = true;
        }
        r.bounds = visible;
        let crop = [
            (r.bounds.x - full.x) / full.width.max(1.),
            (r.bounds.y - full.y) / full.height.max(1.),
            r.bounds.width / full.width.max(1.),
            r.bounds.height / full.height.max(1.),
        ];
        let key = [self.preferences.intensity, self.preferences.size];
        r.static_dirty |= r.static_key != key;
        r.static_key = key;
        let p = &self.preferences;
        let (amount, size, speed, wind, effect, extra, tint) = match p.effect {
            Effect::Rain => (
                p.intensity,
                p.size,
                p.speed,
                p.wind,
                if p.mode == Mode::Hybrid { 1. } else { 0. },
                [0.; 4],
                [0.; 4],
            ),
            Effect::Snow => (
                p.snow.density,
                p.snow.size,
                p.snow.speed,
                p.snow.wind,
                2.,
                [f32::from(p.snow.opacity) / 100., 0., 0., 0.],
                [0.92, 0.96, 1., 0.],
            ),
            Effect::Fog => (
                p.fog.opacity,
                0,
                p.fog.speed,
                p.fog.wind,
                3.,
                [
                    f32::from(p.fog.opacity) / 100.,
                    f32::from(p.fog.height) / 100.,
                    0.,
                    0.,
                ],
                p.fog.color.rgb(),
            ),
            Effect::Clouds => (
                p.clouds.coverage,
                0,
                p.clouds.speed,
                p.clouds.wind,
                4.,
                [
                    f32::from(p.clouds.strength) / 100.,
                    0.,
                    f32::from(p.clouds.softness) / 100.,
                    0.,
                ],
                [0.; 4],
            ),
        };
        let mut values = [0.; 24];
        values[..12].copy_from_slice(&[
            bounds.width / bounds.height.max(1.),
            self.time,
            f32::from(amount) / 100.,
            f32::from(size) / 100.,
            f32::from(speed) / 100.,
            f32::from(wind) / 100.,
            f32::from(p.mist) / 100.,
            f32::from(p.refraction) / 100.,
            effect,
            1.,
            f32::from(p.trails) / 100.,
            0.,
        ]);
        values[16..20].copy_from_slice(&extra);
        values[20..].copy_from_slice(&tint);
        if r.rain {
            // One field texel, for antialiased drop edges.
            values[17] = 1. / r.size[1] as f32;
            // Eco draws fewer layers of water.
            values[18] = if p.quality == super::Quality::Eco {
                0.
            } else {
                1.
            };
        }
        // One output pixel, so snowflakes never shrink below it.
        values[23] = 1. / full.height.max(1.);
        // Crop changes only composition, so scrolling cannot invalidate a frozen field.
        r.dynamic_dirty |= r.last_drops != self.drops || r.last_field.as_ref() != Some(&values);
        r.last_drops.clone_from(&self.drops);
        r.last_field = Some(values);
        // So does a fade: only composition reads it.
        values[11] = self.fade.clamp(0., 1.);
        values[12..16].copy_from_slice(&crop);
        if r.rain {
            // Seconds of condensation since the last frame. A jump back (a
            // new scene, a restarted clock) or a long pause adds little.
            values[16] = match r.last_time {
                Some(last) if self.time >= last => (self.time - last).min(0.25),
                _ => 0.,
            };
            r.last_time = Some(self.time);
        }
        queue.write_buffer(&r.uniform, 0, &floats(values));
        if !self.drops.is_empty() {
            queue.write_buffer(
                &r.instances,
                0,
                &floats(self.drops.iter().take(LIMIT).flatten().copied()),
            );
        }
        if r.rain && !self.warmup.is_empty() && r.warm_time != Some(self.time) {
            r.warm_time = Some(self.time);
            r.warmup = Some(warmup(device, queue, &self.warmup, values));
            r.dynamic_dirty = true;
        }
    }
    fn render(
        &self,
        pipeline: &Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip: &Rectangle<u32>,
    ) {
        let mut slot = self.scene.resources.lock().unwrap();
        let Some(r) = slot.as_mut() else {
            return;
        };
        if r.bounds.width <= 0. || r.bounds.height <= 0. || clip.width == 0 || clip.height == 0 {
            return;
        }
        if self.scene.trace {
            let n = self
                .scene
                .frames
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            if n == 1 || n % 30 == 0 {
                eprintln!(
                    "Rain frame: scene={:p} mode={:?} effect={:?} frames={n} bounds={:?} time={}",
                    Arc::as_ptr(&self.scene),
                    self.preferences.mode,
                    self.preferences.effect,
                    r.bounds,
                    self.time
                );
            }
        }
        if r.static_dirty && self.preferences.effect == Effect::Rain {
            let mut draw = pass(
                encoder,
                &r.fields.static_view,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                "Static rain beads",
            );
            draw.set_pipeline(&pipeline.static_pass);
            draw.set_bind_group(0, &r.params, &[]);
            draw.draw(0..3, 0..1);
            drop(draw);
            r.static_dirty = false;
        }
        if r.fields.fresh && r.rain {
            for (_, view) in &r.fields.fog {
                pass(
                    encoder,
                    view,
                    wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    "Fogged glass",
                );
            }
        }
        r.fields.fresh = false;
        if let Some(warm) = r.warmup.take() {
            let block = UNIFORM as u64;
            for (step, &count) in warm.counts.iter().enumerate() {
                encoder.copy_buffer_to_buffer(
                    &warm.params,
                    step as u64 * block,
                    &r.uniform,
                    0,
                    block,
                );
                let drops = warm.drops.as_ref().map(|buffer| {
                    let start = (step * LIMIT * INSTANCE * 4) as u64;
                    (
                        buffer.slice(start..start + (LIMIT * INSTANCE * 4) as u64),
                        count,
                    )
                });
                self.field(pipeline, encoder, r, drops);
                r.fields.condense(encoder, pipeline, &r.params);
            }
            let last = warm.counts.len() as u64 * block;
            encoder.copy_buffer_to_buffer(&warm.params, last, &r.uniform, 0, block);
        }
        if r.dynamic_dirty && self.preferences.effect != Effect::Snow {
            let count = self.drops.len().min(LIMIT) as u32;
            self.field(pipeline, encoder, r, Some((r.instances.slice(..), count)));
            r.fields.condense(encoder, pipeline, &r.params);
        }
        r.dynamic_dirty = false;
        let mut draw = pass(encoder, target, wgpu::LoadOp::Load, "Rain composition");
        draw.set_viewport(
            r.bounds.x,
            r.bounds.y,
            r.bounds.width,
            r.bounds.height,
            0.,
            1.,
        );
        draw.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
        draw.set_pipeline(match self.preferences.effect {
            Effect::Rain => &pipeline.composite,
            Effect::Snow => &pipeline.compose_snow,
            _ => &pipeline.compose_atmosphere,
        });
        draw.set_bind_group(0, &r.params, &[]);
        draw.set_bind_group(
            1,
            &r.fields.composite[r.fields.current.min(r.fields.composite.len() - 1)],
            &[],
        );
        draw.draw(0..3, 0..1);
    }
}
/// Bytes of one uniform block.
const UNIFORM: usize = 96;
impl Primitive {
    /// Draw the moving water, fog or cloud field.
    fn field(
        &self,
        pipeline: &Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        r: &Resources,
        drops: Option<(wgpu::BufferSlice<'_>, u32)>,
    ) {
        let mut draw = pass(
            encoder,
            &r.fields.dynamic_view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            "Rain field",
        );
        draw.set_bind_group(0, &r.params, &[]);
        if self.preferences.effect != Effect::Rain {
            draw.set_pipeline(&pipeline.atmosphere);
            draw.draw(0..3, 0..1);
            return;
        }
        if self.preferences.mode == Mode::Procedural {
            draw.set_pipeline(&pipeline.procedural);
            for range in procedural_draws(&r.last_field.unwrap_or_default()) {
                if !range.is_empty() {
                    draw.draw(0..6, range);
                }
            }
        } else if let Some((instances, count)) = drops.filter(|(_, count)| *count > 0) {
            draw.set_pipeline(&pipeline.drops);
            draw.set_vertex_buffer(0, instances);
            draw.draw(0..6, 0..count);
        }
    }
}
/// Procedural rain's lens quads: two impact grids of `CELLS` slots and three
/// runoff sets of `LANES`, laid out as in rain.wgsl.
const CELLS: u32 = 8192;
const LANES: u32 = 512;
/// The instance ranges that can reach the glass, in drawing order, from the
/// frame's uniform values (aspect, wind and detail). Eco draws neither the
/// finer impacts nor the narrow runoff lanes. The shader hides any slot that
/// still falls outside.
fn procedural_draws(values: &[f32; 24]) -> [std::ops::Range<u32>; 5] {
    let aspect = values[0].clamp(0., 64.);
    let detail = values[18] > 0.;
    let shear = (values[5] * 0.3).abs();
    let cells = |tile: f32| {
        let columns = (aspect / tile).ceil() as u32 + 3;
        (columns * ((1. / tile).ceil() as u32 + 3)).min(CELLS)
    };
    let lanes = |width: f32| (((aspect + shear) / width).ceil() as u32 + 4).min(LANES);
    let span = |first: u32, count: u32| first..first + count;
    let runoff = 2 * CELLS;
    [
        span(0, cells(0.05)),
        span(CELLS, if detail { cells(0.034) } else { 0 }),
        span(runoff, if detail { lanes(0.034) } else { 0 }),
        span(runoff + LANES, lanes(0.07)),
        span(runoff + 2 * LANES, lanes(0.07)),
    ]
}
/// Upload a fixed-time replay: one uniform block per step, then the frame's own.
fn warmup(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    steps: &[(f32, Vec<[f32; INSTANCE]>)],
    last: [f32; 24],
) -> Warmup {
    let steps = &steps[steps.len().saturating_sub(WARMUP)..];
    let mut blocks = Vec::with_capacity((steps.len() + 1) * UNIFORM);
    for (time, _) in steps {
        let mut values = last;
        values[1] = *time;
        values[16] = 1. / 30.;
        blocks.extend(floats(values));
    }
    blocks.extend(floats(last));
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Rain replay uniforms"),
        size: blocks.len() as u64,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&params, 0, &blocks);
    let stride = LIMIT * INSTANCE;
    let drops = steps.iter().any(|(_, d)| !d.is_empty()).then(|| {
        let mut values = vec![0.; steps.len() * stride];
        for (step, (_, drops)) in steps.iter().enumerate() {
            for (i, v) in drops.iter().take(LIMIT).flatten().enumerate() {
                values[step * stride + i] = *v;
            }
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rain replay drops"),
            size: (values.len() * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, &floats(values));
        buffer
    });
    Warmup {
        params,
        drops,
        counts: steps
            .iter()
            .map(|(_, d)| d.len().min(LIMIT) as u32)
            .collect(),
    }
}
const MIPMAP: &str = r#"
@group(0) @binding(0) var image:texture_2d<f32>;
@group(0) @binding(1) var s:sampler;
struct V { @builtin(position) p:vec4<f32>, @location(0) uv:vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i:u32)->V {
let q=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.))[i];
return V(vec4(q,0.,1.),vec2(q.x*0.5+0.5,0.5-q.y*0.5));
}
@fragment fn fs(v:V)->@location(0) vec4<f32> {return textureSampleLevel(image,s,v.uv,0.);}
"#;

#[cfg(test)]
impl Scene {
    pub fn texture_bytes(&self) -> u64 {
        fn bytes(t: &wgpu::Texture) -> u64 {
            let bpp = match t.format() {
                wgpu::TextureFormat::R16Float => 2,
                wgpu::TextureFormat::Rgba16Float => 8,
                _ => 4,
            };
            (0..t.mip_level_count())
                .map(|i| {
                    u64::from((t.width() >> i).max(1)) * u64::from((t.height() >> i).max(1)) * bpp
                })
                .sum()
        }
        self.resources
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| {
                bytes(&r._background)
                    + bytes(&r.fields._dynamic)
                    + r.fields._static.as_ref().map(bytes).unwrap_or(0)
                    + r.fields.fog.iter().map(|(t, _)| bytes(t)).sum::<u64>()
            })
            .unwrap_or(0)
    }
}
#[cfg(test)]
mod optimization_tests {
    #[test]
    fn rain_procedural_draws_cover_the_glass_and_skip_unused_slots() {
        use super::{CELLS, LANES, procedural_draws};
        let mut values = [0.; 24];
        values[0] = 16. / 9.;
        values[18] = 1.;
        let full = procedural_draws(&values);
        // Wide enough for every cell and lane on a 16:9 glass, far below the
        // slots a full draw would run.
        assert_eq!(full[0].len(), 39 * 23);
        assert_eq!(full[1].start, CELLS);
        assert_eq!(full[1].len(), 56 * 33);
        assert!(full[3].len() >= 29 && full[3].len() < 40);
        assert_eq!(full[4].start, 2 * CELLS + 2 * LANES);
        let total: usize = full.iter().map(|r| r.len()).sum();
        assert!(total < 3500, "{total}");
        values[18] = 0.;
        let eco = procedural_draws(&values);
        assert!(eco[1].is_empty() && eco[2].is_empty());
        assert_eq!(eco[0], full[0]);
        // Wind slants the lanes, so more of them reach the glass; extreme
        // shapes stay within the slots the shader lays out.
        values[5] = -1.;
        assert!(procedural_draws(&values)[3].len() > eco[3].len());
        values[0] = 1e9;
        values[18] = 1.;
        let wide = procedural_draws(&values);
        assert!(wide[0].len() as u32 <= CELLS && wide[2].len() as u32 <= LANES);
        assert!(wide.windows(2).all(|w| w[0].end <= w[1].start));
    }
    use super::*;
    use shader::{Pipeline as _, Primitive as _};
    #[test]
    fn effect_gpu_resources_resize_without_upload_and_cache_frozen_fields() {
        if std::env::var_os("HALCYON_RAIN_GPU_TEST").is_none() {
            return;
        }
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = cosmic::iced::futures::executor::block_on(
            instance.request_adapter(&Default::default()),
        )
        .unwrap();
        let (device, queue) =
            cosmic::iced::futures::executor::block_on(adapter.request_device(&Default::default()))
                .unwrap();
        let mut pipeline = Pipeline::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let scene = Scene::new(Pixels {
            width: 64,
            height: 64,
            rgba: vec![100; 64 * 64 * 4],
        });
        let mut primitive = Primitive {
            scene: scene.clone(),
            preferences: Preferences {
                effect: Effect::Snow,
                ..Default::default()
            },
            time: 2.,
            drops: Vec::new(),
            fade: 1.,
            warmup: Vec::new(),
        };
        let viewport = Viewport::with_physical_size(cosmic::iced::Size::new(1280, 720), 1.);
        let mut bounds = Rectangle {
            x: 0.,
            y: 0.,
            width: 640.,
            height: 360.,
        };
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        let background = {
            let lock = scene.resources.lock().unwrap();
            let r = lock.as_ref().unwrap();
            assert!(r.fields._static.is_none());
            assert_eq!(r._background.mip_level_count(), 1);
            assert_eq!(r.fields._dynamic.format(), wgpu::TextureFormat::R16Float);
            r._background.clone()
        };
        // Snow is drawn during composition, so its field is a placeholder.
        assert_eq!(scene.texture_bytes(), 64 * 64 * 4 + 2);
        bounds.width = 1280.;
        bounds.height = 720.;
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        assert_eq!(
            scene
                .resources
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                ._background,
            background
        );
        let output = texture(
            &device,
            "Cache test target",
            [1280, 720],
            wgpu::TextureFormat::Rgba8Unorm,
            1,
        );
        let target = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        primitive.render(
            &pipeline,
            &mut encoder,
            &target,
            &Rectangle {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            },
        );
        queue.submit([encoder.finish()]);
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        assert!(
            !scene
                .resources
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .dynamic_dirty
        );
        bounds.y = -10.;
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        assert!(
            !scene
                .resources
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .dynamic_dirty,
            "scroll crop alone must not regenerate field"
        );
        primitive.time = 3.;
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        assert!(
            scene
                .resources
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .dynamic_dirty
        );
        primitive.preferences.effect = Effect::Rain;
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        {
            let lock = scene.resources.lock().unwrap();
            let r = lock.as_ref().unwrap();
            assert!(r.fields._static.is_some());
            assert_eq!(r._background.mip_level_count(), 7);
        }
        primitive.preferences.effect = Effect::Fog;
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        assert!(
            scene
                .resources
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .fields
                ._static
                .is_none()
        );
        let unused = Scene::new(Pixels {
            width: 64,
            height: 64,
            rgba: vec![100; 64 * 64 * 4],
        });
        primitive.scene = unused.clone();
        bounds.y = 800.;
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        assert_eq!(
            unused.texture_bytes(),
            0,
            "invisible primitive must not allocate"
        );
    }
}
