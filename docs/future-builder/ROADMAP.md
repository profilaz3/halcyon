# Build sequence

The target is a coherent, production-qualified customization system. 0.4.1 is published; 0.5 is the next working target. These stages supersede the UI-first sequencing in the archived concept proposals.

## Next feature for 0.5

[FB21 — Sun-based appearance schedules](FB21-PLAN.md), requested in
[issue #14](https://github.com/profilaz3/halcyon/issues/14), extends the existing
scheduler and FB15 Moments. Follow the Weather widget's city when enabled;
otherwise offer a manual city. Calculate dawn, sunrise, solar noon, evening
golden hour and sunset locally. Change appearance without moving widgets.
Review the contracts, then calculations/scheduling, then native controls and
final acceptance. Review and fix each stage before advancing. Status: **implemented locally; final 0.5 package acceptance pending**. See [the delivery record](DELIVERY-050.md).

## 0 Design and measure

Tasks FB01–FB04. Review the ownership and migration boundary, style precedence, provider keys, display coordinates, native capabilities and resource limits. Capture current behavior and performance with representative desktop, preview and Settings workloads. Keep the existing wizard repair separate from this project.

**Exit:** a reviewed contract and migration example, reproducible baseline, finite resource budgets, and resolved capability probes for the first slice. No generic engine implementation before these choices have concrete examples.

2026-10-26: this foundation-design gate is recorded in [CONTRACTS.md](CONTRACTS.md), [BUDGETS.md](BUDGETS.md) and [CAPABILITIES.md](CAPABILITIES.md). Native-tree timings and isolated idle/panel probes are baselines. Actual input-to-present latency, maximum-load memory and all new behavior must still pass stage 1 acceptance. Vertical Islands and arbitrary font axes remain outside the initial scope.

## 1 Prove one complete path

Tasks FB05–FB09. Extend shared styling, clock faces and native component measurement. Separate snapshot inputs from provider work; exercise shared provider demand. Connect the same components to the current placement and guarded commit paths. Use a minimal native inspector and isolated multi-instance fixtures.

**Exit:** Tidal-like and Paper-like clock/weather scenes render consistently in desktop and preview; existing config/profile behavior survives migration; cancellation, stale work, Undo and display changes pass. Runtime costs meet the stage 0 budgets. One composite design can be applied without an extra writer or a new service.

This is the first useful foundation. It does not require every widget variant, arbitrary recipes or a complete canvas editor.

## 2 Build expressive defaults and practical controls

Tasks FB10–FB13. Finish Tidal and Paper as real presets, then Moss and Carbon. Add the Type wall, native font fallbacks and the surface choices those looks actually need. Validate Spine with a standard vertical panel. Extend existing Settings with clear inherited/overridden values and usable keyboard controls.

Build the desktop canvas and local Mix board on the proven contracts. Reuse existing content/account editors. Save library designs separately from applying them. Produce real-rendered share images and existing setup files before introducing a hosted gallery.

**Exit:** a person can customize, compare, save and undo a complete desktop without editing configuration files. Small windows and multiple displays retain accessible controls. Screenshots match real supported behavior.

## 3 Add bounded creative extensions

Tasks FB14–FB17. Prototype Ridge alignment and fallback before promoting depth to a supported feature. Add day/night Moments first, with explicit override and schedule policy; extend to four moments/sun times only after time, resume and conflict tests.

Complete the multi-instance persistence migration before exposing personal recipes. Start the builder with a small native part vocabulary and existing data/actions. Recipes must render predictably with missing data, absent fonts and small work areas.

**Exit:** each extension independently passes its contract, lifecycle, privacy and resource gates. Ridge and Moments can ship independently; neither blocks useful basic customization. No arbitrary code plugins or automatic activity rearrangement are implied.

## 4 Share and qualify

Tasks FB18–FB20. Add bounded portable asset packages, credits, capability requirements and compatible fallbacks. Test transfer between different machines and display arrangements. Add curated discovery and stable revision links only after local export/import works.

Run release acceptance against the exact packages: upgrade, uninstall, native input, monitor transitions, device/performance coverage and clear fallback behavior. Keep hosted community work separately deferrable.

**Exit:** evidence supports each shipping claim; no unresolved data loss, input capture, recovery or import-boundary defects. Update approval never replaces a personal remix silently.

## Working rule

Implement the smallest end-to-end feature that demonstrates a contract. Add an abstraction only when a second real use needs it. Preserve the single Cargo slot and run relevant tests when code changes; planning and asset organization do not trigger a rebuild.

Status and task dependencies live in [tracker.json](tracker.json). [QUALITY.md](QUALITY.md) defines the acceptance evidence.
