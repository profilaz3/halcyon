# One Halcyon package

Local 0.5 work. Publication remains separate.

- The Halcyon Debian package contains the app, Islands host, verified helpers and licenses.
- New installations leave Islands off. Dock & panel offers **Enable for next login** and **Turn off for next login**. No additional download or administrator prompt.
- The running panel stays in place until the next login. Per-bar style choices remain independent of host activation.
- Existing enabled installations retain selection and layout. Existing disabled installations stay disabled. User-edited files and crash recovery blockers remain protected.
- System updates replace the app and included host together. Native compatibility, ownership, manifest integrity, stock fallback and bounded helper cancellation remain enforced.

## Upgrade contract

Payload paths stay at `/usr/lib/cosmic-ext-halcyon-islands`; the owning package becomes `cosmic-ext-halcyon`. Manifests name the owner. Legacy manifests and restore journals remain readable.

Halcyon replaces and breaks the old Islands package before 0.5.0. A payload-free transitional `cosmic-ext-halcyon-islands` package depends on Halcyon ≥0.5.0. It is supplied to existing installations through APT, never required by a fresh Halcyon installation. This lets ordinary upgrades complete without removing packages. The bridge runs no maintainer scripts and owns no user files.

Release assembly requires explicit native binary, manifest and license inputs. Release verification binds both executable hashes to the same main package, checks the empty bridge, and retains fresh native acceptance. Published historical releases keep their immutable verification path.

## Validation

Completed locally:

- 35 focused Rust checks: installed identity, read-only update queries, activation cancellation/reaping and Dock interactions.
- 86 focused Python checks: package builders, ownership, enabled/disabled upgrades, recovery, archive admission and release gates. Four existing receipt-dependent fixture cases remain skipped; no qualification is inferred from them.
- Isolated APT upgrade and fresh-install simulations. Real dpkg operations under temporary roots cover both unpack orders, modified bootstrap configuration and removal of the empty bridge.
- Native Settings renders and enable/off button actions at 820×600 and 1024×760. Collapsing the Dock preview now returns its width to the controls.
- Optimized 0.5.0 app build and one assembled Halcyon package. Extraction checks verify both binaries, package metadata, checksums, helper provenance, licenses, root file ownership and disabled first-run behavior.
- Published 0.4.1 still passes historical renewal verification.

Evidence: `.task-bundled-islands-20261028/`. The installed desktop and published releases are unchanged. Final 0.5 artifact qualification and publication remain a separate release task; the release gate still requires them.
