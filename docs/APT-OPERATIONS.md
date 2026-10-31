# Signed APT operations

This pipeline distributes reviewed releases pinned in `distribution.json` under
their explicit compatibility policy. Published releases retain their original
artifact hashes and approval scope. The older 0.2.1 implementation plan and the
initial 0.2.5 bootstrap examples below are historical.

Since 0.5.5, the same atomic Pages deployment also carries the signed Fedora RPM
repository. `distribution.json` pins its release asset and signed review manifest.
`scripts/rpm/distribution.py` checks the archive inventory, source/evidence
bindings, RPM signature and repository signature before staging `rpm/` beside
`apt/`. Daily APT renewal retains those immutable RPM bytes. See
[Fedora packaging](FEDORA-PACKAGING.md) for its separate native qualification.

## Trust and immutable inputs

`scripts/apt/distribution.json` pins every public GitHub asset by SHA256 and byte
size: original main/companion packages, corresponding source, focused evidence,
provenance, release notes, release manifest and original approval; plus the separate
bootstrap release packages, source and manifest. Downloads are HTTPS, bounded and
hash-checked. Application packages are never rebuilt or re-stripped.

The credential-free preparation job runs the existing `release_gate.verify` with
the original manifest/approval. For 0.2.5 this is the approved
`cosmic-1.10-1.11-startup-v1` practical policy, preserving all its limitation fields.
No replacement native-evidence tree or invented full-matrix passing result exists.

Parent approval of **distribution** is separate: the protected `halcyon-apt`
environment variable `HALCYON_APT_DISTRIBUTION_SHA256` must equal the exact committed
`distribution.json` SHA256. The parent sets it only after inspecting concrete
bootstrap outputs and input bindings. Generating the file or passing preparation
is not approval. A mismatch/missing variable blocks signing.

Bootstrap version is `0.2.5-1`; tag `apt-bootstrap-0.2.5-1`. Publish the exact local
outputs once, without `--clobber`, after parent review. Neither the workflow nor
renewal rebuilds them: it consumes the durable pinned release assets. Source and
preferences match the existing 0.2.5 Rust identity constants exactly. The package
includes dpkg checksum records required for 0.2.5 to verify the owned keyring.
Original v0.2.5 assets are not changed or appended to.

## Signing environment

Already provisioned by the parent:

- GitHub environment **halcyon-apt**, restricted to `main`.
- Secret **HALCYON_APT_SIGNING_KEY**: armored signing-subkey-only export, never the
  certification secret.
- Variable **HALCYON_APT_SIGNING_FINGERPRINT**:
  `610B73343D6F40603553A295A4658CCECBC306AB`.
- Pages workflow hosting at `https://profilaz3.github.io/halcyon/`.

Public files are `packaging/updates/halcyon-archive-keyring.gpg` and
`packaging/updates/archive-key.json`. Public keyring SHA256 is
`bc0db85de8d205c56526c7c5dd439e50f9558c47450f57aa47dd7bf1467b772f`.
Certification fingerprint is `A1EB6576D9595ECA57509CE14EB2B42DF234CB16`.
The public key states these actual expiry times:

- Certification key: **2031-10-23 08:50:23 UTC**.
- Signing subkey: **2028-10-23 08:50:23 UTC**.

The two-year subkey is a deliberate practical maintenance interval, not a 90-day
rotation requirement. The primary remains local in a private 0700 directory;
an encrypted/offsite backup has **not** been claimed. The parent must arrange
that separately. CI never needs the certification secret.

The signing script removes the secret from child-process environment before
verification/import, imports only into a private temporary GnuPG home, checks
structural `sec` dummy-marker `#` and usable `ssb` marker `+`, rejects an exported
certification secret, signs with the explicit subkey, and destroys the temporary
home/agent. It never logs the secret or uploads private key material. Only the
sign step receives the secret. No candidate executable or downloaded source code
is executed there.

## Workflow and retention

`.github/workflows/apt-publish.yml` runs manually or daily at **04:17 UTC**. All
jobs require this repository's default branch to be `main` and the run ref to be
`refs/heads/main`. There are no PR, workflow_run or tag triggers. Actions are
pinned to exact commits. Workflow-wide concurrency prevents overlapping archive
replacements and does not cancel an in-progress deployment.

1. **prepare**, read-only contents scope: download pinned GitHub releases, verify
   original practical approval, authenticate/fetch the previous archive, stage
   exact packages/indexes/AppStream and check size. No signing credential.
2. **sign**, protected `halcyon-apt`: require the independent parent approval pin,
   recheck pool identities and retained bytes, then sign InRelease, Release.gpg
   and the archive inventory. Validate signature/key expiry and package the site.
3. **deploy**, `github-pages` environment: only this job has Pages/id-token write
   scopes. Deploy the complete artifact atomically.

Any preparation, trust, signature, expiry or size failure fails Actions and leaves
the previous site unchanged. No failure path publishes unsigned metadata or resets
an existing archive. Owners should enable GitHub Actions failure notifications;
no external alert destination is silently configured. Scheduled workflows on
inactive public repositories may be disabled by GitHub; monitor the workflow's
run history as part of maintenance.

`--bootstrap` is for the first deployment only. In the public workflow it requires
an actual 404 for the existing archive inventory. A network, authorization or
signature failure is never interpreted as an empty repository. Subsequent runs
require the previous signed archive; an outage fails safely until retried.

Packages live at immutable SHA256 pool paths. All previous versions, source/review
references and by-hash objects remain retained. Debian-equivalent versions cannot
change bytes. Clients holding the old Release can fetch its old by-hash index and
pool package across deployment. Metadata-only renewal does not create different
package/index bytes. There is no automatic garbage collector or silent downgrade.

`archive.json.asc` authenticates the previous archive inventory independently of
HTTPS. Signing verifies its retained pool/index records against the new stage,
rejects deletion/replacement or unreviewed extra packages, and checks the complete
unsigned inventory. Expired Release metadata can be retained for byte-preserving
renewal; it is not served as a valid update by disabling expiry checks.

The published site has a hard implementation budget of **850,000,000 bytes**, below
GitHub Pages' 1 GB cap. Both unsigned preparation (with headroom) and final site
packaging check it. The initial site is about 32.6 MB. Large corresponding-source
and evidence assets remain on their immutable GitHub release URLs with hashes and
visible links; no guest image/sysroot is copied to Pages. GitHub Pages also has a
100 GB/month soft bandwidth limit. Monitor actual usage; if retention/traffic
outgrows hosting, migrate deliberately rather than silently pruning rollback data.
[GitHub Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits).

CI artifacts are short-lived transport between jobs only. Product/bootstrap inputs
are durable GitHub release assets, and retained generations are in the working
public archive. If a site is lost, reconstruct from pinned releases and an operator
retained archive copy, then explicitly review recovery. Do not click bootstrap to
work around a corrupt or temporarily unavailable existing source.

## Runnable parent handoff

From the reviewed checkout, the implementation task produces bootstrap outputs at
`../halcyon-021-evidence-recovery/apt-live-distribution-r1/bootstrap/`. Reproduction
uses a fresh output directory:

```sh
python3 -B packaging/updates/build-bootstrap.py \
  --public-keyring packaging/updates/halcyon-archive-keyring.gpg \
  --version 0.2.5-1 --epoch 1792832400 --out /absolute/fresh/bootstrap
```

The parent reviews the output hashes and then creates the separate release (never
clobber an existing asset):

```sh
gh release create apt-bootstrap-0.2.5-1 \
  --repo profilaz3/halcyon --target main \
  --title 'Halcyon APT enrollment 0.2.5-1' \
  --notes 'Explicit scoped APT enrollment. Does not install or activate Halcyon or Islands. Includes public keyring, source configuration and corresponding source.' \
  ../halcyon-021-evidence-recovery/apt-live-distribution-r1/bootstrap/*
sha256sum scripts/apt/distribution.json
# After reviewing that exact file and bootstrap bytes:
gh variable set HALCYON_APT_DISTRIBUTION_SHA256 --repo profilaz3/halcyon \
  --env halcyon-apt --body "$(sha256sum scripts/apt/distribution.json | cut -d ' ' -f 1)"
```

The code must first be merged to `main`; the parent owns PR/push/merge/deployment
approval. Dispatch initial publication from main:

```sh
gh workflow run apt-publish.yml --repo profilaz3/halcyon --ref main -f bootstrap=true
```

Every later manual renewal uses `-f bootstrap=false`; the daily schedule also uses
normal retention. No signing key is passed on the command line. The workflow
reads the existing protected environment secret itself.

Local **unsigned preparation**, before publication of the bootstrap assets, can
use the reviewed retained inputs without changing them:

```sh
python3 -B scripts/apt/live.py prepare --bootstrap \
  --release-cache ../halcyon-021-evidence-recovery/final-release-025-r1/packages \
  --bootstrap-cache ../halcyon-021-evidence-recovery/apt-live-distribution-r1/bootstrap \
  --work /absolute/fresh/prepared
```

The cache option is only for local review/reproduction; the workflow supplies no
cache path and downloads/checks the real release assets. Local staging does not
approve or deploy anything.

After Pages reports success, parent runs actual public acceptance (private APT
root, installs only bootstrap, downloads app without installing it):

```sh
python3 -B scripts/apt/smoke.py
```

It verifies public candidate versions, bootstrap bytes/keyring, real APT-signed
index/package download, absence of automatic Islands installation, and missing-key
rejection. No host APT/dpkg state, global keyring or desktop is modified. Allow CDN
propagation before retrying a failed public check; never weaken trust to pass it.

## Renewal, expiry and rotation

Each generation has an explicit Date and **seven-day Valid-Until**. Daily renewal
signs fresh Release metadata even without a binary change. The expiry checker
fails at 48 hours remaining for either Release or the signing/certification key:

```sh
python3 -B scripts/apt/check_expiry.py --archive /path/to/signed \
  --archive-sha256 '<sha256-of-archive.json>' \
  --public-keyring packaging/updates/halcyon-archive-keyring.gpg
```

If renewal was missed, rerun normal renewal with retained previous bytes; never
set trusted=yes or disable Check-Valid-Until. Renew/rotate the two-year subkey
well before expiry. Certification and backup operations stay offline/parent-owned.

Rotation uses **public-keyring overlap**, not assumed dual-signature support:
ship a new immutable bootstrap/keyring version containing A+B in an A-signed
archive, allow clients to upgrade, then switch to B-only signing. Later distribute
a newer B-only keyring signed by B. Update the committed public key, fingerprint,
new bootstrap asset pins and independent distribution approval together. Keep the
source/Signed-By path and four-package preference bytes stable for 0.2.5 clients.
Clients that missed the bridge explicitly install the independently verified new
keyring release asset before retrying APT. Retired keys must fail for new-only
clients. Same-key expiration extensions also require a new public keyring package;
changing only the publisher's keyring does not extend client trust.

On compromise, stop publication and use a separately authenticated recovery
channel; the compromised key cannot safely authenticate its replacement. Maintain
operator backups of public archive state and offline certification material.

## Local checks and scope

```sh
python3 -B scripts/apt/test_archive.py -v
python3 -B scripts/apt/test_live.py -v
PYTHONPATH=scripts/apt python3 -B -m unittest -v test_admission
```

These use transient private fixture keys and actual disposable APT/dpkg, never
production private keys or host package state. Live tests include signing-only
secret enforcement, independent approval pin, signed predecessor/hash retention,
extra-package rejection, size limits, exact 0.2.5 source/preferences/md5sum
compatibility, real signed-site/bootstrap smoke and workflow branch/secret scopes.
AppStream collection validation is offline and separate from native Store search.
No Cargo, new native VM/hardware campaign, or change to 0.2.5 binaries is required
or claimed by this archive deployment.
