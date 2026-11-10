# Weather-connected scenes

First version implemented and validated locally on 2026-11-08; unreleased.
Builds on FB30's four effects, FB07's shared weather provider and FB21's wake
scheduling. No new renderer, weather request, network source or video.

## What it does

**Match the weather** is a switch on the Wallpaper page, under Moving effect.
It is off by default. When it is on and the Weather widget has a current
reading for its city, that reading picks the effect:

| Weather widget reading (WMO code) | Effect | Level |
| --- | --- | --- |
| Partly cloudy (2) | Cloud shadows | Moderate |
| Foggy (45) | Fog | Moderate |
| Foggy, rime (48) | Fog | Heavy |
| Drizzle (51, 53), freezing drizzle (56), rain (61), showers (80) | Rain | Light |
| Drizzle (55), freezing drizzle (57), rain (63), freezing rain (66), showers (81) | Rain | Moderate |
| Rain (65), freezing rain (67), showers (82), thunderstorms (95, 96, 99) | Rain | Heavy |
| Snow (71), snow grains (77), snow showers (85) | Snow | Light |
| Snow (73) | Snow | Moderate |
| Snow (75), snow showers (86) | Snow | Heavy |
| Clear (0), mostly clear (1), overcast (3) | No effect | — |
| Any other code | Moving effect plays | — |

Light, Moderate and Heavy are a share of the person's own setting for that
effect: rain Density, snow Density, fog Opacity or cloud Coverage. **Weather
strength** chooses the shares; Natural is the default, so configs written
before it behave as before:

| Weather strength | Light | Moderate | Heavy |
| --- | --- | --- | --- |
| Subtle | 25% | 50% | 100% |
| Natural (default) | 40% | 70% | 100% |
| Strong | 70% | 85% | 100% |

Their setting is the cap; the weather never makes an effect stronger, and heavy
weather is always the full setting. Rounding goes up, so a visible setting stays
visible. Every other control (speed, size, wind, color, Rendering, Pause on
battery) is the person's own.

Weather strength is saved with the effect settings as `weather_strength`
(`subtle`, `natural` or `strong`), left out while it is Natural. It goes through
the same staging, Apply, Recent changes and Undo as Match the weather, and looks
keep the person's choice.

## Precedence

1. **Pauses always win.** Reduce motion, the lock screen, sleep, Pause on
   battery, an unknown session and Arrange stop every effect at once, whoever
   chose it. Speed 0 still freezes.
2. **Match the weather on, current reading:** the weather decides, including
   “no effect” under clear or overcast skies. This also applies when Moving
   effect is None.
3. **Otherwise Moving effect plays:** Match the weather off, the Weather widget
   hidden or off, no reading yet, a reading for a different city, an
   unrecognized code, or a reading that is out of date.

A reading is out of date after two Weather refresh intervals (one missed
refresh), and never later than three hours. A reading more than five minutes
in the future is not trusted. When the reading driving the scene expires, a
wake at that moment hands control back to Moving effect even if nothing else
wakes the app; the wake follows the existing suspend-aware wake anchor.

The weather never rewrites Moving effect or its settings, so turning Match the
weather off, or losing the reading, returns to exactly what was chosen. Turning
it on or off is one Wallpaper change through the existing staging, Apply,
Recent changes and Undo path. Looks keep the person's Match the weather choice,
like Rendering quality and Pause on battery.

## Data

The desktop reads the Weather widget's own shared observation for its current
city (FB07's hub), and only while the Weather widget is shown. Nothing is
fetched for the effect: with the Weather widget hidden there is no reading, and
Moving effect plays. Settings reads the widget's cached reading and watches the
cache file, so its state line follows new readings without a request of its own.

## Transitions

Desktop scene changes fade instead of switching: the old scene fades toward the
plain wallpaper over 1.5 s, then the new one fades in over 1.5 s. A change of
effect, of the weather's level, or between an effect and none all fade. A new
surface fades in from the plain wallpaper. When the scene goes quiet without a
pause, the surface fades out first and is removed 1.75 s later; if the weather
returns in the meantime the same surface fades back in. Pauses remove surfaces
at once, as before.

The fade is one uniform read by the composite pass. It never regenerates the
effect field, so a frozen effect stays frozen; it asks for frames only until the
fade ends. The Settings preview and fixed-time captures never fade, so edits show
immediately. Edits that keep the same scene, such as a slider, apply at once.

## Settings

The Match the weather row shows what is happening in plain words:

- Off: “Rain, snow, fog and cloud shadows follow the Weather widget”.
- On: how strong, in the effect's own slider name: “Rain: rain at 40% of your
  density”, “Foggy: fog at 70% of your opacity”, “Partly cloudy: cloud shadows
  at 70% of your coverage”, “Snow: snow at your full density”, “Clear skies:
  no effect”.
- Falling back: “No weather reading yet: using Moving effect”, “Weather
  reading is out of date: using Moving effect”, “Unrecognized weather: using
  Moving effect” or “Turn on the Weather widget to use this”.

While Match the weather is on, **Weather strength** (Subtle, Natural, Strong)
sits under it, and **Adjust** (Rain, Snow, Fog, Cloud shadows) chooses which
effect's controls show, so snow can be tuned before it snows. Adjust starts on
the effect the weather plays now, else Moving effect. It is only this window's
view: it is not saved, and it changes neither what plays on the desktop nor
Moving effect. Turning Match the weather off or on, or leaving the page, starts
it again from what plays. The controls set each effect's own settings (its cap).
Moving effect's explanation says it plays when there's no current weather
reading.

The preview shows what the desktop shows. When Adjust names an effect that
isn't playing, the preview shows that effect at its own settings instead. That
is how heavy weather plays it at every Weather strength, so there is one
honest picture of the cap being tuned rather than a level the weather may never
reach.

## Not in this version

A per-effect allow list, a temporary manual override with a timed return, and
blending two effects at once. Turning Match the weather off is the override.

## Validation

See [the evidence](evidence/fb33-weather-scenes-20261108.json).
