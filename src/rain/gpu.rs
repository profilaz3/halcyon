//! Persistent, per-surface GPU resources; the pipeline/device are Iced's own.
//! The effect field is smaller than the wallpaper. Static beads and background
//! mipmaps are cached; moving drops never trigger image decoding or readbacks.
use super::{Effect, Mode, Preferences, simulation::Simulation};
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
}
#[derive(Default)]
pub struct State {
    last: Option<Instant>,
    time: f32,
    simulation: Simulation,
    seeded: bool,
    density: Option<u8>,
    scene: usize,
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
            if !self.running || self.fixed_time.is_some() || !self.preferences.animates() {
                state.last = None;
                return None;
            }
            if self.scene.trace && state.last.is_none() {
                eprintln!("Rain update: running={} bounds={bounds:?}", self.running);
            }
            let cadence = Duration::from_secs_f64(1. / f64::from(self.preferences.quality.fps()));
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
            // A compositor can suppress frame callbacks for hours. Resume with
            // one small step, never a catch-up loop or a wall-clock time jump.
            state.time = (state.time + dt.min(0.05)) % 3600.;
            if self.preferences.hybrid() {
                let aspect = (bounds.width / bounds.height.max(1.)).clamp(0.1, 10.);
                if state.density != Some(self.preferences.intensity) {
                    state.simulation = Simulation::default();
                    state.seeded = false;
                    state.density = Some(self.preferences.intensity);
                }
                if !state.seeded {
                    state.simulation.seed(aspect, &self.preferences);
                    state.seeded = true;
                }
                state.simulation.step(dt, aspect, &self.preferences);
            }
            return Some(Action::request_redraw_at(
                *now + Duration::from_secs_f64(1. / f64::from(self.preferences.quality.fps())),
            ));
        }
        None
    }
    fn draw(&self, state: &State, _: mouse::Cursor, bounds: Rectangle) -> Primitive {
        let mut drops = if self.preferences.hybrid() {
            state.simulation.instances(self.preferences.trails)
        } else {
            Vec::new()
        };
        if self.preferences.hybrid()
            && (!state.seeded || state.density != Some(self.preferences.intensity))
        {
            let mut sim = Simulation::default();
            let aspect = (bounds.width / bounds.height.max(1.)).clamp(0.1, 10.);
            sim.seed(aspect, &self.preferences);
            for _ in 0..self
                .fixed_time
                .map(|t| 60 + (t.clamp(0., 60.) * 30.) as u32)
                .unwrap_or(0)
            {
                sim.step(1. / 30., aspect, &self.preferences);
            }
            drops = sim.instances(self.preferences.trails);
        }
        Primitive {
            scene: self.scene.clone(),
            preferences: self.preferences.clone(),
            time: self.fixed_time.unwrap_or(state.time),
            drops,
        }
    }
}
#[derive(Debug)]
pub struct Primitive {
    pub scene: Arc<Scene>,
    pub preferences: Preferences,
    pub time: f32,
    pub drops: Vec<[f32; 5]>,
}
pub struct Pipeline {
    background_format: wgpu::TextureFormat,
    params: wgpu::BindGroupLayout,
    textures: wgpu::BindGroupLayout,
    mip_layout: wgpu::BindGroupLayout,
    static_pass: wgpu::RenderPipeline,
    procedural: wgpu::RenderPipeline,
    atmosphere: wgpu::RenderPipeline,
    drops: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
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
    last_field: Option<[f32; 24]>,
    dynamic_dirty: bool,
    last_drops: Vec<[f32; 5]>,
    size: [u32; 2],
    bounds: Rectangle,
    static_key: [u8; 2],
    static_dirty: bool,
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
            format: wgpu::VertexFormat::Float32,
            offset: 16,
            shader_location: 1,
        },
    ];
    let buffers = [wgpu::VertexBufferLayout {
        array_stride: 20,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("rain.wgsl").into()),
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
        let textures = texture_layout(device, 4, "Rain textures");
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
        let procedural = render_pipeline(
            device,
            &shader,
            &[&params],
            "fullscreen",
            "procedural",
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
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let drops = render_pipeline(
            device,
            &shader,
            &[&params],
            "drop_vertex",
            "drop_fragment",
            wgpu::TextureFormat::Rgba16Float,
            true,
            Some(wgpu::BlendState {
                color: add,
                alpha: add,
            }),
        );
        let composite = render_pipeline(
            device,
            &shader,
            &[&params, &textures],
            "composite_vertex",
            "composite",
            format,
            false,
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
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
            mip_layout,
            static_pass,
            procedural,
            atmosphere,
            drops,
            composite,
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
        rain: bool,
    ) -> Self {
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
            size: (super::simulation::LIMIT * 20) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fields = Fields::new(device, pipeline, &view, size, rain);
        Self {
            device: device.clone(),
            _background: background,
            uniform,
            params,
            instances,
            fields,
            rain,
            last_field: None,
            dynamic_dirty: true,
            last_drops: Vec::new(),
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
    composite: wgpu::BindGroup,
}
impl Fields {
    fn new(
        device: &wgpu::Device,
        pipeline: &Pipeline,
        view: &wgpu::TextureView,
        size: [u32; 2],
        rain: bool,
    ) -> Self {
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
        let dynamic = texture(
            device,
            "Moving wallpaper effect",
            size,
            if rain {
                wgpu::TextureFormat::Rgba16Float
            } else {
                wgpu::TextureFormat::R16Float
            },
            1,
        );
        let dynamic_view = dynamic.create_view(&Default::default());
        let composite = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Rain composition"),
            layout: &pipeline.textures,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&dynamic_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&static_view),
                },
            ],
        });
        Self {
            _static: static_texture,
            static_view,
            _dynamic: dynamic,
            dynamic_view,
            composite,
        }
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
        if slot.as_ref().is_none_or(|r| {
            r.device != *device || r.rain != (self.preferences.effect == Effect::Rain)
        }) {
            *slot = Some(Resources::new(
                device,
                queue,
                pipeline,
                &self.scene.pixels,
                size,
                self.preferences.effect == Effect::Rain,
            ));
        }
        let r = slot.as_mut().unwrap();
        if r.size != size {
            let view = r._background.create_view(&Default::default());
            r.fields = Fields::new(device, pipeline, &view, size, r.rain);
            r.size = size;
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
        // Crop changes only composition, so scrolling cannot invalidate a frozen field.
        r.dynamic_dirty |= r.last_drops != self.drops || r.last_field.as_ref() != Some(&values);
        r.last_drops.clone_from(&self.drops);
        r.last_field = Some(values);
        values[12..16].copy_from_slice(&crop);
        let mut bytes = [0u8; 96];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip(values) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        queue.write_buffer(&r.uniform, 0, &bytes);
        if !self.drops.is_empty() {
            queue.write_buffer(
                &r.instances,
                0,
                &floats(
                    self.drops
                        .iter()
                        .take(super::simulation::LIMIT)
                        .flat_map(|v| *v),
                ),
            );
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
        if r.dynamic_dirty {
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
            } else if self.preferences.mode == Mode::Procedural {
                draw.set_pipeline(&pipeline.procedural);
                draw.draw(0..3, 0..1);
            } else {
                draw.set_pipeline(&pipeline.drops);
                draw.set_vertex_buffer(0, r.instances.slice(..));
                draw.draw(
                    0..6,
                    0..self.drops.len().min(super::simulation::LIMIT) as u32,
                );
            }
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
        draw.set_pipeline(&pipeline.composite);
        draw.set_bind_group(0, &r.params, &[]);
        draw.set_bind_group(1, &r.fields.composite, &[]);
        draw.draw(0..3, 0..1);
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
            })
            .unwrap_or(0)
    }
}
#[cfg(test)]
mod optimization_tests {
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
        assert_eq!(scene.texture_bytes(), 64 * 64 * 4 + 320 * 180 * 2);
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
