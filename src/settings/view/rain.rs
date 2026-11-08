//! Wallpaper and effects share one local preview and Apply action.
use super::*;
use crate::rain::weather::{self, Source};
use crate::rain::{Effect, FogColor, Mode, Preferences, Quality};
use crate::settings::rain::Edit;
fn edit(e: Edit) -> Message {
    Message::Rain(e)
}
impl Settings {
    /// What the desktop shows for `p`, the same way it decides: the Weather
    /// widget's cached reading while Match the weather is on.
    pub(super) fn effect_scene(&self, p: &Preferences) -> weather::Scene {
        let reading = (self.config.data_visible(Widget::Weather))
            .then_some(self.rain.weather.as_ref())
            .flatten();
        weather::resolve(
            p,
            reading,
            chrono::Local::now(),
            weather::cutoff(self.config.refresh_intervals.duration(Widget::Weather)),
        )
    }
    /// Match the weather's state line: what the weather shows now, or why
    /// Moving effect plays instead.
    fn weather_state(&self, p: &Preferences, scene: &weather::Scene) -> String {
        if !p.weather {
            return "Rain, snow, fog and cloud shadows follow the Weather widget".into();
        }
        match scene.source {
            Source::Weather { code, play } => {
                let sky = crate::data::weather_description(code);
                match play {
                    Some((effect, _)) => {
                        format!("{sky}: showing {}", effect.title().to_lowercase())
                    }
                    None => format!("{sky}: no effect"),
                }
            }
            Source::NoReading if !self.config.data_visible(Widget::Weather) => {
                "Turn on the Weather widget to use this".into()
            }
            Source::NoReading => "No weather reading yet: using Moving effect".into(),
            Source::Stale => "Weather reading is out of date: using Moving effect".into(),
            Source::Unknown(_) => "Unrecognized weather: using Moving effect".into(),
            Source::Manual => String::new(),
        }
    }
    pub(super) fn rain_controls(&self) -> Element<'_, Message> {
        let p = self.wallpaper.effect().unwrap_or(&self.config.rain);
        let scene = self.effect_scene(p);
        // The controls adjust what plays: the weather's effect while it
        // chooses (its strength is a share of these), otherwise Moving effect.
        let weather_plays = scene.from_weather() && scene.preferences.enabled;
        let effect = if weather_plays {
            scene.preferences.effect
        } else {
            p.effect
        };
        const EFFECTS: &[Effect] = &[Effect::Rain, Effect::Snow, Effect::Fog, Effect::Clouds];
        const TITLES: &[&str] = &[
            "None",
            Effect::Rain.title(),
            Effect::Snow.title(),
            Effect::Fog.title(),
            Effect::Clouds.title(),
        ];
        // One choice: no effect, or which one. Choosing applies it.
        let next = p.clone();
        let chosen = if p.enabled {
            EFFECTS.iter().position(|e| *e == p.effect).map(|i| i + 1)
        } else {
            Some(0)
        };
        let mut rows = vec![split(
            detail(
                "Moving effect",
                if p.weather {
                    "Plays behind windows and widgets when there’s no current weather reading"
                } else {
                    "Plays behind windows and widgets"
                },
            ),
            cosmic::widget::dropdown(TITLES, chosen, move |i| {
                let mut p = next.clone();
                p.enabled = i > 0;
                if i > 0 {
                    p.effect = EFFECTS[i - 1];
                }
                edit(Edit::Set(p))
            })
            .width(160)
            .into(),
        )];
        let next = p.clone();
        rows.push(split(
            detail_state("Match the weather", self.weather_state(p, &scene)),
            container(toggler(p.weather).on_toggle(move |on| {
                let mut p = next.clone();
                p.weather = on;
                edit(Edit::Set(p))
            }))
            .id(cosmic::widget::Id::new("rain-weather"))
            .into(),
        ));
        if self.wallpaper.effect_conflict() {
            rows.push(label(
                "Effects changed elsewhere. Choose which version to keep.",
                12.,
                desktop::ERROR,
            ));
            rows.push(
                cosmic::widget::flex_row(vec![
                    button::text("Use current")
                        .on_press(Message::Wallpaper(
                            crate::settings::wallpaper::Message::ResolveEffect(false),
                        ))
                        .into(),
                    button::text("Keep my preview")
                        .on_press(Message::Wallpaper(
                            crate::settings::wallpaper::Message::ResolveEffect(true),
                        ))
                        .into(),
                ])
                .spacing(6)
                .into(),
            );
        }
        if p.enabled || weather_plays || self.rain.preview {
            if effect == Effect::Rain {
                let style_help=self.help("rain-style-help","Droplet motion","Flowing uses a procedural GPU shader. Merging simulates large drops that join and leave small trails. Both use your wallpaper and stay behind widgets, windows and the dock.");
                let style_choices = segmented(
                    [(Mode::Procedural, "Flowing"), (Mode::Hybrid, "Merging")]
                        .into_iter()
                        .map(|(mode, title)| {
                            let mut next = p.clone();
                            next.mode = mode;
                            (title.into(), p.mode == mode, Some(edit(Edit::Set(next))))
                        })
                        .collect(),
                    self.settings_accent(),
                );
                rows.push(
                    column::with_children(vec![style_help, style_choices])
                        .spacing(8)
                        .into(),
                );
            }
            let mut adjustments = Vec::new();
            let controls = match effect {
                Effect::Rain => vec![
                    ("intensity", "Density", p.intensity),
                    ("trails", "Trails", p.trails),
                    ("size", "Drop size", p.size),
                    ("speed", "Speed", p.speed),
                    ("mist", "Mist", p.mist),
                    ("refraction", "Refraction", p.refraction),
                ],
                Effect::Snow => vec![
                    ("intensity", "Density", p.snow.density),
                    ("size", "Flake size", p.snow.size),
                    ("speed", "Speed", p.snow.speed),
                    ("opacity", "Opacity", p.snow.opacity),
                ],
                Effect::Fog => vec![
                    ("opacity", "Opacity", p.fog.opacity),
                    ("height", "Height", p.fog.height),
                    ("speed", "Speed", p.fog.speed),
                ],
                Effect::Clouds => vec![
                    ("intensity", "Coverage", p.clouds.coverage),
                    ("opacity", "Shadow strength", p.clouds.strength),
                    ("softness", "Softness", p.clouds.softness),
                    ("speed", "Speed", p.clouds.speed),
                ],
            };
            for (key, title, value) in controls {
                let next = p.clone();
                let control = column::with_children(vec![
                    split(
                        if key == "trails" {
                            self.help("rain-trails-help", title, "How much moving drops leave behind. Lower values keep the glass cleaner. Set to 0% for no trails.")
                        } else {
                            label(title, 13., FG)
                        },
                        label(if key == "trails" && value == 0 { "Off".into() } else { format!("{value}%") }, 12., MUTED),
                    ),
                    container(
                        slider(0..=100, value, move |value| {
                            let mut p = next.clone();
                            match effect {
                                Effect::Rain=>match key { "intensity"=>p.intensity=value, "trails"=>p.trails=value, "size"=>p.size=value, "speed"=>p.speed=value,"mist"=>p.mist=value,_=>p.refraction=value },
                                Effect::Snow=>match key { "intensity"=>p.snow.density=value,"size"=>p.snow.size=value,"speed"=>p.snow.speed=value,_=>p.snow.opacity=value },
                                Effect::Fog=>match key { "opacity"=>p.fog.opacity=value,"height"=>p.fog.height=value,_=>p.fog.speed=value },
                                Effect::Clouds=>match key { "intensity"=>p.clouds.coverage=value,"opacity"=>p.clouds.strength=value,"softness"=>p.clouds.softness=value,_=>p.clouds.speed=value },
                            }
                            edit(Edit::Draft(p))
                        })
                        .on_release(edit(Edit::Commit))
                        .width(Length::Fill)
                        .apply(|v| completed_slider(v.into(), edit(Edit::Commit))),
                    )
                    .id(cosmic::widget::Id::new(format!("rain-{key}")))
                    .into(),
                ])
                .spacing(5)
                .into();
                if matches!(key, "intensity" | "trails" | "opacity" | "height")
                    || (effect == Effect::Snow && key == "size")
                {
                    rows.push(control);
                } else {
                    adjustments.push(control);
                }
            }
            if effect == Effect::Fog {
                rows.push(self.help("fog-color-help","Color", "A neutral, cool or warm tint for the drifting fog. The wallpaper stays unchanged."));
                rows.push(segmented(
                    [
                        (FogColor::Neutral, "Neutral"),
                        (FogColor::Cool, "Cool"),
                        (FogColor::Warm, "Warm"),
                    ]
                    .into_iter()
                    .map(|(color, title)| {
                        let mut next = p.clone();
                        next.fog.color = color;
                        (
                            title.into(),
                            p.fog.color == color,
                            Some(edit(Edit::Set(next))),
                        )
                    })
                    .collect(),
                    self.settings_accent(),
                ));
            }
            let wind = match effect {
                Effect::Rain => p.wind,
                Effect::Snow => p.snow.wind,
                Effect::Fog => p.fog.wind,
                Effect::Clouds => p.clouds.wind,
            };
            let next = p.clone();
            adjustments.push(
                column::with_children(vec![
                    split(
                        label("Wind", 13., FG),
                        label(
                            if wind == 0 {
                                "Still".into()
                            } else {
                                format!(
                                    "{} {}%",
                                    if wind < 0 { "←" } else { "→" },
                                    wind.unsigned_abs()
                                )
                            },
                            12.,
                            MUTED,
                        ),
                    ),
                    slider(-100..=100, i16::from(wind), move |value| {
                        let mut p = next.clone();
                        match effect {
                            Effect::Rain => p.wind = value as i8,
                            Effect::Snow => p.snow.wind = value as i8,
                            Effect::Fog => p.fog.wind = value as i8,
                            Effect::Clouds => p.clouds.wind = value as i8,
                        }
                        edit(Edit::Draft(p))
                    })
                    .on_release(edit(Edit::Commit))
                    .width(Length::Fill)
                    .apply(|v| completed_slider(v.into(), edit(Edit::Commit))),
                ])
                .spacing(5)
                .into(),
            );
            adjustments.push(self.help("rain-quality-help","Rendering","Eco uses a smaller effect texture at 30 fps. Balanced uses a sharper field at 30 fps. Smooth uses the same field at 60 fps. The wallpaper stays sharp. Speed at 0% freezes snow, fog and clouds. Animation pauses with the lock screen, sleep and Reduce motion."));
            adjustments.push(segmented(
                [
                    (Quality::Eco, "Eco"),
                    (Quality::Balanced, "Balanced"),
                    (Quality::Smooth, "Smooth"),
                ]
                .into_iter()
                .map(|(quality, title)| {
                    let mut next = p.clone();
                    next.quality = quality;
                    (
                        title.into(),
                        p.quality == quality,
                        Some(edit(Edit::Set(next))),
                    )
                })
                .collect(),
                self.settings_accent(),
            ));
            let next = p.clone();
            adjustments.push(switch(
                "Pause on battery",
                "",
                p.pause_on_battery,
                move |on| {
                    let mut p = next.clone();
                    p.pause_on_battery = on;
                    edit(Edit::Set(p))
                },
            ));
            rows.push(self.disclosure(
                "rain-adjust",
                if effect == Effect::Rain {
                    "Adjust rain"
                } else {
                    "Adjust effect"
                },
                column::with_children(adjustments).spacing(14).into(),
            ));
            let state = self.rain.lifecycle;
            if !self.config.material_motion {
                rows.push(label("Paused while Reduce motion is on", 12., MUTED));
            } else if !state.ready {
                rows.push(label("Waiting for the desktop session", 12., MUTED));
            } else if p.pause_on_battery && state.on_battery {
                rows.push(label("Paused on battery", 12., MUTED));
            }
        }
        container(group(None, rows))
            .id(cosmic::widget::Id::new("rain-controls"))
            .into()
    }
    pub(super) fn rain_preview(&self, height: f32) -> Option<Element<'_, Message>> {
        // The preview shows what the desktop shows, weather included.
        let scene = self.effect_scene(self.wallpaper.effect().unwrap_or(&self.config.rain));
        let p = &scene.preferences;
        if !p.enabled && !self.rain.preview {
            self.rain.release_image();
            return None;
        }
        let mut active = p.clone();
        active.enabled = true;
        if !active.active(self.config.material_motion, self.rain.lifecycle) {
            self.rain.release_image();
            return None;
        }
        let handle = self.wallpaper.shown().handle.as_ref()?;
        let scene = self.rain.scene(handle)?;
        Some(crate::rain::visibility::visible(
            cosmic::iced::widget::Shader::new(crate::rain::gpu::Program {
                scene,
                preferences: p.clone(),
                running: self.config.material_motion,
                fixed_time: None,
                fade: Default::default(),
            })
            .width(Length::Fill)
            .height(height)
            .into(),
        ))
    }
}
