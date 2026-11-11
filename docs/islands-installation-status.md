# Islands installation capability, version 1

Halcyon consumes the read-only `halcyon-islands status --json` contract. The same
implementation (`companion/install_status.py`) is also shipped with the main app
at `/usr/lib/cosmic-ext-halcyon/islands-status.py`, so removing the optional CLI
still gives a reliable absence result. Rust uses that stable helper directly.
No installation probe launches the companion or changes selection. V2 separately
queries child-owned read-only Capabilities for runtime health (see extension below).

The JSON object has `format: "halcyon-islands-installation"`, `version: 1`,
`installation: "installed" | "absent"`, `provenance: "package" | "manual" | null`,
`enabled: boolean`, and nullable booleans `compatible`, `running`, `healthy`.
Unknown format/version, timeout, invalid data or failed discovery means **pending**
in Rust. Initial unknown provenance exposes neither Islands controls nor the install link.
Known-installed provenance is retained while compatibility is pending; management
stays available and compatibility is null. A timeout never implies absence or
that the standard panel is running.

Package presence requires dpkg's installed status and the regular package CLI.
An unsupported payload protocol/source sets compatibility false; a missing or
altered payload/manifest sets health false, while installed/package provenance
remains available for management. Manual evidence requires a completed, unrestored `companion/install.py` record with no package
owner and exact binary, manifest and selector hashes/modes. A removed package's
user bundle is not a manual installation. Files or a socket alone are not proof.
Installation does not imply activation. Compatibility uses the existing selector's
reviewed-source policy; it does not claim runtime ABI or full-session acceptance.
Running and runtime health stay unknown in this read-only response; payload
damage can establish unhealthy status. A capable IPC host is required separately
for live arrangement.

Settings refreshes on open/focus. It uses the shared installation state for
the Islands panel controls on **Dock and panel**. Installation/runtime probes and native add-on
edits run on blocking workers; the event handler consumes structured results.
Read-only UI planning uses that cached capability. The store independently
revalidates on the worker under transaction locks before sidecar writes, and
refuses a write if a UI handler bypasses the worker. Imported
and scheduled looks omit optional Islands settings, including `island_gap`, when
absent, keeping saved sidecars. Applying a look does not enable disabled Islands. The setup wizard and Dock
controls still honor an explicit user choice to enable or disable them.
The Islands panel ships in the main Halcyon package, so there is no separate
download or install button. When it is absent, **Dock and panel** says the
Islands panel isn't installed and asks you to reinstall Halcyon from the same
place you got it, then sign out and back in.

Tests use an explicit root. The CLI `--root` path never queries host packages or
IPC; Rust fixtures read only `<fixture>/islands-installation.json`, with the same
versioned JSON shape. Missing fixture evidence means absent. Pending/corrupt
fixture evidence never falls back to the host. The fixture record is not a live
installation attestation API.

## Group 3 extension (same installation v1 envelope)

V2 package bundles use `bundle_contract.classify_compatibility` as the sole runtime
admission policy. The old source-only `compatible` description above applies only
to legacy records. New status adds `status_version: 2`, `status`, `reason`,
`installed_generation`, `active_generation`, `compatibility` and sanitized
`package_versions`. The status labels are exactly Ready, Log out to activate,
Islands disabled, Checking compatibility and Compatibility update needed.
Legacy status v1 remains readable. New helpers do not claim standard-panel
health merely because Islands is disabled or its verification timed out. Unknown running measurement can
never be Ready. The explicit installed runtime target and measured runtime profiles are
separate from the complete qualification closure; only an explicit qualified catalog binding connects them. Integrity failure retains installed provenance and management.
Ready additionally requires the exact selected running ELF and child-owned
read-only Capabilities reply; this health result never establishes installation.
A removed package still produces absent even when a retained generation exists.

Rust parses these structured results on the existing background discovery job.
It maps status labels for display, without inferring a cohort from package
versions or running IPC. Copy diagnostics uses only the cached, allowlisted
structured data, so it performs no native-thread process or lock waits. Unknown
status versions remain pending. Diagnostics never invoke refresh, clear crash
markers, read personal configuration, or expose filesystem/session identities.

See [COMPATIBILITY.md](COMPATIBILITY.md) for the catalog, final-artifact evidence,
actual 0.2.0 bootstrap and restore-chain contracts. An empty reviewed catalog is
an honest blocked qualification state, not an invitation to synthesize support.

Diagnostics use `qualification_cohort`, `runtime_targets` and `running_profiles` (digest lists),
with `installed_cohort: null`,
never a guessed running full-package cohort. These fields are also present at the top level of status v1. Rust cached diagnostics
retain only validated 64-digit hashes (at most 64 profiles); no process or file IO was added to Settings.

Runtime measurement v4 distinguishes `prelaunch` eligibility from `running`
readiness. Status uses the actual IPC owner's procfs identity and checks its
panel-owned children, counts and ancestry before Ready. A prelaunch measurement
cannot establish Ready even with a healthy IPC reply, and contributes no
`running_profiles`. Staging reports `runtime_readiness: not_measured` and promises
only the next login after successful staging. These phases add no native-thread
IO to Settings and do not change installation/provenance or absent-management policy.


Settings first runs `--presence` on its blocking worker and displays provenance,
then starts bounded compatibility work. `--progress` emits an initial record and
one final JSON line; ordinary CLI JSON remains a single record. Rust's nonblocking
pipe reader bounds output to 128 KiB total / 64 KiB per record and keeps its
six-second outer cutoff. The Python worker uses the remaining five-second helper
budget, cancels and reaps verification and emits honest pending on timeout.
Disabled status performs no installed-runtime or procfs audit.

Open/focus requests coalesce to one in-flight check. Completion generations reject
stale results. A timeout/unknown compatibility result stops automatic focus
retries; reopening Settings or focusing it again deliberately retries. Cached presentation is not
transaction authority: existing background write/Undo/baseline contracts continue
to revalidate. No status operation changes selection, kills a host, clears a latch
or writes user settings. Missing optional CLI plus retained generation still
means absent, not a supported manual installation.
