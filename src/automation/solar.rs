//! Local solar events. Meeus solar coordinates and equation of time, as
//! documented by NOAA: https://gml.noaa.gov/grad/solcalc/calcdetails.html.
//! Events are UTC instants belonging to a date in the selected city's zone.
//! No forecasts, machine time zone, network access or polar fallback times.
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Weather,
    Manual,
}

/// Read once per evaluation, independently of weather-provider demand.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context {
    pub weather: Option<crate::config::Location>,
    pub manual: Option<crate::config::Location>,
    pub weather_enabled: bool,
    raw: Option<serde_json::Value>,
}

pub fn validate_city(city: &crate::config::Location) -> Result<Tz, String> {
    if city.name.trim().is_empty()
        || city.name.len() > 256
        || city.name.chars().any(char::is_control)
        || !city.latitude.is_finite()
        || !city.longitude.is_finite()
        || !(-90.0..=90.0).contains(&city.latitude)
        || !(-180.0..=180.0).contains(&city.longitude)
    {
        return Err("Choose a valid city.".into());
    }
    city.timezone
        .as_deref()
        .and_then(|zone| zone.parse().ok())
        .ok_or_else(|| "Choose the city again to confirm its time zone.".into())
}

impl Context {
    pub fn from_config(
        config: &crate::config::Config,
        settings: &super::rules::Automation,
    ) -> Self {
        Self {
            weather: config.location.clone(),
            manual: settings.manual_city.clone(),
            weather_enabled: if config.glance.enabled {
                config.glance.show_weather
            } else {
                config.enabled_widgets.iter().any(|w| w == "weather")
            },
            raw: None,
        }
    }
    pub fn read(store: &crate::desktop::DesktopStore, settings: &super::rules::Automation) -> Self {
        match crate::desktop::wallpaper::automatic::read_config(store) {
            Ok((raw, config)) => Self {
                raw: Some(raw),
                ..Self::from_config(&config, settings)
            },
            Err(_) => Self {
                manual: settings.manual_city.clone(),
                ..Self::default()
            },
        }
    }
    pub fn city(&self, source: Source) -> Result<&crate::config::Location, String> {
        let city = match source {
            Source::Weather if !self.weather_enabled => {
                return Err("Weather is off. Turn it on or choose a schedule city.".into());
            }
            Source::Weather => self.weather.as_ref(),
            Source::Manual => self.manual.as_ref(),
        }
        .ok_or_else(|| "Choose a city for this schedule.".to_string())?;
        validate_city(city)?;
        Ok(city)
    }
    /// Coordinates and zone, rather than display name. Includes source; an
    /// unresolved/disabled binding never qualifies old queued work.
    pub fn revision(&self, source: Source) -> Option<String> {
        use sha2::{Digest, Sha256};
        let city = self.city(source).ok()?;
        let mut hash = Sha256::new();
        hash.update([source as u8]);
        hash.update(city.latitude.to_bits().to_le_bytes());
        hash.update(city.longitude.to_bits().to_le_bytes());
        hash.update(city.timezone.as_ref()?.as_bytes());
        Some(
            hash.finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
    }
    pub fn event(&self, source: Source, event: Event, date: NaiveDate) -> Option<DateTime<Utc>> {
        let city = self.city(source).ok()?;
        occurrence(
            event,
            date,
            city.latitude,
            city.longitude,
            validate_city(city).ok()?,
        )
    }
    pub fn date(&self, source: Source, at: DateTime<Utc>) -> Option<NaiveDate> {
        Some(
            at.with_timezone(&validate_city(self.city(source).ok()?).ok()?)
                .date_naive(),
        )
    }
    /// Checks the exact inputs again under the existing config writer lock.
    /// Manual-city inputs are protected by the rules lock throughout execution.
    pub fn guards(&self, source: Source) -> Result<crate::desktop::WidgetPatch, String> {
        let mut guards = crate::desktop::WidgetPatch::new();
        if source == Source::Weather {
            let raw = self
                .raw
                .as_ref()
                .ok_or("Weather settings could not be read.")?;
            for pointer in [
                "/location",
                "/enabled_widgets",
                "/glance/enabled",
                "/glance/show_weather",
            ] {
                let value = crate::desktop::store::widget::get(raw, pointer);
                guards.set(pointer, value.clone(), value)?;
            }
        }
        Ok(guards)
    }
}

/// Bounded weekly search. A missing event stays missing; the UI says when no
/// event occurs in the coming week, and the existing minute wake rechecks it.
pub fn next(
    context: &Context,
    source: Source,
    event: Event,
    days: super::rules::Days,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let date = context.date(source, after)?;
    (0..=8)
        .filter_map(|n| {
            let date = date.checked_add_signed(Duration::days(n))?;
            if !days.has(super::rules::weekday_index(date)) {
                return None;
            }
            context.event(source, event, date).filter(|at| *at > after)
        })
        .min()
}

pub fn latest(
    context: &Context,
    source: Source,
    event: Event,
    days: super::rules::Days,
    after: DateTime<Utc>,
    upto: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let date = context.date(source, upto)?;
    (0..=8)
        .filter_map(|n| {
            let date = date.checked_sub_signed(Duration::days(n))?;
            if !days.has(super::rules::weekday_index(date)) {
                return None;
            }
            context
                .event(source, event, date)
                .filter(|at| *at > after && *at <= upto)
        })
        .max()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Dawn,
    Sunrise,
    Noon,
    GoldenHour,
    Sunset,
}

impl Event {
    pub const ALL: [Self; 5] = [
        Self::Dawn,
        Self::Sunrise,
        Self::Noon,
        Self::GoldenHour,
        Self::Sunset,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Dawn => "Dawn",
            Self::Sunrise => "Sunrise",
            Self::Noon => "Solar noon",
            Self::GoldenHour => "Golden hour",
            Self::Sunset => "Sunset",
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            Self::Dawn => "Civil dawn: the sun reaches 6° below the horizon before sunrise.",
            Self::Sunrise => {
                "Calculated sunrise at a level horizon. Terrain and weather can change when you see the sun."
            }
            Self::Noon => {
                "The sun's daily transit at the chosen city, which usually differs from 12:00."
            }
            Self::GoldenHour => {
                "Evening golden hour starts when the sun descends to 6° above the horizon."
            }
            Self::Sunset => {
                "Calculated sunset at a level horizon. Terrain and weather can change when you see the sun."
            }
        }
    }
    fn altitude(self) -> Option<(f64, bool)> {
        match self {
            Self::Dawn => Some((-6.0, true)),
            Self::Sunrise => Some((-0.833, true)),
            Self::GoldenHour => Some((6.0, false)),
            Self::Sunset => Some((-0.833, false)),
            Self::Noon => None,
        }
    }
}

/// Returns None for an absent event, invalid coordinates, or a date outside
/// the supported 1900–2100 interval. Neighbouring UTC dates cover the dateline.
pub fn occurrence(
    event: Event,
    date: NaiveDate,
    latitude: f64,
    longitude: f64,
    zone: Tz,
) -> Option<DateTime<Utc>> {
    if !(1900..=2100).contains(&date.year())
        || !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        return None;
    }
    (-2..=2)
        .filter_map(|offset| {
            let midnight = date
                .checked_add_signed(Duration::days(offset))?
                .and_hms_opt(0, 0, 0)?
                .and_utc();
            let jd = midnight.timestamp() as f64 / 86400.0 + 2440587.5;
            // Iterate the coordinates at the event itself; bounded even at poles.
            let mut minute = 720.0 - 4.0 * longitude;
            for _ in 0..5 {
                let (equation, declination) = position(jd + minute / 1440.0);
                let angle = match event.altitude() {
                    Some((altitude, rising)) => {
                        let lat = latitude.to_radians();
                        let divisor = lat.cos() * declination.cos();
                        if divisor.abs() < 1e-12 {
                            return None;
                        }
                        let cosine =
                            (altitude.to_radians().sin() - lat.sin() * declination.sin()) / divisor;
                        if !(-1.0..=1.0).contains(&cosine) {
                            return None;
                        }
                        cosine.acos().to_degrees() * if rising { 1.0 } else { -1.0 }
                    }
                    None => 0.0,
                };
                minute = 720.0 - 4.0 * (longitude + angle) - equation;
            }
            let at =
                midnight.checked_add_signed(Duration::seconds((minute * 60.0).round() as i64))?;
            (at.with_timezone(&zone).date_naive() == date).then_some(at)
        })
        .min()
}

/// Equation of time in minutes and apparent solar declination in radians.
fn position(jd: f64) -> (f64, f64) {
    let t = (jd - 2451545.0) / 36525.0;
    let longitude = (280.46646 + t * (36000.76983 + t * 0.0003032)).rem_euclid(360.0);
    let anomaly = (357.52911 + t * (35999.05029 - 0.0001537 * t)).to_radians();
    let eccentricity = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
    let center = anomaly.sin() * (1.914602 - t * (0.004817 + 0.000014 * t))
        + (2.0 * anomaly).sin() * (0.019993 - 0.000101 * t)
        + (3.0 * anomaly).sin() * 0.000289;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let apparent = (longitude + center - 0.00569 - 0.00478 * omega.sin()).to_radians();
    let seconds = 21.448 - t * (46.8150 + t * (0.00059 - t * 0.001813));
    let obliquity = (23.0 + (26.0 + seconds / 60.0) / 60.0 + 0.00256 * omega.cos()).to_radians();
    let declination = (obliquity.sin() * apparent.sin()).asin();
    let y = (obliquity / 2.0).tan().powi(2);
    let l = longitude.to_radians();
    let equation = y * (2.0 * l).sin() - 2.0 * eccentricity * anomaly.sin()
        + 4.0 * eccentricity * y * anomaly.sin() * (2.0 * l).cos()
        - 0.5 * y * y * (4.0 * l).sin()
        - 1.25 * eccentricity * eccentricity * (2.0 * anomaly).sin();
    (4.0 * equation.to_degrees(), declination)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    #[test]
    fn solar_events_are_ordered_across_seasons_and_hemispheres() {
        for (lat, lon, tz) in [
            (45.815, 15.982, "Europe/Zagreb"),
            (-33.87, 151.21, "Australia/Sydney"),
            (0.0, 0.0, "UTC"),
        ] {
            for day in ["2026-03-20", "2026-06-21", "2026-09-23", "2026-12-21"] {
                let times = Event::ALL.map(|event| {
                    occurrence(event, date(day), lat, lon, tz.parse().unwrap()).unwrap()
                });
                assert!(
                    times.windows(2).all(|pair| pair[0] < pair[1]),
                    "{day} {tz}: {times:?}"
                );
            }
        }
    }

    #[test]
    fn solar_polar_missing_events_are_not_invented() {
        let zone = "Arctic/Longyearbyen".parse().unwrap();
        for day in ["2026-06-21", "2026-12-21"] {
            for event in [Event::Sunrise, Event::Sunset, Event::GoldenHour] {
                assert!(occurrence(event, date(day), 78.22, 15.65, zone).is_none());
            }
            assert!(occurrence(Event::Noon, date(day), 78.22, 15.65, zone).is_some());
        }
        assert!(occurrence(Event::Sunrise, date("2026-06-21"), 90.0, 0.0, zone).is_none());
    }

    #[test]
    fn solar_city_dates_hold_at_dateline_and_dst_changes() {
        for (lat, lon, tz) in [
            (1.87, -157.4, "Pacific/Kiritimati"),
            (-13.83, -171.75, "Pacific/Apia"),
            (45.815, 15.982, "Europe/Zagreb"),
            (40.71, -74.01, "America/New_York"),
        ] {
            for day in ["2026-03-08", "2026-03-29", "2026-10-25", "2026-11-01"] {
                let zone: Tz = tz.parse().unwrap();
                for event in Event::ALL {
                    let at = occurrence(event, date(day), lat, lon, zone).unwrap();
                    assert_eq!(at.with_timezone(&zone).date_naive(), date(day));
                    let next =
                        occurrence(event, date(day).succ_opt().unwrap(), lat, lon, zone).unwrap();
                    assert!((next - at).num_seconds().abs_diff(86400) < 600);
                }
            }
        }
    }

    #[test]
    fn solar_rejects_unbounded_inputs() {
        for (lat, lon) in [
            (f64::NAN, 0.0),
            (0.0, f64::INFINITY),
            (91.0, 0.0),
            (0.0, -181.0),
        ] {
            assert!(
                occurrence(Event::Noon, date("2026-06-21"), lat, lon, chrono_tz::UTC).is_none()
            );
        }
        assert!(occurrence(Event::Sunrise, date("2101-01-01"), 0.0, 0.0, chrono_tz::UTC).is_none());
    }
}
