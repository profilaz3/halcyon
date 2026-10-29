# Islands compatibility and immutable qualification

## Supported-family startup (0.5.0; policy v4)

Islands supports attempted startup on COSMIC **1.10 and 1.11**, including AMD,
Intel and NVIDIA desktops. Physical hardware and COSMIC 1.10 have **not** been
natively qualified by the recorded VM test. Support means a bounded attempt with
native interface checks, actual child-owned IPC readiness and reliable stock
fallback; it does not guarantee every driver or future COSMIC version works.
Native backdrop blur remains disabled.

The explicit catalog `startup_policy` is separate from exact qualification rows.
Panel and session must belong to the same supported family; patch/rebuild/source
suffixes may differ. The installed compositor/settings daemon must be present.
The verified companion must have the pinned native source/ELF, protocol 1 and
probe 1. Its real loader/Wayland probe must succeed before starting a panel.
Unknown families, ambiguous panel configuration, source or interface failures
choose stock. Missing or invalid individual applets are reported independently. No environment variable bypasses this product policy.

This policy trusts the ordinary system package installation for OS code, providers
and resources. It does **not** attest their archive hashes, dependency closure,
kernel or GPU. Core session executables and discovered configured applet commands
are pinned to opened installed objects for one operation. A restarted core,
replaced executable or configuration race invalidates that observation. Readiness
requires the expected core session and verified host-owned IPC; optional applet
process counts do not decide whether the panel can run.

Configured desktop IDs are resolved using native XDG precedence without a fixed
applet list. Standalone/multiplex executables, third-party entries, wrappers and
repeated applets are supported. Missing/invalid entries and unobserved direct
children produce bounded applet-specific diagnostics. Wrapper disappearance is
not treated as failure. These observations do not prove popup/rendering health.
The native host skips malformed launch commands instead of panicking.

Zero, one and multiple bars and outputs use the same resolver (up to its native
32-output bound), retaining sidecar, enabled absent-name fallback, All/Active/Name
and ABA checks. Exact historical qualification retains its recorded child
contracts; it does not constrain ordinary startup to the previously tested list.

One immutable verified bundle/catalog context is used per operation. There is no
persistent path/stat acceptance cache. The five-second prelaunch deadline includes
the unchanged ABI probe; child-owned protocol/capabilities and actual inventory
must pass within the existing 15-second readiness deadline. Owned process-group
cleanup, cancellation, descriptor isolation, notification handoff, crash latches
and stock fallback are unchanged. Status is read-only, bounded by the existing
worker/Settings deadlines, and never starts, replaces or kills a host.

Packages using this family startup policy contain the validated policy without
the historical exact-cohort records. The complete historical catalog remains in
the repository and corresponding source archive. This avoids parsing 25 MiB of
historical attestation on each startup/status operation; the family policy,
native binary pins, integrity checks and deadlines are unchanged. Packages with
the legacy exact-cohort policy retain their complete catalog.

### Status and upgrades

Installation provenance and management remain available while checking. **Ready**
means the active verified host owns the expected IPC and the core session is
valid, not that every applet or this hardware was qualified. Diagnostics distinguish
`family_ready_hardware_not_qualified` from historical qualification. **Log out to
activate** applies only to a genuinely deferred/different selected generation.
An already-selected generation without a healthy host reports **Compatibility
update needed**, not repeated logout advice or a false claim that stock is running.

A supported-family 0.2.0 user can stage this generation regardless of VM device,
kernel or exact package-byte mismatch. Staging preserves the original owned
restore chain and does not touch a running host. Activation is at next login;
unknown interfaces or failed readiness fall back safely. Disable unwinds the new
and original installation with guarded external edits, baseline and Undo intact.
New/disabled/manual users are not automatically enabled. Historical v2 generations
remain verifiable under their original policy for migration; missing new helpers
cannot carry the new startup policy. Public 0.2.0 assets remain immutable.

### Final 0.3.0 package acceptance

The final 0.3.0 main and Islands packages passed focused acceptance together in
an offline, complete COSMIC 1.11 KVM login with virtio/Mesa llvmpipe. Checks cover
installed file integrity, Settings input, live Islands switching, owned-process
cleanup, stock notification recovery and disable/relogin with layout preservation.
The [review](releases/0.3.0-acceptance.md) records exact bytes, observations and a
rendering artifact also seen with the unmodified stock panel. This does not
qualify physical hardware, COSMIC 1.10 or the full native matrix.

`companion/compatibility/evidence/accepted-final-030.json` binds the final package,
ELF, helper and evidence hashes. The release gate checks those bindings separately
from historical qualification. The corresponding source includes the complete
historical catalog; the installed catalog contains only the validated family policy.

### Exact historical qualification remains exact

The unchanged catalog row records native run **40071768899** on the exact Pop
noble COSMIC 1.11, one-output virtio/Mesa llvmpipe scope: prelaunch 2.399 s, running
admission 4.344 s from prelaunch start, all 19 old owned PIDs absent, stock
notifications and relogin passed. Its 1,783-package/165,721-file authenticated
source/distribution cohort and 105-file runtime target are preserved. This is not
physical-GPU, COSMIC 1.10 or full historical-matrix qualification.

`companion/compatibility/evidence/accepted-runtime-40071768899.json`, the original
pending test authorization and raw observation are unchanged. Their tested payload
is `f3bb6c779355b9fb4180a18c84dd7453e8d00b51720edc7376d8535bceb15635`.
New startup helpers have a **different payload** and are not relabeled as that
native result. Exact-target authoring/measurement tools below still describe the
stronger evidence contract; they are not ordinary supported-family login gates.

## Trust and identity

`scripts/compatibility/discover.py --source-config <json> --previous <json>
--out <candidate.json>` verifies InRelease with gpgv in a private empty trust
home. The public Pop key is pinned to SHA256 in `pop-noble.json` and fingerprint
`63C46DF0140D738961429F4E204DD8AEC33A7AFF`, independently published at
https://apt.pop-os.org/. Its bytes are from the pinned upstream keyring commit
`356a75e405d00a23889998d2e8f2508a9e67b535`. This key is public verification
material; no signing key is generated or needed.

Discovery verifies signed scope, index SHA256/size, every selected binary and
full source archive SHA256/size. Source archive identities include Debian
patches; a version's abbreviated Git suffix is never binary attestation. HTTPS
is required and downloads, subprocesses and decompression are bounded. Pop's
Release currently has no Valid-Until: the earlier of signed Valid-Until (when
present) and signed Date + **seven days** is the enforced deadline. Future dates,
wrong keys, expired metadata and changed bytes block the candidate. A static base
pocket with Suite=Codename and no Valid-Until can instead use an explicitly
reviewed `immutable_release_sha256` pin of the complete signed InRelease; a
publisher Valid-Until is never overridden. Rolling pockets retain the seven-day
policy. Clock trust
remains the executing machine's responsibility.

The default configuration deliberately discovers the five core package/source
identities without calling them a complete environment. Supply `cohort_lock`, an
exact name→version map from the retained complete image, and all its signed
archives (including the distribution's pinned Ubuntu keys/components). All
Depends/Pre-Depends alternatives, version constraints and virtual providers must
resolve inside that lock using `python3-apt` semantics. Native qualification also
requires an exact guest package inventory, clean dpkg audit and dependency
closure. Session/applets/settings daemon, libraries, graphics, kernel and system
packages belong in the complete image identity, not just panel/comp/session.
Missing archive retention/source inputs explicitly block discovery. Optional
`retain_packages: true` saves verified content-addressed debs beside the result.

Each archive declares `pocket` (the `dists/` path), expected signed `suite`,
`codename`, `origin`, components and pinned public key. These are distinct from
installed `distribution`/`suite`/`architecture`: Ubuntu noble-updates and
noble-security both have signed Codename noble. Binary and source rows retain
all signed index provenance, including collisions. Conflicting binary bytes or
control dependencies block selection unless `package_sha256[name]` explicitly
pins one binary identity; conflicting source archives independently block it.
Archive iteration order cannot silently overwrite either identity.

`--retained-inputs inputs.json` reads only a URL→`{path, sha256}` blob manifest;
missing entries are never fetched. The signed Release still authenticates every
index. `resolve_metadata_only: true` resolves exact locks without downloading
package/source blobs, reports all missing blobs with URL/hash/size, and always
blocks qualification. A normal run also reports missing retained blobs explicitly.
The result's `missing_inputs`, `collisions`, `indexes` and per-package/source
`provenance` retain the authentication and reconstruction trail. Missing binary
bytes mean no `files` inventory and cannot form an admissible catalog row.

### Streaming payload and inventory resources

Authenticated binaries and sources use `archive_io` streaming adapters. Each
object must match the **exact signed Size and SHA256**, independently of HTTP
headers or a retained map's digest. Reads are at most 1 MiB; short, overlong and
same-size changed bodies fail. The online adapter runs DNS/connect/TLS, response
headers, chunk framing/trailers and body reading in an owned worker process. The
caller polls only its body pipe against an absolute deadline; it can kill/reap
the worker even when urllib/HTTPResponse is blocked inside framing or header
parsing. The worker retains a 30-second socket inactivity timeout as an additional
bound. EOF is successful only after worker exit zero; errors are sanitized. No
production reader thread or socket survives scope exit. Sources are
hashed without spooling or extracting. A binary is streamed to a private file
beside the output, then inventoried from those verified bytes. With
`retain_packages: true`, only a fully verified/inventoried snapshot is atomically
published to `packages/<sha256>.deb`. Failed temporary snapshots are removed;
existing verified retained inputs are untouched. Source references with the same
signed size/hash reuse a successful verification **within that run only**;
all references and signed provenance remain in the candidate.

Policy ceilings are separate from each object's signed size. Configuration may
**tighten**, never raise, these via a `resource_limits` object:

| `resource_limits` key | Default ceiling |
| --- | --- |
| `binary_bytes` | 4 GiB per signed .deb |
| `source_bytes` | 2 GiB per signed source blob |
| `total_payload_bytes` | 32 GiB reserved per run (successful source reuse is counted once) |
| `expanded_package_bytes` | 8 GiB of complete tar stream per package |
| `member_bytes` | 4 GiB per tar member |
| `member_metadata_bytes` | 1 MiB decoded link/owner/PAX metadata per member, and 1 MiB aggregate encoded extensions in its chain |
| `package_members` | 250,000 headers/members per package |
| `path_bytes` | 4,096 UTF-8 bytes per normalized path |
| `inventory_json_bytes` | 64 MiB per-package path/inventory budget and aggregate file-map JSON |
| `disk_reserve_bytes` | 256 MiB free beyond the current signed binary size |
| `payload_seconds` | 1,800 seconds absolute across all HTTPS stages; separate kill/reap cleanup bound of 1 second |
| `package_seconds` | 900 seconds per inventory/decompressor |

InRelease stays bounded to 4 MiB; compressed and expanded indexes stay bounded to
512 MiB. InRelease/map reads have a 300-second deadline; signed-index streaming
uses the default object deadline. Signed expiry is checked
again before a long discovery run writes its result. Limits never create an
unsigned exception. The 1,875,589,312-byte firmware binary and 871,086,992 /
903,613,276-byte source inputs fit the separate payload policy; actual bytes still
must be verified. Signed `Installed-Size` is an estimate, not a substitute for
measuring the complete expanded stream.

Inventory reads every regular member, including bytes outside `/usr` and `/lib`,
and records every regular runtime file in those two namespaces. All tar bytes,
including headers, padding and trailers, count toward the expanded bound. It
rejects traversal, normalized duplicate paths, oversized members/extension
headers, malformed/truncated headers and unsupported global/sparse extensions;
ordinary GNU long names and bounded per-file PAX `x`/Solaris `X` metadata are
supported. Unknown archive constructs fail rather than disappearing from an inventory. Extension
headers are bounded before tarfile allocates them, recursive extension chains
are limited to 16 with an aggregate allocation budget. Processed TarInfo history
is discarded immediately (Python `r|` otherwise retains it); link/owner/PAX
metadata has its own per-member bound, including for out-of-scope files. This
preserves the no-link-follow policy while avoiding metadata accumulation across
members. Timeouts terminate the owned decompressor process group.
No package content or maintainer script is extracted/executed. Existing mutable
configuration and symlink/hardlink policy is unchanged; link targets are not
followed. Native runtime ownership and readiness checks are unchanged.

`fetch(url)` remains a bounded metadata/small-fixture interface. Payload callers
use `get.open(url)` as a context manager returning a readable binary stream; both
production HTTPS and `retained_fetch` implement it. HTTPS additionally exposes
`open_deadline(url, deadline, timeout_code)`, which discovery uses to pass the
absolute monotonic object deadline before any worker connection work begins.
The worker is the sibling `http_transport.py`, launched with the current Python
interpreter in isolated mode; ship that file alongside `archive_io.py`. Custom
in-process test adapters do not confer the production HTTPS deadline guarantee.
Byte-returning injected callbacks cannot handle objects above the metadata ceiling. The retained adapter
hashes the same opened regular file while it is consumed, checks its local map
hash and file stability, and independently enforces the signed object size/hash.
Missing retained inputs never fall back to network. Relative map paths remain
relative to that map, so rebase paths when merging metadata/payload maps on a
remote runner. The map itself is bounded to 16 MiB.

The candidate's nonidentity `resources` object reports the effective limits,
reserved payload bytes/objects, and each package's measured expanded/regular
bytes, largest regular member, largest decoded member metadata, header/member
and inventory counts. It also reports `capacity` measurements. Candidate output is streamed atomically with a
128 MiB record ceiling; partial output cannot masquerade as success. A resource
failure is a blocked discovery error, never permission to omit files or packages.

### Measure the final catalog before packaging

Catalog v1 uses the explicit **64 MiB** `bundle_contract.CATALOG_BYTES` limit
through `read_catalog`, shared by runtime bundle verification, packaging,
qualification/promotion and capacity measurement. Ordinary `read_json` records
(manifests, selections and journals) retain their **16 MiB** limit; the individual
file-digest ceiling remains **512 MiB**. The candidate writer remains **128 MiB**.

This catalog-specific allowance follows actual complete discovery: 1,783 packages,
165,721 file entries, a 25,689,147-byte full-cohort envelope and a 23,002,305-byte
identity-only lower bound. Both exceeded the old 16 MiB reader. A 64 MiB catalog
allows the measured cohort plus one comparable retained cohort and room for
runtime/equivalence/evidence fields. It is not a guarantee of arbitrarily many
retained rows: measure each final combined catalog. Never prune identities to fit.
Older ABI/payload combinations retain their separate immutable packages.

The reader opens without following the final symlink and refuses symlink ancestry,
nonregular files, oversized opened files, growth beyond the cap, truncation and
observed replacement/mutation. It reads at most the limit plus one byte, verifies
stable opened-file metadata and checks the manifest digest over the exact bytes
it parses. Packaging and generation staging copy bounded authored bytes without
reserialization. The bootstrap selector verifies executable helpers first, then
delegates catalog data verification to this reader before any refresh or launch;
it does not perform an unbounded preliminary catalog hash. Staging rechecks the
expected catalog digest while capturing its bounded bytes after bundle validation.
No path/stat-only cache or permanent trust cache is used; each load and pure
classification validates the full catalog. File-path traversal validation uses
exact POSIX components without allocating a Path for every inventory entry.

The wire format and cohort identity calculation are unchanged. Old bounded catalogs
and previous empty catalogs remain valid. Older helpers safely refuse catalogs
above 16 MiB; do not mix them with a new catalog. Changed helper bytes change the
payload/generation identity, so final packaging must use a new monotonic version
and native qualification of those exact final bytes. No old artifact is rewritten.
The current catalog now carries the one explicitly accepted target described above; structural/synthetic fixtures never supply its evidence.

Run on the completed full candidate and exact final catalog:

```sh
python3 scripts/compatibility/catalog_capacity.py \
  --candidate candidate.json --catalog final-catalog.json --out capacity.json
```

This measures actual catalog bytes and calls the real bounded reader and catalog
schema validator, and requires a row bound to this exact candidate identity;
exit 2 means it cannot certify fit. Without `--catalog`, it
reports only measurements and exits 2. It never edits/adopts the catalog. The
candidate report includes full-cohort envelope size and a lower bound excluding
only `url`/`provenance`, exactly the fields omitted by `cohort_id`. This is a
measurement, not an alternative subset cohort: all packages, file maps and source
identities remain. The lower bound excludes unknown future runtime/evidence and
other rows; being below the limit is **not** a final fit claim. If the completed
catalog exceeds the reader, stop for coordinated format/reader review. Do not
raise the cap blindly, prune the installed lock, drop firmware/source checks or
set candidate state manually. Capacity success is not native qualification.

Candidate ID is the canonical JSON SHA256 of distribution, suite, architecture,
whole package/source/hash/file inventory and declared processes, excluding proof
locations (`url`/`provenance`). Refreshing a signed index without changing payload
bytes does not create a new cohort. An event ID combines candidate and stable
failure reasons, with a bounded deduplication history.
The six-hour GitHub workflow writes only artifacts/cache, has read-only rights,
and is not deployed until this branch is reviewed and merged. No runtime path
fetches a catalog or package over the network.

## Bundle, catalog and running session

`companion/bundle_contract.py` is the Python policy authority.
`classify_compatibility(installed, running, manifest, catalog)` is pure and returns
`ready`, `logout_required`, `unsupported` or `invalid_bundle` with a stable reason
in its default running phase; the explicit prelaunch phase can return `eligible`.
Measurements are separate IO adapters. V2 manifests bind every helper, selector,
installer, catalog and ELF by exact digest/mode; the generation is the SHA256 of
the final manifest. The payload hash binds all executable components except the
catalog; the manifest binds the catalog separately. One package can contain
multiple cohorts for that exact payload. If an older ABI requires a different
payload, retain its immutable companion package and evidence; do not attach its
cohort to this package's binary or install COSMIC version holds.

Catalog policy **v2** separates the unchanged complete qualification cohort from
`runtime_target` v1 (`scope: islands-runtime`). Runtime assurance is deliberately
narrower than whole-host attestation. The original candidate ID, complete package,
source and file maps remain mandatory and unchanged. Whole-OS archive equality
is **not** a login/Settings prerequisite. Chrony keys and the original base-files
os-release archive hash are not runtime inputs. Public installed os-release is
only a bounded distro selector, not a substituted archive digest.

A target binds qualification cohort, evidence ID, finalized payload, runtime
profile and explicit equivalence-list identities. Its selected packages retain
exact versions, architectures and authenticated binary identities. Each public
`installed_files` entry declares a size, SHA256, kind, source identity, original
package/archive path, resolution policy and dependency rationale. Every digest
must exist in the unchanged full archive map. No transformed-resource or
unreadable-file exemption exists. Required resolved paths stay under public
/usr, /lib or /lib64 roots; root-owned runtime files may not be group/world
writable. Other packages may change without becoming a fabricated new full cohort.

`installed_identity` returns `halcyon-installed-runtime` v1 with the explicit
scope, verified target IDs and measured work, never a verified installed full
cohort. Package version/architecture is only one necessary condition, not proof
of live code. Unknown loader environment overrides refuse. `work` is the exact
sum/count of declared files. IO adapters enforce that budget across declared
alternatives; a live phase additionally allows one manifest-bound host ELF under
the unchanged 512 MiB per-file cap. New phases hash content anew. They retain open
descriptors and metadata plus entry/symlink watches, deduplicate only already
hashed pinned objects inside that phase, and reject replacement, in-place writes,
ABA, overflow or changed executable mappings. Path/stat values never establish
content acceptance by themselves.

Each native target declares `runtime` v4 and evidence-bound
`runtime_compatibility` bindings. The runtime profile contains:

- `processes`: the complete declared required process inventory (explicitly separate from the frozen discovery process seed). Each entry has `executable`, ELF `sha256`,
  `count` (1–64 for session dependencies, `"configured"` for panel children), explicit `lifecycle` and an allowlist `mapped` of executable mapping paths and
  digests. Include relevant applets/services, not just the four mandatory session peers: cosmic-comp, cosmic-session,
  cosmic-settings-daemon and cosmic-notifications. `session` dependencies must already exist before launch; the four
  core processes always use this lifecycle. `panel-child` processes are created
  by this panel and must be absent before launch, then present in the frozen effective configuration
  counts before readiness. A qualified but deliberately unconfigured child has
  an expected count of zero, while a configured child must actually appear. All panel-owned applets must use `panel-child`.
  `cosmic-panel` and `cosmic-panel-islands` are forbidden as dependencies: the
  host is identified separately. Paths must match procfs's resolved names.
- `constraints`: explicitly qualified measurable constraints. Supported fields
  are `kernel_release` from `/proc/sys/kernel/osrelease`,
  `kernel_version_sha256` of `/proc/version`, and `module_versions` mapping loaded
  module names to sysfs `version`/`srcversion` strings. These strings are not
  kernel/module/firmware byte attestations. Omitted constraints make no claim.
- `unmeasured`: the explicit list `previously-read-resources`, `kernel-bytes`,
  `driver-firmware`, `anonymous-code`. Native qualification must assess whether
  those limits are appropriate for the target; the classifier never labels them
  observed package bytes.

A profile ID is its canonical JSON SHA256. Each accepted binding has `id`,
`evidence_id` equal to the target's evidence ID, and a nonempty `reason` recording
its qualified equivalence/scope. Profiles referenced by bindings must exist in
this catalog and preserve the complete process names, paths, count sources, lifecycle
roles and configuration policy. A core-only equivalent profile cannot hide required panel children.
Unchanged-code package rebuilds may deliberately share a qualified
profile. Resource-only changes need explicit scoped qualification; package
versions or a unique catalog row never establish running identity.

`running_identity` returns `format: halcyon-runtime-measurement`, `version: 4`,
`phase: prelaunch | running`, `verified` and **all** matching `profiles`, never a full installed `cohort`.
Each match also carries `configurations[profile_id] = {sha256, children}`: a
fingerprint of observed configuration/desktop inputs, outputs and PATH, and the
exact derived expected child counts (including zeros). The pure classifier and
qualification validator require that configuration record and an explicit target
binding to a measured profile. Configuration changes do not change the catalog
profile or require a new installed package.

Every profile declares `configuration` with `resolver:
"cosmic-panel-ca20ce3-islands-v2"`, a qualified `max_outputs` bound, `native_source` and `applets`.
`applets` maps desktop file stems to `{process, args}`: a declared panel-child
executable and its exact allowed argument vector. Multiple desktop IDs can map
to one executable; all child executables must have a binding. Even an empty
child policy must measure the real configuration and reject configured applets;
there is no core-only escape from configuration checks.

`native_source` must equal `bundle_contract.CONFIGURATION_SOURCE`: the pinned
upstream revision and exact SHA256s of the reviewed companion patch, core
`lib.rs` and resulting patched `islands.rs`. Bundle verification also checks the
manifest's upstream against this contract. These assumptions enter the runtime
profile ID, catalog digest and frozen qualification request; existing manifest,
payload and ELF hashes bind that target to exact bundle bytes. This is a source
contract, not proof that the ELF was compiled from those inputs. Final build
provenance and independent native qualification must establish that relationship.
The source-pin regression checks tracked patch/core bytes and reconstructs the
added `islands.rs` bytes without compiling or editing them.

`panel_configuration.py` implements a bounded supported subset of the pinned
panel's no-argument startup path, read-only:

- Resolve `com.system76.CosmicPanel/v1/entries`, then each named bar's `name`,
  `output`, `plugins_wings` and `plugins_center`. Read user `XDG_CONFIG_HOME`
  overrides first; use the first existing namespace directory in XDG data
  home/directories for per-key system defaults. Missing entries use the pinned
  built-in Panel/Dock lists, not an empty list. Missing per-bar fields use pinned
  struct defaults. Options, lists, tuples, quoted strings, trailing commas and
  nested comments are supported. Invalid/unsupported RON declines admission
  instead of guessing which upstream error/default path would win.
- Count all three plugin regions per effective bar. Query actual `wl_output` v4
  names over a separate bounded registry connection, never inherited session
  descriptors, DRM guesses or a second host. `All` multiplies by live outputs,
  and `Active` creates one bar. Native `Name` creates a bar only on a matching
  output, **except** for valid enabled Islands on `Dock` or `Panel`: the patched
  `islands.rs::reconcile_outputs` keeps one bar on the preferred output or one
  deterministic fallback (built-in eDP/LVDS/DSI first, then lexical name). Its
  configured plugins remain required. Output count must fit the qualified scope;
  implementation support is not native multi-display proof.
- The output query has one one-second deadline across connect, requests and
  replies, a 32-output / 4096-event bound, and one contiguous client object-ID
  allocator shared by registry, outputs and both sync callbacks. The second sync
  follows all output binds; callback IDs are never reused on this short-lived
  connection. Missing names, old output protocol, removal or name changes refuse
  admission. `wl_display.error` records object/code and a bounded diagnostic:
  fixed protocol messages are retained, arbitrary server text is represented by
  length/hash rather than potentially private paths. Tests include a disposable
  libwayland server; this is protocol evidence, not native COSMIC qualification.
- Read/watch/fingerprint each supported bar's
  `XDG_CONFIG_HOME/cosmic/io.github.desktopcards.DockIslands/v1/{Dock,Panel}`
  (HOME/.config only if XDG_CONFIG_HOME is unset; no system-data fallback).
  Missing means default-disabled. The complete canonical Settings RON schema is
  validated: required/unknown/duplicate fields, bool and enum types, version 1,
  gap 0–64, both three-section permutations, positions 0–10000, at most 32 saved
  output layouts, and nonempty output names of at most 128 UTF-8 bytes without
  control characters. The fixed arrays in `together.order`, `separate.order`
  and `separate.positions` require three-element RON **tuples** `(a,b,c)`;
  square-bracket lists are native-invalid. Compact and pretty output from the
  shared Rust `Settings` serializer are covered by retained, Rust-loaded
  [fixtures](../companion/fixtures/settings-generator/README.md), including
  saved-output layouts. This fixture host does not qualify native COSMIC.
  Recognized invalid/oversized/unreadable sidecars disable
  **Islands fallback**, leaving native All/Active/present Name behavior intact.
  Noncanonical RON extensions outside this bounded reader reject the snapshot
  rather than guessing disabled; this prevents an enabled native extension from
  silently being counted as zero. Other entry names have no Islands fallback.
  All/Active still watch sidecar edits even though their count is unchanged.
- Mirror pinned desktop search order: XDG application roots, sorted direct
  files, then queued nested directories; match file stems, selecting the first
  matching path for each unique plugin on a bar. Malformed higher-priority
  overrides never silently fall through. Decode desktop escapes, split direct
  `Exec` using the pinned shell-word subset (no shell execution or field-code
  expansion), resolve executable PATH order, and require the exact qualified
  resolved executable and arguments. Hidden/TryExec flags are not used as an
  absence exemption: this panel path itself uses Exec directly.
- Scope is native direct ELF applets. Flatpak/wrapper indirection, symlinked
  config/desktop trees, duplicate plugin IDs within a single bar, duplicate or
  ambiguous names/keys, localized Exec, raw-string/extended RON syntax and
  ambiguous Exec escapes/comments are unsupported and select stock before
  launch without a crash marker. Repeating a supported applet across Panel,
  Dock or another supported bar is allowed. The resolver never edits layouts.

Snapshot dependencies are watched with bounded inotify coverage **before** they
are read, including absent higher-priority entries and directory replacements.
The launch guard retains that snapshot across preflight and readiness; changed
then restored files still invalidate it. This includes existing, newly created,
replaced, deleted or change-and-restored Islands sidecars. Output names are also rechecked.
A startup configuration race tears down/falls back without a persistent crash
latch. A boot/login-scoped `fallback-session` marker preserves the existing
fresh-notification-descriptor restart route while preventing same-login retry
loops. The next login may retry the unchanged qualified artifact. Once readiness
has passed the startup watch is closed; normal live layout edits are allowed,
and a subsequent real host crash still creates the persistent crash latch.
Stable configured missing/excess/foreign children and code mismatches remain
startup failures with the existing persistent latch. Read-only status measures
the current effective configuration, so an in-progress live reconfiguration is
unknown until its exact owned child set stabilizes.

`prepare_launch` verifies the bundle and complete installed inventory, including
applet bytes, then derives/freezes the effective supported configuration and requests the
`prelaunch` measurement. That phase measures all
session processes/constraints, requires absence of every declared panel-child
process in the target XDG session, and directly checks each future child's
installed ELF against the selected profile. Matching, old, wrong or unreadable
leftover applets block eligibility; arbitrary missing session dependencies do not
count as measured. `classify_compatibility(..., phase='prelaunch')` can return
`eligible` with `qualified_prelaunch`; it never returns `ready`.

The launcher freezes that verified contract, configuration and eligible profile IDs for
one launch, retains the one-host lock and descriptor-isolated ABI probe, then
starts the actual host once. During the existing readiness deadline, child-owned
Capabilities AND the `running` phase must pass. The real host's procfs PID,
starttime, ELF, uid and XDG session are pinned; it must be the supervised process
group/session leader. Every panel child must have the expected bytes, executable
mappings and count, descend from that exact host, and remain in its group/session.
The host identity is checked again after bounded process snapshots. A foreign
same-session applet, old inode or reused PID cannot substitute for a child.
Session dependencies/constraints are measured again, so disappearing cores are
not ignored after launch. Only a profile admitted in prelaunch can become ready.
No second host is launched to discover identity.

A child failure/mismatch remains unready and exhausts the existing timeout.
One read-only measurement may be in flight; its file IO cannot extend that
deadline and any late result is discarded. The
supervisor tears down its host group, preserves the crash latch and fresh-session
notification restart/fallback rules. Read-only status obtains the current host
from authenticated IPC ownership and measures its actual children; prelaunch
eligibility is never displayed as Ready or listed as running profiles.
Staging still checks installed qualification only and reports
`runtime_readiness: not_measured`; it neither launches nor asserts eligibility.

It measures every declared process in the same uid/XDG session. Executable bytes
come from the live `/proc/PID/exe` inode, even after package replacement. Mappings
come from map_files or an open file with the exact procfs device/inode identity;
changed/deleted inaccessible libraries fail closed. Two bounded snapshot attempts
check process starttime, executable and mappings; no process/lock waits occur.
Unrelated exited/zombie processes are ignored; unreadable, disappearing,
duplicated or missing required processes remain unknown (Linux truncated comm
is handled). Multiple matching profiles are safe and do not create ambiguity
about an unmeasured complete qualification lock. Unknown or incompatible running code requires
another login. A responsive IPC socket cannot override admission.

Native drivers can collect the supported read-only measurement with
`python3 /usr/lib/cosmic-ext-halcyon-islands/runtime_identity.py --catalog
/usr/lib/cosmic-ext-halcyon-islands/catalog.json` in the actual target session.
The default phase is `running`; it authenticates the existing host via IPC and
procfs and never creates one. For prelaunch diagnostics add `--phase prelaunch`.
Use only the `running` phase `runtime` object in raw qualification observations; capture the process inventory and
constraint evidence independently. This command never installs, launches or stages.
Runtime ABI probe, inherited notification isolation, child-owned IPC readiness,
one-host locking/supervision and crash latch remain additional gates.

New launcher execution requires V2 measured admission. Legacy source metadata is
readable for ownership migration and old manual-install status, without expanding
V2 admission. The actual old copied 0.2.0 launcher remains byte-for-byte untouched
until a qualified generation can be successfully staged.

The catalog binds a cohort to an exact payload and preallocated evidence ID,
complete image SHA256 and scope. This avoids a circular hash in which evidence
contains a package digest while that package embeds the evidence digest. A row
is a **reviewed qualification target**, not permission to publish. The detached
final result and explicit approval bind the complete final deb, manifest,
catalog, payload and native observations. The current row records the parent-accepted focused native outcome; it does not assert unexecuted scenarios.
Catalog authoring and final evidence review remain maintainer-controlled.

## Finalize, qualify, promote

1. Build pinned source with `companion/build.py --revision <full-40-hex>
   --archive-sha256 <64-hex> --archive <file> --workdir <new-dir>
   --target-dir <isolated-target> --offline`. Patch uses `--fuzz=0`.
2. `packaging/islands/build-deb.py --binary <ELF> --manifest <source-manifest>
   --licenses <notices> --version <monotonic-version> --catalog <reviewed-catalog>
   --out <dir>` strips and finalizes **before** qualification. A preliminary build
   may establish payload hash for catalog authoring; only the final complete
   package is submitted for acceptance. Existing version filenames cannot be
   replaced by different bytes. Keep all corresponding source and notices.
3. Supply a `halcyon-native-image` v1 JSON descriptor: `kind:
   complete-login-session`, `sha256` (disk), `recipe_sha256`,
   `retained_inputs_sha256`, `cohort_id`, and a precise `graphics` scope. Retain
   signed ISO/index/package inputs, full package map, firmware/kernel, recipe and
   runner versions independently. Software VM is not physical GPU qualification.
4. Run:
   ```sh
   python3 scripts/compatibility/qualify.py \
     --candidate verified-candidate.json --image retained-image.json \
     --package final-islands.deb --main-package final-main.deb --out new-evidence-dir \
     --driver /absolute/path/to/reviewed-native-driver
   ```
   Without `--driver`, this produces the exact immutable request and a blocked
   result, never a simulated pass. The driver is parent-owned infrastructure,
   selected independently of candidate code. It receives `--request <json>
   --out <dir>`, a fresh private HOME, only PATH/locale/TMPDIR from the environment,
   and a 90-minute bound. The request fixes exact package/manifest/payload/catalog,
   image/cohort and mandatory scenarios. Driver outputs `observations.json` and
   relative digest-bound evidence. Packages are not rebuilt inside this step.
5. `native_scenarios.py` requires independently observable login, applet input,
   rendered bars, notifications, one host, logout/crash/fallback, hung and
   responsive-but-absent hosts, descriptor failure, reset/Undo, legacy Repair,
   manual modes, real native Auto, four materials, radii, vertical bars,
   transparent gaps, partial packages, running-old/installed-new, actual 0.2.0
   migration, multi-generation Disable and disabled/new/external users. Each
   scenario needs native=true, result=pass, explicit observed/expected assertions,
   and hashed artifacts of its required kinds. Supplying `--main-package` adds
   `updates-discovery`, `updates-authorization-cancel`, `updates-apply-relogin`
   and `updates-failure-recovery`; exact evidence kinds are in SYSTEM_UPDATES.
   The final release requires these integrated scenarios and exact main bytes.
   Missing/blocked scenarios fail
   closed. Fractional-scale, multi-monitor and physical GPU are separate claims.
   Unit fixtures validating this schema are explicitly not native evidence.
6. Review the raw observations, images, logs and every failure. Write an explicit
   approval JSON with `decision: approve`, `package_sha256`, `evidence_sha256`
   (result.json), `inputs` identical to the frozen request, and `review_reference`.
   Run `qualify.py --package <same-deb> --main-package <same-main-deb>
   --evidence <dir> --approval <file>` to recheck every observation/artifact.
   No automated status or successful build creates this approval.

The existing real infrastructure is the parent's
`ci/qualification-probe-021` retained-image runner. Its documented flow verifies
the retained artifact/ISO/APT/disk/firmware hashes, creates a qcow2 overlay, boots
QEMU with private QMP/QGA sockets and `-nic none`, and injects final debs through
read-only ISO media. Adapt that runner to the above request/result protocol;
never mount a live user home or give the guest publication credentials. The
original reports and scripts remain read-only in the sibling infra worktree.
The recorded 1787-package software-graphics guest and old frozen-package runs
are baseline inputs, not acceptance of this change. Artifact retention currently
expires October 30; the parent must preserve it before running final acceptance.

Release assembly and review run locally against retained final artifacts.
Run `scripts/compatibility/release_gate.py --artifacts /path/to/final-artifacts
--manifest-sha256 REVIEWED_SHA256` before publication. No tag-triggered build or
intermediate Actions promotion artifact is part of this path. Signed APT
publication and renewal use `apt-publish.yml`; follow [APT operations](APT-OPERATIONS.md).
`release_gate.py` requires main, companion and corresponding-source hashes and
the acceptance or review required by the selected release policy.
A `system_updates_acceptance: passed` string grants nothing. `inputs` is a derived
identity envelope repeated exactly in request, raw observations, result and
approval: package/manifest/payload/catalog, candidate ID and full candidate hash,
image ID and full image descriptor hash, graphics scope, evidence ID, runtime
contract hash, final main package hash and extracted main identity hash. Summary fields are rederived from the
request; promotion also directly verifies final extracted bundle/catalog and
complete final package bytes. Raw observations must include the measured configuration fingerprint/child counts and `runtime_measurement`
matching the target profile in measurement v4 `phase: running`, actual guest
package inventory and all scenarios. A prelaunch result or a missing configuration measurement is explicitly rejected.
For an integrated run, request `main` is extracted from the actual final
`cosmic-ext-halcyon` deb: package digest/version, every regular `/usr`/`/lib` file
hash, `/usr/bin/halcyon` path and executable digest. Raw `main.installed` must
contain that measured installed identity. Raw `main.running` must contain
`executable`, the live inode's `sha256`, `same_session: true`, and `evidence`
references to hashed `processes` artifacts proving that observation. The driver
must read `/proc/PID/exe` and verify the actual session, not copy an installed
filename's digest. The release gate re-extracts the exact final main deb too.
The synthetic pipeline tests echo fixture identities only inside temporary test
runs; they are neither a native driver nor reusable qualification evidence.
The detached approval hashes result.json, which hashes observations.json and the
canonical request. No catalog digest cycle or post-test repack is permitted. No signing/publishing credentials enter this workflow.

## Generation lifecycle and diagnostics

The package's XDG autostart invokes `refresh.py` independently of Halcyon widgets
or Settings. It runs only in a COSMIC session, from locally installed package
bytes. Root package scripts never touch homes or restart bars. The conffile
respects administrator changes across package upgrades.

`stage_installed_update(home, installed_bundle)` returns `unchanged`, `staged`,
`disabled`, `external_change` or `blocked`, plus a next-login flag. It verifies the
installed package version/root-owned bundle, intended installed runtime target, existing
owned enable record and all effective current-file hashes/modes. It shares the
CLI's nonblocking per-user `.islands.lock`, copies a whole generation, fsyncs it,
then journals and atomically replaces selection/selector through `install.py`.
Selector bytes and mode come from the verified immutable generation snapshot and
are digest-checked before the transaction; later package replacement cannot mix
a new selector with the selected old generation.
A changed payload under the same version and all downgrades are refused.
Incomplete journal layers are rolled back with the existing guarded replay.
External edits block automatic repair. Orphan staged generations are harmless;
there is no automatic cleanup of saved recovery evidence.

The stable selector verifies a single content-addressed generation before
importing it. It launches only its already resolved generation. Staging is handled by the
independent package-owned autostart bootstrap, not on the login panel path. It never swaps a live host. The guarantee is **the
next login after successful staging**, possibly the second login after the
original package upgrade. Crash markers are never cleared. New/disabled/manual
users are never automatically enabled or converted. Unsupported XDG layouts are
blocked, not redirected into another user's state. Production Enable checks that
`~/.local/bin` precedes `/usr/bin` in the session PATH before creating a lock or
journal. A failed prerequisite explains the required PATH; no shell file is edited.

Every selection change is a layer in the original checksum/mode-guarded
installation chain. `halcyon-islands disable` unwinds these layers, then the
original 0.2.0 installation. Existing layout, baseline/Undo and crash markers
remain intact. Package removal leaves a verified retained generation usable only
for its known cohort. If the CLI itself was removed, the retained generation's
`install.py --restore <install-record.json>` remains available; restore owned
records newest-first. No removed package's leftover files become manual
installation provenance.

A directly copied manual launcher can lack generation helpers. It declines
legacy/invalid manifests before importing them and catches missing-helper imports
as stock-fallback conditions; no legacy version-only admission is restored.

`halcyon-islands status --json` and `diagnostics` expose versioned installation,
compatibility, selected/installed generation and reason separately. Settings
consumes this on its existing background worker and offers Copy diagnostics.
Read-only diagnostics contain no paths, config contents, session IDs, tokens or
exception text. Absent installations still have exactly one Get Islands link;
installed disabled/broken/incompatible states retain management.

## Local regression commands

```sh
python3 -m unittest discover -s scripts/compatibility -p 'test_*.py'
python3 -m unittest discover -s companion -p 'test_*.py'
python3 scripts/test_panel_companion.py
python3 packaging/islands/test_islands.py
```

For actual released-package migration tests, set `HALCYON_RELEASED_020` to the
immutable public `cosmic-ext-halcyon-islands_0.2.0_amd64.deb`, SHA256
`1ec258de00228f02d37fa6201bd4ac4050344594cb452ad6d05ada459ea81c5e`.
`companion.test_refresh.Actual020` extracts and invokes the actual shipped
installer, copies its actual launcher and ELF, migrates, then reverses multiple
generations. With no supplied package those tests explicitly skip; fixture tests
never stand in for them or real-session migration/notification acceptance.


## Runtime target authoring and bounded verification (0.2.5, publication ON HOLD)

This boundary trusts the root-managed OS, Python, kernel/procfs and package
manager. It is a reliability gate, not a sandbox against root or the logged-in
user. It does not attest previously read resources, kernel/firmware bytes or
anonymous/JIT code. Graphics scope must be justified by exact code/provider
mappings, supported protocol behavior and discriminating measured constraints;
if those cannot distinguish the reviewed backend, do not author an admission row.
A GPU/VM name or one successful trace is insufficient.

Author from explicit native roots: the companion ELF, all four session peers,
all supported applet Exec bindings, their ELF interpreters/DT_NEEDED closure,
conditionally loaded EGL/GLES/driver/loader providers, and public resources
identified by the reviewed pinned source. Keep source-review records for dlopen,
loader resolution/configuration, protocol globals and resource consumers. Include
all supported configuration branches, not just paths seen in one session. No
package-name regex or automatic "whatever is installed" authoring exists.

The small offline checker is runnable without installation or a guest:

```sh
python3 -B scripts/compatibility/author_runtime_target.py \
  --catalog final-bundle/catalog.json --cohort EXACT_FULL_COHORT_ID \
  --root public-sysroot --bundle final-bundle \
  --dependency-review dependency-review.json --native-review native-scope.json \
  --out authoring-audit.json
```

The sysroot is a disposable **public runtime-file** export, with declared
resolved bytes at their paths, not a whole guest extraction or private-file
copy. Escaping/broken symlinks refuse. `dependency-review.json` has format
`halcyon-runtime-dependency-review`, version 1, source-review references,
`roots`, `dynamic_providers`, `resources` equal to target dependency-review lists,
and `objects` keyed by every code path plus `@payload`. Each object declares
`interpreter`, `needed`, `rpath`, `runpath` exactly as bounded readelf observes,
and `resolved` maps every needed SONAME/interpreter to a declared code path.
The checker never executes ELF/ldd. It verifies all declared file bytes and graph
coverage; conditional providers and actual loader choices still require source
and native review. `native-scope.json` uses format `halcyon-runtime-scope-review`,
version 1, graphics, exact constraints, source_review and runtime_discriminators.
Both exact record hashes are in the target. The audit always says
**STRUCTURAL-NOT-QUALIFIED**; it never changes a catalog or candidate state.

Qualification requests bind runtime target/profile/equivalence and exact companion
binary identity in `inputs`, along with the unchanged complete candidate and final
main package identity. Raw observations require `installed_runtime` for this exact
target, v4 `runtime_measurement` with the actual host SHA256, configuration counts,
and both `runtime_reviews` artifact references (`dependency`, `native`, kind
`runtime-review`) whose hashes match the target. All existing native scenarios,
final main-process evidence, approval, repack rejection and release gates remain.
Catalog v2 contains the single accepted focused native target described above. Structural/synthetic fixtures cannot qualify additional targets.

Each status/launch operation owns one verified immutable catalog context. It
shares parsed objects for identical catalog bytes, still verifies captures at
distinct locations, and closes all watches at completion. There is no disk cache,
service or cross-request content acceptance. Intact unchanged staging returns
before runtime measurement. A new stage still uses a fresh verified copy,
monotonic version and guarded selection transaction; replacing the source package
after a complete immutable copy cannot splice a different selector into it.

The selector gives capture + prelaunch + ABI probe one 5-second budget, with
stock fallback on refusal/timeout. The ephemeral verifier runs separately, with
no inherited session/notification descriptors, and is canceled/reaped at cutoff.
The supervisor owns all host/session descriptors. After spawn, the existing
15-second readiness deadline includes measurement; it requires the actual host,
allowlisted executable mappings, child-owned Capabilities IPC and configured
owned children. Prelaunch requires zero panel-owned children and checks their
installed bytes; it does not wait for children before starting their parent.
The supervisor independently watches frozen config inputs so a stalled verifier
cannot turn a detected layout ABA into a persistent crash latch. Normal missing,
foreign, excess or wrong-code child failures retain the persistent latch.

Positive native timing remains a release gate: cold + two warm + ten warm calls,
status ≤5 seconds cold/≤2 warm (outer 6 seconds unchanged), prelaunch including
probe ≤5 seconds and cleanup ≤1 second, post-spawn readiness ≤15 seconds (warm
target ≤5). Record actual target count/bytes, per-phase unique reads, RSS, host
creation/teardown and descriptor/worker counts; RSS targets are 256 MiB for one
full row and 384 MiB for two. Refusals and fixture timings are not positive native
acceptance. If a genuine required target cannot meet these bounds, keep it
blocked and report the measured bottleneck; never omit required checks to pass.

### Runtime cancellation and Settings observation lifetime

The prelaunch verifier uses a catchable parent-death cancellation signal so its
ABI subprocess is killed and reaped when the outer launcher exits, including
forced outer death. The probe also has its own kernel parent-death kill binding;
forced worker termination cannot leave a live probe mistaken for a panel.
Session descriptors remain isolated. This does not change host supervision,
readiness, notification routing, latches or stock dispatch policy.

One Settings refresh shares an absolute six-second budget across presence,
compatibility and read-only IPC. IPC reuses that operation's installation
observation; Checking/timeout does not trigger another verification. Its socket
is nonblocking, replies are bounded, and trickled bytes cannot restart the budget.
This connection accepts only Capabilities, MaterialSupport and Inspect. All
transaction/write connections still obtain fresh installation authority. Known
installation controls remain available while Checking, and timeout requires an
explicit retry rather than repeated focus-triggered work.


### Bounded device scope and resource authoring

A runtime profile may declare `constraints.drm_devices`: a canonically sorted,
exact inventory (1–8 PCI display devices, including duplicate identical devices).
Each record has `vendor`, `device`, `subsystem_vendor`, `subsystem_device`
(lowercase `0x` plus four hex digits), `pci_class` (`0x03` plus four hex digits),
ordered owning-device-to-PCI `drivers`, `virtio_features` (exactly 64 or 128
exposed bits, or null only for non-virtio devices), and `primary_nodes`/
`render_nodes` counts. Card
numbers and PCI bus addresses are not compatibility identity. Extra devices,
unbound/unsupported ancestry, unreadable attributes or different values refuse;
there is no missing-device exemption. This is a discriminator for an explicitly
qualified scope, not generic virtio or GPU support.

The ordinary-user reader opens only public sysfs attributes, never DRM device
nodes. It bounds discovery to 256 class entries, 16 nodes, 256 direct PCI children per
node and 256 bytes per attribute. PCI identity requires its bus/subsystem and
bus-device links, not the presence of vendor/device attributes. Only two explicit
layouts are supported: DRM directly below PCI or below its direct virtio child.
For virtio-pci, both require exactly one direct virtio-bus GPU child (vendor
0x1af4, device 0x0010), its actual bound driver and complete feature value. Missing,
ambiguous, foreign or unbound children refuse, including PCI-parent DRM. Child
attributes, bus/driver links and inventory identities participate in both phase
snapshots. The finite feature widths preserve the prior 64-bit representation
and all 128 bits actually exported by the pinned 7.2.6 guest; other lengths refuse.
This captures the exposed ABI value without asserting unobserved kernel/firmware
semantics or inventing the uncaptured native child-driver binding. FIFO/device
files and escaping links refuse.
Runtime checks take fresh before/after device and kernel/module observations
within each verification phase; device path/inode changes invalidate that phase.
They do not persist a path/stat acceptance cache or attest past hotplug history,
firmware bytes, GPU state or an actual renderer string. Existing process mappings,
installed bytes, kernel/module constraints, timeouts and stock fallback remain.

In the hash-bound native scope review, a device-constrained target additionally
requires `device_observation` with format `halcyon-drm-device-observation`,
version 1 and `devices` exactly equal to the declared inventory. `graphics`
continues to describe the actual observed EGL/GL context in native evidence; it
is not substituted for device measurement. Record actual compositor and companion
contexts in qualification. Do not infer renderer values from PCI IDs or qualify
other GPUs from a virtio result. Old helpers refuse the new constraint; repack
helpers and rebind payload/target/evidence rather than updating an old artifact.

Resource review distinguishes code selection from user data. EGL vendor JSON,
loader resolution and PipeWire/SPA module selection affect executable providers
and must be accounted for with source/native evidence and exact allowed code.
Generated loader caches are routing observations, not substituted package-file
hashes. A default provider config alone does not account for effective overrides
or future optional dlopen branches. Keep an incomplete target non-admissible.
Fonts, icon themes, translations and keyboard layout data do not select arbitrary
ELF providers merely because the program reads them. Preserve supported user
styling/layout variation; do not pin every installed font or user theme. Review
their parser/code dependencies and test relevant fallback behavior. Custom native
plugins or applet executables still require explicit supported code policy.


## 0.3.0 final-package acceptance

Policy v2 pins the newly built companion and its exact patch. Final main and
Islands packages must match a separately reviewed
`companion/compatibility/evidence/accepted-final-030.json` record. Until that
record exists, publication fails with `final_native_acceptance_pending`.

The record binds both package and extracted ELF hashes, all companion helpers,
the payload and catalog, and the retained native evidence archive. It requires
observed installation, Settings, startup, input, owned cleanup, stock recovery
and relogin in a complete COSMIC 1.11 login session with virtio/llvmpipe graphics.
The parent reviews the raw observations before recording acceptance. A candidate's
own `passed` fields cannot replace this record in the frozen release source.

The final artifact approval additionally binds that acceptance record, its raw
evidence, the corresponding source and the release manifest. Changed packages,
helpers or evidence require new acceptance; repacking cannot reuse an approval.
Physical GPUs, COSMIC 1.10 and the full historical scenario matrix remain outside
this focused scope. Historical 0.2.5 acceptance and published APT-renewal pins
remain unchanged.

## Historical 0.2.5 packaging and practical-policy release gate

The following gate binds the original 0.2.5 ELFs. Policy v2 requires a fresh
companion build and new final-artifact acceptance; it must not reuse the old
ELF qualification or approval. The historical gate intentionally rejects new
binaries until the next release acceptance is recorded.

Reuse the existing accepted main and companion ELFs; Python policy changes need
new helper/package manifests, not native rebuilds. Build main and Islands packages
from the same frozen source with the normal packaging scripts. Package version
is checked against dpkg and for monotonic staging; it is not a native ABI identity.
The manifest binds the exact catalog and every helper/ELF byte and mode. The
catalog retains the historical VM row alongside the explicit new startup policy;
a row for an older payload is historical evidence, not admission of the new one.
No remote catalog service or login-time network is needed (about 25.8 MB locally,
under the unchanged 64 MiB catalog bound).

For this intentionally revised product policy, final `release-manifest.json`
sets `compatibility_policy: "cosmic-1.10-1.11-startup-v1"`. Keep exact main-deb,
islands-deb and corresponding-source artifact hashes. Generate a **pending**
review request, after building immutable final artifacts:

```sh
PYTHONPATH=companion:scripts/compatibility python3 -B scripts/compatibility/practical_release.py \
  --artifacts /path/to/final-artifacts > /path/to/final-artifacts/approval-request.json
```

Parent's explicit final artifact approval supplies `approval.json` with that
request's bound fields, `decision: "approve"` and a meaningful `review_reference`.
The request generator never approves itself. `release_gate.py --artifacts ...
--manifest-sha256 ...` verifies actual final artifact hashes, current helper/catalog
source, matching main status helpers, unchanged accepted native ELFs, historical
acceptance digest and explicit limitations. Repacking cannot inherit old approval.
The limitations explicitly include physical hardware, COSMIC 1.10, new family
helpers and the full matrix **not** being natively qualified.

The original `qualify.promotion` full-matrix path is unchanged for manifests without
this explicit policy. No synthetic native observation, qualification row or
complete-session pass is generated to satisfy it. Publication still requires the
parent's final authorization; this policy change does not publish anything.
