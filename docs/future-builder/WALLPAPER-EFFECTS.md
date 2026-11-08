# Manual wallpaper effects

Implemented and validated locally, including a real COSMIC desktop review on
2026-11-08; unreleased. Builds on FB29's existing native GPU surface, wallpaper cache, ownership, setup transport and guarded preferences.
No new renderer, decoder, video, network source or generic plugin engine.

## Scope

One selected effect at a time: Rain, Snow, Fog or Cloud shadows. Each retains its
own settings. The existing rain mode and controls remain intact. Old preferences
without an effect selector mean Rain. Default rain drop size is 30%; explicit saved
sizes are preserved. Manual selection only; automatic Weather mapping is separate.

- Snow: density, flake size, speed, wind and opacity. Three analytic layers.
- Fog: opacity, height, speed, wind and neutral/cool/warm color.
- Cloud shadows: coverage, softness, speed, wind and shadow strength.

All use the existing quality, battery, lock, sleep, Reduce motion and occlusion
policies. Zero visible intensity suppresses the desktop surface. Hidden or disabled
settings do not make network requests. Appearance → Wallpaper owns the controls.

## Validation

Evidence is collected under `.task-wallpaper-effects-20261030/`. Each effect is
rendered and visually reviewed before moving to the next. Integrated native
Settings/Undo, setup transport, private desktop transitions and hardware GPU timing
are required before marking this local task validated. Physical-device and final
packaging acceptance remain separate.

The final local pass includes 22 focused regression tests and 15 hardware GPU
benchmark cases (both rain modes and all three new effects at 1080p, 1440p and 4K).
Native compact/wide Settings captures, slider interaction/Undo, setup round trips,
zero-intensity image fidelity, stationary effects and varied controls passed.
Two isolated desktop runs passed animation, zero-speed redraw suppression, lock,
disable/re-enable, global power/occlusion rules, background replacement and dock
input. Owned fixture processes were reaped; the installed app/session was untouched.

The nested Xvfb/COSMIC compositor logs persistent EGL surface failures and some
captures show partially clipped Clock text. Native offscreen effect/Settings renders
are clean; a disabled-effect baseline was stable. The real desktop review below did
not reproduce the clipping. This is not release qualification. Detailed captures
and observations are in the private evidence directory.

## Optimization pass

Evidence: `.task-effects-optimization-20261031/result.json`. The rebuilt app passed
26 focused regressions and a fresh isolated desktop behavior run. Hardware GPU
timestamps cover 25 stress cases: all five renderers at 4K in Eco, Balanced and
Smooth, portrait 4K, and two independent 1440p render targets. Density, size, speed
and visible intensity are raised; full settings are recorded per case. Each case
samples 60 frames and excludes the first 15. Frame requests use the selected
30/60 fps cadence; the benchmark sleeps for that interval after each submission.

On AMD Radeon integrated graphics (RADV RENOIR), median 4K Balanced GPU time is
1.87–2.82 ms per frame. The largest p95 among the 25 cases is 3.42 ms. GPU timings
exclude background upload, the desktop compositor and other applications; they
are not end-to-end frame latency, energy consumption or battery-life measurements.
Two offscreen targets do not qualify physical multi-monitor behavior.

- Snow, fog and cloud shadows retain one half-resolution R16F opacity field and
  a single-level background. Their owned texture payload falls from 73.8 to
  35.6 MiB per 4K Balanced output (52%). This is logical texture storage, not total
  process memory or driver VRAM accounting. Rain retains its refraction fields.
- Resizing or changing quality reuses the background texture. Frozen effects
  reuse their field; scrolled-out previews stop requesting frames. Unrelated
  redraws do not increase the selected animation cadence.
- Merging rain reuses its collision grid and trail scratch storage. Median CPU
  simulation time falls from 18.8 to 10.2 microseconds in the same 4K Balanced
  stress case. Drop count remains capped at 384.
- All 22 before/after pixel comparisons differ by at most 1/255 in any channel;
  rain and cloud-shadow captures are identical. Maximum mean channel difference
  is 0.0045 on the 0–255 scale.

Hardware claims apply only to the measured AMD device. This performance pass does
not establish release qualification.

## Real desktop review

Evidence: `evidence/fb30-native-20261108.json`. A release build of `origin/main`
(`b27fc19bc`) ran on the owner's real COSMIC session, one 1920×1080 output at 75 Hz,
in place of the stopped `halcyon.service`. It used temporary copies of the app
config and XDG folders; the same bounded command started the service again. The
installed app and its config were untouched. 25 `cosmic-screenshot` captures were
reviewed.

- Rain (Flowing and Merging), Snow, Fog and Cloud shadows render behind windows,
  widgets and the dock. Cloud shadows were also checked with a difference image
  against the effect-off baseline.
- The Clock was never clipped: the owner's 200% bold digital Clock, the same
  Clock moved clear of windows, and a smaller Clock across a minute rollover,
  each with an effect running and with none. The app logged no EGL errors. Xvfb
  is no longer installed, so the nested run could not be repeated; the clipping
  is attributed to that compositor's EGL surface failures.
- Speed 0% freezes the effect: two captures 11 s apart are pixel-identical and no
  GPU work is submitted. Reduce motion removes the effect surface at once and
  turning it off resumes it.
- Lock, sleep and battery pauses were driven through a private stand-in for
  logind and UPower on a separate bus, so the real session was never locked. Each
  stopped drawing and showed the plain wallpaper; unlocking, waking and turning
  Pause on battery off each resumed at 30 fps.

GPU busy time comes from amdgpu `drm-engine-gfx` in `/proc/<pid>/fdinfo`, for
Halcyon and for `cosmic-comp`. At 30 fps Halcyon used 48–56 ms per second
(1.6–1.9 ms per frame; a frame trace confirmed 28.9 fps) for every effect, and
the compositor about 39 ms per second, against 0.1 and 5.6 ms with effects off.
Smooth used 1.1–1.3 ms per frame at 60 fps. Frozen, Reduce motion, locked,
sleeping and battery-paused effects used none. This is busy time at whatever
clock the GPU chose, mostly 400 of 2000 MHz at 30 fps, and includes all of
Halcyon's surfaces, so it is not directly comparable with the pass timestamps
above. It stays below their 4K Balanced range and their largest p95 (3.42 ms).
The compositor cost of presenting a full-screen animated surface was not part of
the earlier measurements.

Limits: one 1080p output; no multi-monitor, HiDPI, battery-powered device,
energy or packaged-artifact qualification.
