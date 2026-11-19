//! One staged finish for the native card and the optical pane behind it.
use super::*;
use crate::rain::{Effect, Glass, GlassMode, Preferences};
use crate::settings::{
    color_well,
    rain::{Edit, GlassTab},
};
use cosmic::iced::widget::Stack;
use cosmic::widget::image::Handle;

fn set(p: Preferences) -> Message {
    Message::Rain(Edit::Set(p))
}

impl Settings {
    pub(super) fn widget_glass_editor(&self) -> Element<'_, Message> {
        let p = self.wallpaper.effect().unwrap_or(&self.config.rain);
        let enabled = p.enabled && p.effect == Effect::Lens;
        let mut lens = p.clone();
        lens.enabled = true;
        lens.effect = Effect::Lens;
        let mut rows = vec![heading(
            "Widget glass",
            "Choose how the wallpaper looks through your desktop widgets.",
        )];
        if let Some(error) = self.wallpaper.unavailable() {
            rows.push(desktop::banner(error.to_owned(), desktop::WARN, vec![]));
            return column::with_children(rows).spacing(12).into();
        }
        let wide = self.content_width() >= 710.;
        let width = if wide {
            (self.content_width() * 0.46).min(480.)
        } else {
            (self.content_width() - 32.).clamp(180., 520.)
        };
        let preview = self.glass_sample(&lens, width);
        let glass = lens.glass.unwrap_or_default();
        let choices = std::iter::once(None)
            .chain(GlassMode::ALL.map(Some))
            .map(|mode| {
                let mut next = lens.clone();
                next.glass = mode.map(|mode| {
                    if glass.mode == mode {
                        glass
                    } else {
                        glass.preset(mode)
                    }
                });
                (
                    mode.map_or("Classic", GlassMode::title).into(),
                    enabled && p.glass.map(|g| g.mode) == mode,
                    Some(set(next)),
                )
            })
            .collect();
        rows.push(segmented(choices, self.settings_accent()));
        if p.weather {
            rows.push(label(
                "Match the weather can replace this finish while a weather effect is playing.",
                12.,
                MUTED,
            ));
        }
        let mut controls = Vec::new();
        if let Some(g) = p.active_glass() {
            let tabs = [
                (GlassTab::Surface, "Surface"),
                (GlassTab::Tint, "Tint"),
                (GlassTab::Shape, "Shape"),
            ];
            controls.push(segmented(
                tabs.into_iter()
                    .map(|(tab, name)| {
                        (
                            name.to_owned(),
                            self.rain.glass_tab == tab,
                            Some(Message::Rain(Edit::GlassTab(tab))),
                        )
                    })
                    .collect(),
                self.settings_accent(),
            ));
            let make_slider = |title, unit, max, value, change: fn(&mut Glass, u8)| {
                let next = lens.clone();
                self.glass_slider(title, unit, max, value, move |value| {
                    let mut next = next.clone();
                    if let Some(glass) = &mut next.glass {
                        change(glass, value);
                    }
                    Message::Rain(Edit::Draft(next))
                })
            };
            match self.rain.glass_tab {
                GlassTab::Surface => controls.push(group(
                    None,
                    vec![
                        make_slider("Frost level", "%", 100, g.frost, |g, n| g.frost = n),
                        make_slider("Backdrop blur", " px", 32, g.blur, |g, n| g.blur = n),
                        make_slider("Surface opacity", "%", 100, g.opacity, |g, n| g.opacity = n),
                    ],
                )),
                GlassTab::Tint => {
                    let target = color_well::Target::GlassTint;
                    let mut tint = vec![group_row(
                        "Tint color",
                        "",
                        self.color_button(
                            "glass-tint-color",
                            target,
                            Some(color_well::rgb(g.color)),
                            true,
                        ),
                    )];
                    if let Some(picker) = self.color_panel(target, &[]) {
                        tint.push(picker);
                    }
                    tint.push(make_slider("Tint strength", "%", 100, g.tint, |g, n| {
                        g.tint = n
                    }));
                    controls.push(group(None, tint));
                }
                GlassTab::Shape => {
                    let mut style_config = self.config.clone();
                    style_config.rain.glass = None;
                    let style_radius = crate::widget_style::corner_radius(&style_config) as u8;
                    let mut shape = vec![make_slider(
                        "Corner radius",
                        " px",
                        32,
                        g.radius.unwrap_or(style_radius),
                        |g, n| g.radius = Some(n),
                    )];
                    if g.radius.is_some() {
                        let mut next = lens.clone();
                        next.glass = Some(Glass { radius: None, ..g });
                        shape.push(button::text("Use style corners").on_press(set(next)).into());
                    }
                    controls.push(group(None, shape));
                }
            }
        }
        if enabled
            && (p.glass.is_none()
                || (glass.mode == GlassMode::Polished && self.rain.glass_tab == GlassTab::Surface))
        {
            let next = lens.clone();
            controls.push(
                self.glass_slider("Edge refraction", "%", 100, p.refraction, move |value| {
                    let mut next = next.clone();
                    next.refraction = value;
                    Message::Rain(Edit::Draft(next))
                }),
            );
        }
        if wide && !controls.is_empty() {
            rows.push(
                row::with_children(vec![
                    preview,
                    column::with_children(controls)
                        .spacing(10)
                        .width(Length::Fill)
                        .into(),
                ])
                .spacing(16)
                .into(),
            );
        } else {
            rows.push(preview);
            rows.extend(controls);
        }
        if enabled {
            let mut off = p.clone();
            off.enabled = false;
            rows.push(
                button::text("Turn off widget glass")
                    .on_press(set(off))
                    .into(),
            );
        }
        rows.push(
            button::text("Weather effects")
                .on_press(Message::SearchResult(
                    crate::settings::navigation::WEATHER_EFFECTS,
                ))
                .into(),
        );
        column::with_children(rows).spacing(14).into()
    }

    fn glass_slider<'a>(
        &self,
        title: &'static str,
        unit: &str,
        max: u8,
        value: u8,
        change: impl Fn(u8) -> Message + 'a,
    ) -> Element<'a, Message> {
        let commit = Message::Rain(Edit::Commit);
        column::with_children(vec![
            split(
                label(title, 13., FG),
                label(format!("{value}{unit}"), 12., MUTED),
            ),
            slider(0..=max, value, change)
                .on_release(commit.clone())
                .width(Length::Fill)
                .apply(|v| completed_slider(v.into(), commit)),
        ])
        .spacing(4)
        .into()
    }

    fn glass_sample(&self, prefs: &Preferences, w: f32) -> Element<'_, Message> {
        static SAMPLE: std::sync::OnceLock<Handle> = std::sync::OnceLock::new();
        let fallback = || {
            SAMPLE.get_or_init(|| {
                let image = ::image::load_from_memory(include_bytes!(
                    "../../../assets/looks/evening-wallpaper.jpg"
                ))
                .expect("bundled glass preview wallpaper")
                .thumbnail(800, 500)
                .into_rgba8();
                Handle::from_rgba(image.width(), image.height(), image.into_raw())
            })
        };
        let handle = self
            .wallpaper
            .shown()
            .handle
            .as_ref()
            .filter(|h| matches!(h, Handle::Rgba { .. }))
            .unwrap_or_else(fallback);
        let h = 198.;
        let cw = (w - 36.).min(320.);
        let ch = 160.;
        let mut cfg = self.config.clone();
        cfg.rain = prefs.clone();
        let mut layers = vec![
            cosmic::widget::image(handle.clone())
                .content_fit(cosmic::iced::ContentFit::Fill)
                .width(w)
                .height(h)
                .into(),
        ];
        if let Some(scene) = self.rain.scene(handle) {
            scene.set_panes(crate::rain::gpu::Panes {
                rects: vec![[(w - cw) / (2. * w), (h - ch) / (2. * h), cw / w, ch / h]],
                radius: crate::widget_style::corner_radius(&cfg) / h,
                ..Default::default()
            });
            layers.push(
                cosmic::iced::widget::Shader::new(crate::rain::gpu::Program {
                    scene,
                    preferences: prefs.clone(),
                    running: false,
                    fixed_time: Some(0.),
                    fade: Default::default(),
                })
                .width(w)
                .height(h)
                .into(),
            );
        }
        let card = crate::tools::cards::messages::<Message>(
            &crate::messages::sample(),
            &cfg,
            None,
            Some([cw, ch]),
        );
        layers.push(
            container(container(card).width(cw))
                .center_x(w)
                .center_y(h)
                .into(),
        );
        Stack::with_children(layers)
            .width(w)
            .height(h)
            .clip(true)
            .into()
    }
}
