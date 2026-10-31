# Native wallpaper rain

Local, unreleased. Two original GPU rain implementations; no video files,
decoders, downloads, browser engine or added rendering dependency.

## Controls

Appearance → Wallpaper → Living wallpaper. Rain is off by default. “Try rain”
animates the Settings preview without enabling the desktop effect. Flowing uses
procedural droplets; Merging uses moving, merging drops with evaporating trails.
Density and Trails are directly available, independently adjustable from 0–100%.
Trails at 0% removes all moving-drop traces. Adjust rain contains size, speed, wind,
mist, refraction, rendering quality and battery behavior. Help stays beside controls.
Changing density immediately refreshes the Merging preview and desktop effect.

Controls save on completion through guarded preferences and scoped Undo. Changes
apply to every connected display. Existing wallpapers and their COSMIC ownership
are retained. Solid colors, pictures and the actual current slideshow image use
the existing bounded image workers; unsupported sources retain the static desktop.

## Rendering and lifecycle

- Native Iced/wgpu Shader widgets use the existing device. Animation state and
  redraw requests stay inside each widget; no frame-by-frame app messages.
- Original analytic droplet slopes; no ray marching or repeated field evaluation
  to estimate derivatives. Merging uses a spatial grid and at most 384 drops,
  including trails. Residual drops have independent spacing, size, shape, lifetime
  and fading strength; they do not share a fixed vertical deposition grid. A
  resumed frame advances at most 50 ms.
- Rain caches wallpaper mipmaps and static beads in two bounded RGBA16F fields.
  Snow, fog and cloud shadows instead use one R16F opacity field and a background
  without mipmaps. Field resizing retains the uploaded background. Eco uses
  one-third resolution at 30 fps; Balanced
  uses half resolution at 30 fps; Smooth uses half resolution at 60 fps. Fields
  are capped at 1920 pixels per side. The wallpaper itself stays sharp.
- At most eight output surfaces. Raster preparation is capped at 3840×2160 total
  pixels, with an 8192-pixel side bound. No per-frame image decoding or readback.
- Background-layer surfaces have empty input regions and no keyboard ownership.
  Widgets, windows and dock popups stay above them. Native background-state
  receipts trigger remapping after cosmic-bg recreates its own background.
- File watches use decode-time metadata stamps, so image replacement is detected
  without loading the entire file on a timer or recreating surfaces on subscribe.
- Lock, sleep, battery (by default), Reduce motion, Arrange and disabling rain
  remove runtime surfaces and release their GPU resources. Missing session/power
  observations pause the effect. Graphical session lookup works for user services
  outside the login session. Compositor frame callbacks stop hidden animation.
- Per-output ownership and generation checks reject late image completions and
  retired resize events. Existing stale Arrange-surface rejection stays intact.
- Unrelated window redraws do not advance animation above its selected cadence.
  Scrolled-out Settings previews stop requesting frames; unchanged fields are
  reused. Merging rain reuses bounded collision and trail storage each frame.

Both graphics target formats retain the picture's native brightness. The original
WGSL and bounded Rust simulation are part of Halcyon; reference projects supplied
technique/lifecycle context, not copied shader code or runtime dependencies.

## Setups and compatibility

Rain is a shared surface preference in the existing style domain. Explicit rain
settings, including turning it off, round-trip through setup format 19 and design
format 5. Untouched setups retain their earlier minimum format. Old readers reject
new records instead of silently losing the effect. Native wallpaper selection is
not replaced by saving rain controls.

## Validation

Reproducible evidence, native captures, build logs and device measurements are in
`.task-rain-implementation-20261030/`; its `result.json` records the final scope.

Coverage includes both actual GPU pipelines, animated pixels, dry-image color
fidelity, compact/wide Settings, scrolling, Apply/Undo, setup capture/import,
concurrent edits, decode cancellation, stale surface completion and watcher races.
Private nested COSMIC exercises both styles behind a real stock dock, a clickable
calendar popup, fullscreen occlusion/resume, simulated logind/UPower transitions,
battery opt-out, disable/re-enable and background-service restart. All fixture
processes are owned and reaped. Host configuration, packages and session stay intact.

GPU timestamps compare effect passes at 1080p, 1440p and 4K on this workstation's
AMD integrated GPU. They exclude the desktop compositor and are not power/battery
measurements. Physical hotplug, suspend, mixed-DPI devices, non-AMD qualification
and final packaged-artifact acceptance remain outside this local validation.

The initial implementation passed 76 distinct regression tests and the explicit
hardware benchmark. The trail-polish revision is recorded separately in
`.task-rain-trails-20261030/`: 18 focused regressions cover varied/fading residuals,
independent density/trail controls, old-preference defaults, live density changes,
GPU captures, compact/wide Settings, save/Undo and the existing lifecycle guards.
Its rebuilt ELF passed the private desktop transitions above; the explicit
hardware benchmark also passed again on both modes at all three resolutions. The nested compositor is software;
its app opened AMD render nodes, but per-process DRM engine counters were unavailable.
Hardware GPU timing is established by the separate explicit Vulkan benchmark.

Snow, drifting fog and cloud shadows extend the same surface and lifecycle; see
[Manual wallpaper effects](WALLPAPER-EFFECTS.md). The internal `rain` preference
record remains for backward compatibility.
