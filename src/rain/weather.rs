//! Weather scenes: an opt-in mapping from the Weather widget's own reading to
//! the existing effects. Nothing is fetched for the effect. While Match the
//! weather is on and the reading is current, the weather decides, including
//! "no effect" under calm skies. A missing, old or unknown reading hands
//! control back to the person's own choice.
use super::{Effect, Preferences};
use chrono::{DateTime, Duration, FixedOffset, Local};
use serde::{Deserialize, Serialize};

/// How strong the reported weather is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Light,
    Moderate,
    Heavy,
}

/// Weather strength: how far light and moderate weather turn the person's
/// own setting down. Heavy weather is always their full setting.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    Subtle,
    #[default]
    Natural,
    Strong,
}
impl Strength {
    pub const ALL: [Self; 3] = [Self::Subtle, Self::Natural, Self::Strong];
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub const fn title(self) -> &'static str {
        match self {
            Self::Subtle => "Subtle",
            Self::Natural => "Natural",
            Self::Strong => "Strong",
        }
    }
    /// The share of the person's own setting. Their setting is the ceiling:
    /// weather never makes an effect stronger than they chose.
    pub const fn percent(self, level: Level) -> u16 {
        match (self, level) {
            (_, Level::Heavy) => 100,
            (Self::Subtle, Level::Light) => 25,
            (Self::Subtle, Level::Moderate) => 50,
            (Self::Natural, Level::Light) => 40,
            (Self::Natural, Level::Moderate) => 70,
            (Self::Strong, Level::Light) => 70,
            (Self::Strong, Level::Moderate) => 85,
        }
    }
}

/// What a WMO weather code (the Weather widget's `weather_code`) asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// Rain, snow, fog or a partly cloudy sky: this effect at this level.
    Play(Effect, Level),
    /// Clear, mostly clear or overcast: no effect.
    Calm,
    /// A code this table doesn't know. Treated like no reading.
    Unknown,
}

/// The mapping table. Drizzle, freezing rain and thunderstorms show as rain;
/// snow grains and snow showers as snow. Overcast skies cast no shadows.
pub const fn condition(code: u32) -> Condition {
    use Condition::{Calm, Play, Unknown};
    use Effect::{Clouds, Fog, Rain, Snow};
    use Level::{Heavy, Light, Moderate};
    match code {
        0 | 1 | 3 => Calm,
        2 => Play(Clouds, Moderate),
        45 => Play(Fog, Moderate),
        48 => Play(Fog, Heavy),
        51 | 53 | 56 | 61 | 80 => Play(Rain, Light),
        55 | 57 | 63 | 66 | 81 => Play(Rain, Moderate),
        65 | 67 | 82 | 95 | 96 | 99 => Play(Rain, Heavy),
        71 | 77 | 85 => Play(Snow, Light),
        73 => Play(Snow, Moderate),
        75 | 86 => Play(Snow, Heavy),
        _ => Unknown,
    }
}

/// A reading stops describing the sky after two refresh intervals (one
/// missed refresh), and never later than three hours.
pub fn cutoff(refresh: std::time::Duration) -> Duration {
    Duration::from_std(refresh.saturating_mul(2))
        .unwrap_or(Duration::MAX)
        .min(Duration::hours(3))
}

/// Clocks drift; a reading this far ahead of the local clock is not trusted.
const FUTURE_TOLERANCE: Duration = Duration::minutes(5);

/// The current condition from the Weather widget's reading for its city.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub code: u32,
    pub updated_at: DateTime<FixedOffset>,
}
impl Observation {
    pub fn from_snapshot(snapshot: &crate::data::WeatherSnapshot) -> Option<Self> {
        Some(Self {
            code: snapshot.weather.current.weather_code,
            updated_at: DateTime::parse_from_rfc3339(&snapshot.updated_at).ok()?,
        })
    }
}

/// Why the scene is what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Match the weather is off: the person's choice.
    Manual,
    /// On, but there is no reading for the Weather city: the person's choice.
    NoReading,
    /// On, but the reading is older than the cutoff: the person's choice.
    Stale,
    /// On, but the reading's code isn't in the table: the person's choice.
    Unknown(u32),
    /// The weather chose. `None` is a calm sky with no effect.
    Weather {
        code: u32,
        play: Option<(Effect, Level)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// What to render. Pause, lock, battery and Reduce motion still apply.
    pub preferences: Preferences,
    pub source: Source,
    /// When the reading driving this scene stops being current.
    pub until: Option<DateTime<FixedOffset>>,
}
impl Scene {
    /// The weather's level, part of what a viewer sees: a change fades.
    pub fn level(&self) -> Option<Level> {
        match self.source {
            Source::Weather {
                play: Some((_, level)),
                ..
            } => Some(level),
            _ => None,
        }
    }
    pub fn from_weather(&self) -> bool {
        matches!(self.source, Source::Weather { .. })
    }
}

/// `manual` with `effect` playing at `level` of the person's own setting,
/// as their Weather strength sets it.
pub fn apply(manual: &Preferences, effect: Effect, level: Level) -> Preferences {
    let percent = manual.weather_strength.percent(level);
    // Rounded up, so a visible setting stays visible; never above the setting.
    let scale = |value: u8| (u16::from(value) * percent).div_ceil(100) as u8;
    let mut p = manual.clone();
    p.enabled = true;
    p.effect = effect;
    match effect {
        Effect::Rain => p.intensity = scale(p.intensity),
        Effect::Snow => p.snow.density = scale(p.snow.density),
        Effect::Fog => p.fog.opacity = scale(p.fog.opacity),
        Effect::Clouds => p.clouds.coverage = scale(p.clouds.coverage),
    }
    p
}

/// The effect to show now. The weather wins only while Match the weather is
/// on and its reading is current; otherwise the person's choice plays.
pub fn resolve(
    manual: &Preferences,
    reading: Option<&Observation>,
    now: DateTime<Local>,
    cutoff: Duration,
) -> Scene {
    let fallback = |source| Scene {
        preferences: manual.clone(),
        source,
        until: None,
    };
    if !manual.weather {
        return fallback(Source::Manual);
    }
    let Some(reading) = reading else {
        return fallback(Source::NoReading);
    };
    let age = now.fixed_offset().signed_duration_since(reading.updated_at);
    if age >= cutoff || age < -FUTURE_TOLERANCE {
        return fallback(Source::Stale);
    }
    let until = Some(reading.updated_at + cutoff);
    match condition(reading.code) {
        Condition::Unknown => fallback(Source::Unknown(reading.code)),
        Condition::Calm => Scene {
            preferences: Preferences {
                enabled: false,
                ..manual.clone()
            },
            source: Source::Weather {
                code: reading.code,
                play: None,
            },
            until,
        },
        Condition::Play(effect, level) => Scene {
            preferences: apply(manual, effect, level),
            source: Source::Weather {
                code: reading.code,
                play: Some((effect, level)),
            },
            until,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rain::lifecycle;

    fn at(minutes_ago: i64) -> Observation {
        Observation {
            code: 0,
            updated_at: (Local::now() - Duration::minutes(minutes_ago)).fixed_offset(),
        }
    }
    fn reading(code: u32, minutes_ago: i64) -> Observation {
        Observation {
            code,
            ..at(minutes_ago)
        }
    }
    fn opted_in() -> Preferences {
        Preferences {
            weather: true,
            ..Default::default()
        }
    }
    const HALF_HOUR: std::time::Duration = std::time::Duration::from_secs(1800);

    #[test]
    fn weather_mapping_table_covers_every_reported_code() {
        use Condition::*;
        use Effect::*;
        use Level::*;
        let table = [
            (0, Calm),
            (1, Calm),
            (2, Play(Clouds, Moderate)),
            (3, Calm),
            (45, Play(Fog, Moderate)),
            (48, Play(Fog, Heavy)),
            (51, Play(Rain, Light)),
            (53, Play(Rain, Light)),
            (55, Play(Rain, Moderate)),
            (56, Play(Rain, Light)),
            (57, Play(Rain, Moderate)),
            (61, Play(Rain, Light)),
            (63, Play(Rain, Moderate)),
            (65, Play(Rain, Heavy)),
            (66, Play(Rain, Moderate)),
            (67, Play(Rain, Heavy)),
            (71, Play(Snow, Light)),
            (73, Play(Snow, Moderate)),
            (75, Play(Snow, Heavy)),
            (77, Play(Snow, Light)),
            (80, Play(Rain, Light)),
            (81, Play(Rain, Moderate)),
            (82, Play(Rain, Heavy)),
            (85, Play(Snow, Light)),
            (86, Play(Snow, Heavy)),
            (95, Play(Rain, Heavy)),
            (96, Play(Rain, Heavy)),
            (99, Play(Rain, Heavy)),
        ];
        for (code, expected) in table {
            assert_eq!(condition(code), expected, "code {code}");
            // Every mapped code has a Weather widget description.
            assert_ne!(crate::data::weather_description(code), "Weather update");
        }
        for code in [4, 44, 50, 70, 90, 100, u32::MAX] {
            assert_eq!(condition(code), Unknown, "code {code}");
        }
    }

    #[test]
    fn weather_intensity_never_exceeds_the_persons_setting() {
        let mut manual = opted_in();
        manual.intensity = 50;
        manual.snow.density = 1;
        manual.fog.opacity = 0;
        manual.clouds.coverage = 100;
        for level in [Level::Light, Level::Moderate, Level::Heavy] {
            let rain = apply(&manual, Effect::Rain, level);
            assert!(rain.intensity <= 50 && rain.intensity > 0);
            assert_eq!(rain.intensity, [20, 35, 50][level as usize]);
            // A visible setting stays visible; zero stays zero.
            assert_eq!(apply(&manual, Effect::Snow, level).snow.density, 1);
            assert_eq!(apply(&manual, Effect::Fog, level).fog.opacity, 0);
            assert!(!apply(&manual, Effect::Fog, level).has_visible_effect());
            let clouds = apply(&manual, Effect::Clouds, level);
            assert_eq!(clouds.clouds.coverage, [40, 70, 100][level as usize]);
            // Only the chosen effect's strength changes; its other controls,
            // and every other effect's settings, are the person's own.
            assert_eq!(clouds.clouds.strength, manual.clouds.strength);
            assert_eq!(clouds.snow, manual.snow);
            assert_eq!(clouds.quality, manual.quality);
            assert_eq!(clouds.pause_on_battery, manual.pause_on_battery);
            assert!(clouds.validate().is_ok());
        }
    }

    #[test]
    fn weather_strength_sets_the_share_for_each_level() {
        use Level::*;
        use Strength::*;
        let expected = [
            (Subtle, [25, 50, 100]),
            (Natural, [40, 70, 100]),
            (Strong, [70, 85, 100]),
        ];
        for (strength, percents) in expected {
            let mut manual = opted_in();
            manual.weather_strength = strength;
            manual.intensity = 100;
            manual.snow.density = 100;
            manual.fog.opacity = 100;
            manual.clouds.coverage = 100;
            for (level, percent) in [Light, Moderate, Heavy].into_iter().zip(percents) {
                assert_eq!(strength.percent(level), percent, "{strength:?} {level:?}");
                let at = |effect| apply(&manual, effect, level);
                assert_eq!(u16::from(at(Effect::Rain).intensity), percent);
                assert_eq!(u16::from(at(Effect::Snow).snow.density), percent);
                assert_eq!(u16::from(at(Effect::Fog).fog.opacity), percent);
                assert_eq!(u16::from(at(Effect::Clouds).clouds.coverage), percent);
            }
            // Heavy weather is always the person's full setting.
            assert_eq!(apply(&manual, Effect::Rain, Heavy).intensity, 100);
        }
    }

    #[test]
    fn weather_strength_rounds_up_and_never_exceeds_the_setting() {
        use Level::*;
        use Strength::*;
        for strength in Strength::ALL {
            let mut manual = opted_in();
            manual.weather_strength = strength;
            for value in 0..=100u8 {
                manual.intensity = value;
                for level in [Light, Moderate, Heavy] {
                    let scaled = apply(&manual, Effect::Rain, level).intensity;
                    assert!(scaled <= value, "{strength:?} {level:?} {value}");
                    // A visible setting stays visible; zero stays zero.
                    assert_eq!(scaled == 0, value == 0, "{strength:?} {level:?} {value}");
                    let exact = u32::from(value) * u32::from(strength.percent(level));
                    assert_eq!(u32::from(scaled), exact.div_ceil(100));
                }
            }
        }
        // 45 at 25% is 11.25, rounded up to 12; at 85% 38.25 is 39.
        let mut manual = opted_in();
        manual.intensity = 45;
        manual.weather_strength = Subtle;
        assert_eq!(apply(&manual, Effect::Rain, Light).intensity, 12);
        manual.weather_strength = Strong;
        assert_eq!(apply(&manual, Effect::Rain, Moderate).intensity, 39);
        manual.intensity = 1;
        manual.weather_strength = Subtle;
        assert_eq!(apply(&manual, Effect::Rain, Light).intensity, 1);
    }

    #[test]
    fn weather_strength_defaults_to_natural_and_round_trips() {
        // Configs written before Weather strength load as Natural, and
        // Natural is left out so they're written back unchanged.
        let old: Preferences = serde_json::from_str(r#"{"weather":true,"intensity":60}"#).unwrap();
        assert_eq!(old.weather_strength, Strength::Natural);
        assert!(
            !serde_json::to_string(&old)
                .unwrap()
                .contains("weather_strength")
        );
        assert_eq!(Preferences::default().weather_strength, Strength::Natural);
        for (strength, key) in [(Strength::Subtle, "subtle"), (Strength::Strong, "strong")] {
            let p = Preferences {
                weather_strength: strength,
                ..old.clone()
            };
            let encoded = serde_json::to_string(&p).unwrap();
            assert!(encoded.contains(&format!(r#""weather_strength":"{key}""#)));
            assert_eq!(serde_json::from_str::<Preferences>(&encoded).unwrap(), p);
        }
        assert!(serde_json::from_str::<Preferences>(r#"{"weather_strength":"wild"}"#).is_err());
        // Resolving the scene uses it.
        let mut manual = old;
        manual.weather_strength = Strength::Subtle;
        let scene = resolve(
            &manual,
            Some(&reading(61, 1)),
            Local::now(),
            cutoff(HALF_HOUR),
        );
        assert_eq!(scene.preferences.intensity, 15);
        assert_eq!(scene.level(), Some(Level::Light));
    }

    #[test]
    fn weather_stale_missing_and_future_readings_fall_back_to_the_choice() {
        let mut manual = opted_in();
        manual.enabled = true;
        manual.effect = Effect::Fog;
        let cutoff = cutoff(HALF_HOUR);
        assert_eq!(cutoff, Duration::hours(1));
        let now = Local::now();
        let resolve = |r: Option<&Observation>| resolve(&manual, r, now, cutoff);
        // Ages measured from the same moment the scene is resolved at.
        let reading = |code, minutes_ago| Observation {
            code,
            updated_at: (now - Duration::minutes(minutes_ago)).fixed_offset(),
        };
        assert_eq!(resolve(None).source, Source::NoReading);
        assert_eq!(resolve(None).preferences, manual);
        let fresh = reading(63, 59);
        assert!(resolve(Some(&fresh)).from_weather());
        assert_eq!(
            resolve(Some(&fresh)).until,
            Some(fresh.updated_at + Duration::hours(1))
        );
        for stale in [reading(63, 60), reading(63, 600), reading(63, -6)] {
            let scene = resolve(Some(&stale));
            assert_eq!(scene.source, Source::Stale);
            assert_eq!(scene.preferences, manual);
            assert_eq!(scene.until, None);
        }
        // A little clock drift is tolerated.
        assert!(resolve(Some(&reading(63, -4))).from_weather());
        let unknown = resolve(Some(&reading(42, 1)));
        assert_eq!(unknown.source, Source::Unknown(42));
        assert_eq!(unknown.preferences, manual);
        // The cutoff follows the Weather refresh interval, at most 3 hours.
        assert_eq!(
            super::cutoff(std::time::Duration::from_secs(300)),
            Duration::minutes(10)
        );
        assert_eq!(
            super::cutoff(std::time::Duration::from_secs(10800)),
            Duration::hours(3)
        );
    }

    #[test]
    fn weather_precedence_is_explicit_and_the_choice_is_kept() {
        let mut manual = Preferences {
            enabled: true,
            effect: Effect::Snow,
            ..Default::default()
        };
        let now = Local::now();
        let cutoff = cutoff(HALF_HOUR);
        let rain = reading(65, 5);
        // Off: the choice plays, whatever the weather.
        let scene = resolve(&manual, Some(&rain), now, cutoff);
        assert_eq!(scene.source, Source::Manual);
        assert_eq!(scene.preferences, manual);
        // On with a current reading: the weather wins over the choice.
        manual.weather = true;
        let scene = resolve(&manual, Some(&rain), now, cutoff);
        assert_eq!(
            scene.source,
            Source::Weather {
                code: 65,
                play: Some((Effect::Rain, Level::Heavy))
            }
        );
        assert_eq!(scene.preferences.effect, Effect::Rain);
        assert_eq!(scene.preferences.intensity, manual.intensity);
        assert_eq!(scene.level(), Some(Level::Heavy));
        // Calm skies play nothing, even over a chosen effect.
        let scene = resolve(&manual, Some(&reading(0, 5)), now, cutoff);
        assert!(!scene.preferences.enabled && scene.from_weather());
        assert_eq!(scene.level(), None);
        // With no effect chosen, the weather still plays when it calls for one.
        manual.enabled = false;
        let scene = resolve(&manual, Some(&reading(2, 5)), now, cutoff);
        assert!(scene.preferences.enabled);
        assert_eq!(scene.preferences.effect, Effect::Clouds);
        // The weather never rewrites the choice it falls back to.
        assert!(!manual.enabled && manual.effect == Effect::Snow);
        assert_eq!(
            resolve(&manual, Some(&reading(2, 120)), now, cutoff).preferences,
            manual
        );
    }

    #[test]
    fn weather_scenes_keep_pause_and_reduced_motion() {
        let manual = opted_in();
        let scene = resolve(
            &manual,
            Some(&reading(73, 1)),
            Local::now(),
            cutoff(HALF_HOUR),
        );
        let p = scene.preferences;
        let ready = lifecycle::State {
            ready: true,
            ..Default::default()
        };
        assert!(p.active(true, ready));
        // Reduce motion, an unknown session, lock, sleep and battery still stop it.
        assert!(!p.active(false, ready));
        assert!(!p.active(true, lifecycle::State::default()));
        for paused in [
            lifecycle::State {
                locked: true,
                ..ready
            },
            lifecycle::State {
                sleeping: true,
                ..ready
            },
            lifecycle::State {
                on_battery: true,
                ..ready
            },
        ] {
            assert!(!p.active(true, paused), "{paused:?}");
        }
        let mut no_pause = manual.clone();
        no_pause.pause_on_battery = false;
        let p = resolve(
            &no_pause,
            Some(&reading(73, 1)),
            Local::now(),
            cutoff(HALF_HOUR),
        )
        .preferences;
        assert!(p.active(
            true,
            lifecycle::State {
                on_battery: true,
                ..ready
            }
        ));
        // Speed 0 still freezes a weather-driven effect.
        let mut frozen = manual.clone();
        frozen.snow.speed = 0;
        let p = resolve(
            &frozen,
            Some(&reading(73, 1)),
            Local::now(),
            cutoff(HALF_HOUR),
        )
        .preferences;
        assert!(!p.animates());
    }
}
