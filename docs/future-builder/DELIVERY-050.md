# 0.4.1 maintenance and 0.5 delivery

Authorized 2026-10-27. Complete the stages in order, with implementation review,
focused validation and corrections before advancing. Each visual change also
requires inspection of actual native renders at compact and wide sizes.

| Stage | Outcome | Status |
|---|---|---|
| Maintenance | Release the architecture repairs as 0.4.1; verify packages, upgrade, native Settings and signed updates | Complete |
| Solar Moments | Local sun calculations, safe scheduling, city binding and native rule controls | Complete locally |
| Adaptive setups | Coordinated Tidal, Paper and Moss appearances with truthful native previews | Complete locally |
| Updates | One clear Halcyon/Islands status and next action through existing OS installation | Complete locally |
| Personal widgets | Calendar agenda, then system readings in the existing recipe and Canvas system | Complete locally |
| Integration | Final behavior, rendering, migration and resource checks; reconcile plans and evidence | Complete locally |

The solar contract is [FB21](FB21-PLAN.md). Architecture repairs and their earlier
validation are in [the follow-up tracker](../ARCHITECTURE-FOLLOWUP.md).
The 0.5 feature implementation does not itself publish a 0.5 release.

Private build logs, checkpoints, visual reviews and artifact records live in
`.task-050-delivery-20261027/`. Keep one Cargo invocation at a time. Reuse valid
evidence; rerun checks when a source change, failure or unresolved concern warrants it.

SSD preparation removed 3.45 GiB of unused compiler output, leaving 74.75 GiB
available. Current release outputs, cached dependencies, source and unique evidence
were retained. Recheck space before source/package assembly and major builds.

## Local feature validation

- Solar: 74 automation checks and 17 focused sun/city tests. Native compact and
  wide, light/dark rule editing inspected. [Contract and checks](FB21-PLAN.md).
- Adaptive looks: four focused checks plus ten automation-editor checks. Actual
  appearance application, preserved content/geometry, city checks, repeated
  activation and read-only native previews. [Behavior](ADAPTIVE-LOOKS.md).
- Updates: 39 checks, including private PackageKit sessions and complete/missing
  app-and-companion update offers. Native installation and status screens reviewed.
  [Behavior](COORDINATED-UPDATES.md).
- Recipes: 25 focused checks, including Calendar privacy and retained cache,
  System demand, missing readings, migration, public transport and Apply/Undo.
  Native builder and active-widget screens reviewed at 820×600 and 1280×800.
  [Behavior](PERSONAL-WIDGETS-050.md).

The integrated suite exercised 1,395 Rust tests: 1,374 passed initially, 18 existing
worker/platform entrypoints were ignored, and three test issues were corrected.
Two captures used truncated PNG dimensions for rounded renderer buffers; a search
expectation predated Calendar/GPU recipe support. The affected 17 Glance and nine
navigation checks then passed, bringing the distinct passing total to 1,377.
The final native pass then added the Adaptive looks controller route and a
clear placeholder for the appearance picker; controller and focused solar
checks cover those corrections.

Maintenance CI corrections supply explicit COSMIC defaults to the mix fixture,
run bounded render workers in release mode, atomically publish lifecycle fixture
observations and save fractional-size native captures correctly. All 35 launcher
lifecycle tests pass locally. These corrections do not alter the published
0.4.1 package payloads.

## Final native pass

The final optimized executable passed isolated COSMIC sessions at 820×600 and
1280×800. Eighteen captures cover adaptive appearance, solar editing, update status,
and Calendar/System samples, missing states and applied recipes. Each recipe was
applied and undone with native keyboard input; original preferences were restored.
The published 0.4.1 executable refused both newer design records without changing
them. All private processes were reaped. No host package, service or desktop
configuration was changed.

The current development executable has SHA256
`0813cd189306f3136678504d97e09655d716e6ea757581636d3cd3ffe92e10b1`.
Its Cargo version remains the 0.4.1 baseline; it is not a new release artifact.
Feature source is committed locally; release identity and final package acceptance
belong to the next release preparation. Detailed evidence and source hashes are in
`.task-050-delivery-20261027/integration/validation.json`.

Final available SSD space: approximately 73 GiB after builds and captures. The
3.45 GiB cleanup preserved current artifacts, sources and unique evidence.

[Maintenance CI](https://github.com/profilaz3/halcyon/actions/runs/40592896633)
completed successfully: Rust, Python, collector, Islands core, license notices,
Debian package build and inspection. This run covers the published maintenance
branch and its test corrections; the 0.5 features remain local.
