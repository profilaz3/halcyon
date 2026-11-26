# Hourly weather

## Context

The Weather widget asks Open-Meteo for the current conditions and four
daily forecasts, and caches the reply as a `WeatherSnapshot`. Its full
reading (Part 1 of the app audit) can show only those four days; "will it
rain at five" needs hours.

## Decision

- The same single request also asks for `hourly=temperature_2m,weather_code,
  precipitation_probability,is_day` with `forecast_hours=25`: the coming 24
  hours from the current one. No extra request, location or key.
- `Weather` gains `hourly: Option<Hourly>` (`#[serde(default)]`), with
  parallel `time`, `temperature_2m`, `weather_code`,
  `precipitation_probability` and `is_day` lists as Open-Meteo sends them. A
  cache written before has none and still loads; a reply without hours shows
  none.
- The Weather reading shows the next 24 hours as a strip: time, conditions,
  temperature and chance of rain. The card is unchanged.

## Alternatives

- A second request for hours only when the reading opens. A wait in the
  reading and twice the requests for a reading opened often.
- Hours in the card. Cards are sized for at-a-glance; the reading has room.

## Consequences

Each reply grows by roughly 2 KB, well inside the 256 KB bound. Hours a
reply lists with missing values are skipped, not drawn as zero.
