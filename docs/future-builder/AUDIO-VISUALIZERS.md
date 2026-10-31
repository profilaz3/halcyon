# Audio visualizers

Local, unreleased implementation. Spectrum and circular Music presentations;
no waveform mode.

- Spectrum: opacity 0–100%, width and height, horizontal/vertical position,
  full-width and bottom-edge shortcuts. Placement uses the Music widget's display.
- Behind dock: separate Bottom-layer surface, no keyboard focus or pointer region.
  Explicit overlap opts out of reserved panel space; otherwise the compositor's
  working area applies. Opaque dock pixels cover the visualizer.
- Circular Music: radial bars, orbit dots and pulse rings around existing player
  controls. Uses Music's current position and size, including per-display layout.
- Settings: live player controls plus an explicitly labeled static spectrum sample.
  Sliders commit on completion through existing guarded preferences and Undo.
- Portable setup format 18 carries the visualizer preferences; older setups retain
  their existing minimum version. Player identities and audio are not exported.

## Audio boundary

Opt-in; defaults preserve existing Music cards. One local `parec` output-monitor
stream drives both presentations, using PulseAudio or PipeWire's Pulse protocol.
Debian and RPM packages depend on `pulseaudio-utils`. The source is explicitly
`@DEFAULT_MONITOR@`, never the default microphone. Missing capture support fails
closed and retries only while the feature is in use.

Mono 22,050 Hz PCM, 1,024-sample fixed FFT, 48 bounded logarithmic frequency bins,
roughly 21 updates per second. No audio files, uploads or saved samples. Capture
stops when playback stops, Music is disabled or its assigned display is hidden,
Arrange opens, or Reduce motion is enabled. Dropping the subscription kills the
owned reader process; failures clear the visible spectrum and retry after 15 s.
The visualization reflects all audio on the default output, not an individual app.

## Validation

Implementation evidence is kept in `.task-visualizer-implementation-20261030/`.
The validation result there distinguishes native widget renders, isolated audio,
and private compositor tests from physical-device acceptance. No host install,
logout, package publication or version change is part of this task.

127 distinct Rust regressions passed, including the opt-in private audio case.
Native captures were visually inspected. The final ELF passed the private COSMIC
companion dock test: behind-dock rendering, clickable audio popup, custom top-right
geometry, opacity changes and reader shutdown on Pause. Standard dock behavior
also passed. Physical hotplug, battery and mixed-scale acceptance are not claimed.
