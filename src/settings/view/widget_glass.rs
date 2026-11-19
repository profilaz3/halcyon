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
        let base = self.wallpaper.effect().unwrap_or(&self.config.rain);
        let id = self
            .rain
            .current_finish
            .filter(|id| base.finishes.entries.contains_key(id));
        let p = base.edit_finish(id);
        let set = |next| set(base.store_finish(next, id));
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
        if let Some(f) = id.and_then(|id| base.finishes.entries.get(&id)) {
            rows.push(label(
                format!(
                    "Editing {} · {} linked",
                    f.name,
                    base.finishes
                        .links
                        .values()
                        .filter(|v| Some(**v) == id)
                        .count()
                ),
                13.,
                MUTED,
            ));
        }
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
                    Message::Rain(Edit::Draft(base.store_finish(next, id)))
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
                    let swatch = g.color.unwrap_or_else(|| self.config.widget_theme_accent());
                    let mut tint = vec![group_row_state(
                        "Tint color",
                        if g.color.is_some() {
                            "Custom color"
                        } else {
                            "Follows theme accent"
                        },
                        self.color_button(
                            "glass-tint-color",
                            target,
                            Some(color_well::rgb(swatch)),
                            true,
                        ),
                    )];
                    if let Some(picker) = self.color_panel(target, &[]) {
                        tint.push(picker);
                    } else if g.color.is_some() {
                        let mut next = lens.clone();
                        next.glass = Some(Glass { color: None, ..g });
                        tint.push(
                            button::text("Use theme accent")
                                .id(cosmic::widget::Id::new("glass-tint-theme"))
                                .on_press(set(next))
                                .into(),
                        );
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
                || (matches!(glass.mode, GlassMode::Polished | GlassMode::Crystal)
                    && self.rain.glass_tab == GlassTab::Surface))
        {
            let next = lens.clone();
            controls.push(
                self.glass_slider("Edge refraction", "%", 100, p.refraction, move |value| {
                    let mut next = next.clone();
                    next.refraction = value;
                    Message::Rain(Edit::Draft(base.store_finish(next, id)))
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
        rows.push(
            button::text(if self.rain.library_open {
                "Close saved finishes"
            } else {
                "Saved finishes…"
            })
            .on_press(Message::Rain(Edit::Library))
            .into(),
        );
        if self.rain.library_open {
            rows.push(self.finish_library(base, id));
        }
        if enabled {
            let mut off = base.clone();
            off.enabled = false;
            rows.push(
                button::text("Turn off widget glass")
                    .on_press(Message::Rain(Edit::Set(off)))
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

    fn finish_library<'a>(&'a self, p: &'a Preferences, id: Option<u32>) -> Element<'a, Message> {
        let lib = &p.finishes;
        let name = &self.rain.finish_name;
        let action = |v| Message::Rain(v);
        let mut rows = Vec::new();
        let widget = self.rain.finish_widget;
        let mut choose = |key: Option<u32>, title: String| {
            let selected = key == id;
            let mut buttons = vec![
                button::text(if selected { "Editing" } else { "Edit" })
                    .on_press_maybe((!selected).then_some(action(Edit::SelectFinish(key))))
                    .into(),
            ];
            if let Some(w) = widget {
                let linked = lib.links.get(&w).copied() == key;
                buttons.push(
                    button::text(if linked {
                        "In use".to_string()
                    } else {
                        format!("Use on {}", w.title())
                    })
                    .on_press_maybe((!linked).then_some(action(Edit::LinkFinish(w, key))))
                    .into(),
                );
            }
            rows.push(group_row(
                &title,
                "",
                row::with_children(buttons).spacing(4).into(),
            ));
        };
        choose(None, "Shared finish".into());
        for (&key, f) in &lib.entries {
            choose(Some(key), f.name.clone());
        }
        rows.push(
            text_input("Finish name", name)
                .on_input(|v| Message::Rain(Edit::FinishName(v)))
                .into(),
        );
        let mut actions = vec![
            button::suggested(if id.is_some() {
                "Save as copy"
            } else {
                "Save finish"
            })
            .on_press_maybe(
                (lib.entries.len() < 32
                    && (lib.valid_name(name, None)
                        || id.is_some_and(|id| lib.entries[&id].name == name.trim())))
                .then_some(action(Edit::StoreFinish { copy: true })),
            )
            .into(),
        ];
        if let Some(key) = id {
            actions.push(
                button::text("Rename")
                    .on_press_maybe(
                        (lib.valid_name(name, id) && name.trim() != lib.entries[&key].name)
                            .then_some(action(Edit::StoreFinish { copy: false })),
                    )
                    .into(),
            );
            actions.push(
                button::text(if self.rain.confirm_remove {
                    "Confirm removal"
                } else {
                    "Remove…"
                })
                .on_press(action(Edit::RemoveFinish))
                .into(),
            );
            if self.rain.confirm_remove {
                let n = lib.links.values().filter(|v| **v == key).count();
                rows.push(label(
                    format!(
                        "{n} linked widgets will use the shared finish. You can undo this change."
                    ),
                    13.,
                    MUTED,
                ));
            }
        }
        rows.push(row::with_children(actions).spacing(6).into());
        group(Some("Saved finishes"), rows)
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
        let _finish = crate::widget_style::for_glass_preview(prefs.active_glass());
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
                theme_accent: cfg.widget_theme_accent(),
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
        use crate::tools::cards;
        let size = Some([cw, ch]);
        let target = self.rain.finish_widget.unwrap_or(Widget::Messages);
        let card: Element<'_, Message> = match target {
            Widget::Music => {
                let now = std::time::Instant::now();
                let (feed, player) = cards::recipe::paused_sample();
                cards::music(
                    cards::MusicView {
                        feed: Some(feed),
                        player: Some(player),
                        art: Some(cards::recipe::sample_art()),
                        now,
                        manually_selected: false,
                        problem: false,
                        last: None,
                    },
                    &cfg,
                    None,
                    size,
                )
            }
            Widget::Folder => {
                let sample = crate::folder::sample();
                cfg.folder.path = Some(sample.source.clone());
                cfg.folder.show_hidden = false;
                cards::folder(&sample, &cfg, [cw, ch], |_| None, None)
            }
            Widget::Weather => {
                crate::app::settings_weather_preview_sized(&cfg, [cw, ch]).map(|_| Message::Preview)
            }
            Widget::Clock => {
                crate::app::settings_clock_preview(&cfg, [cw, ch]).map(|_| Message::Preview)
            }
            Widget::Devbox => crate::app::settings_devbox_preview(&cfg).map(|_| Message::Preview),
            Widget::Projects => {
                crate::app::settings_projects_preview(&cfg).map(|_| Message::Preview)
            }
            Widget::Markets => cards::markets(&self.tools.market_preview, &cfg, None, size),
            Widget::Devices => cards::devices(&cards::sample_devices(), &cfg, size),
            Widget::Focus => cards::focus(
                &Default::default(),
                &cfg,
                0,
                "",
                None::<fn(cards::Action) -> Message>,
                size,
            ),
            Widget::Calendar => self.samples.calendar_view().map(|_| Message::Preview),
            Widget::Glance => self.samples.overview_view().map(|_| Message::Preview),
            Widget::Notes => {
                static SAMPLE: std::sync::OnceLock<crate::tools::notes::Note> =
                    std::sync::OnceLock::new();
                let note = SAMPLE.get_or_init(|| {
                    let mut n = crate::tools::notes::Note::default();
                    n.text = "Plan the next studio session".into();
                    n
                });
                cards::notes(
                    cards::NotesView {
                        note: Ok(note),
                        editing: None,
                        paused: false,
                        problem: None,
                        discarded: false,
                    },
                    &cfg,
                    None,
                    size,
                )
            }
            Widget::Messages => {
                cards::messages::<Message>(&crate::messages::sample(), &cfg, None, size)
            }
        };
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
