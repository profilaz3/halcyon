#!/usr/bin/env bash
# Runs inside a disposable Fedora container started by container_acceptance.py.
#
#   container-stage.sh build|fresh|upgrade|upgrade-disabled FEDORA
#
# /work holds the source closure, the previous release and the tools. Each
# stage appends one {"check": ..., "passed": ...} line per check to
# /work/fcFEDORA/results/STAGE.jsonl and exits non-zero if any check failed.
set -euo pipefail

stage=${1:?stage}
fedora=${2:?Fedora version}
work=/work
target=$work/fc$fedora
tools=$work/tools
source_tree=$work/source/main
official_key=$source_tree/packaging/updates/RPM-GPG-KEY-halcyon
results=$target/results/$stage.jsonl
failed=0

mkdir -p "$target/results"
: >"$results"

finish() {
  # Rootful engines write as root; hand the outputs back to the caller.
  if [ -n "${HOST_UID:-}" ]; then
    chown -R "$HOST_UID:${HOST_GID:-$HOST_UID}" "$target" /var/cache/libdnf5 2>/dev/null || true
  fi
}
trap finish EXIT

pass() { printf '{"check": "%s", "passed": true}\n' "$1" >>"$results"; echo "PASS $1"; }
fail() { printf '{"check": "%s", "passed": false}\n' "$1" >>"$results"; echo "FAIL $1"; failed=1; }
# check NAME COMMAND...: COMMAND must succeed. refuse NAME COMMAND...: it must fail.
check() { local name=$1; shift; if "$@"; then pass "$name"; else fail "$name"; fi; }
refuse() { local name=$1; shift; if "$@"; then fail "$name"; else pass "$name"; fi; }
hash_is() { [ "$(sha256sum "$1" | cut -d ' ' -f 1)" = "$2" ]; }
# grep without -q reads all output, so tee never meets a closed pipe under pipefail.
signatures_ok() { "$@" | tee /dev/stderr | grep 'signatures OK' >/dev/null; }
json() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
phase() { json /var/lib/halcyon/updates/rpm-source.json phase 2>/dev/null || echo none; }
snapshot() { python3 -I "$tools/container_acceptance.py" snapshot --root /; }
flip_byte() {
  python3 -I -c 'import sys; p = sys.argv[1]; b = bytearray(open(p, "rb").read()); b[-4096] ^= 0xFF; open(p, "wb").write(b)' "$1"
}

dnf=(dnf -y --setopt=keepcache=True)
# The acceptance repository lives outside /etc/yum.repos.d, so enrollment
# still sees a system without any Halcyon source.
acceptance=(--setopt=reposdir=/etc/yum.repos.d,/etc/fb25-acceptance.repos.d)

. /etc/os-release
if [ "$ID" != fedora ] || [ "$VERSION_ID" != "$fedora" ] || [ "$(uname -m)" != x86_64 ]; then
  echo "Expected Fedora $fedora on x86_64, found $ID $VERSION_ID $(uname -m)" >&2
  exit 2
fi

repository() {
  mkdir -p /etc/fb25-acceptance.repos.d
  cat >/etc/fb25-acceptance.repos.d/acceptance.repo <<EOF
[acceptance]
name=Throwaway acceptance repository for Fedora $fedora
baseurl=file://$target/repo/fedora/$fedora/x86_64/
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=file://$target/test-key.asc
metadata_expire=0
skip_if_unavailable=0
EOF
}

installed() {
  local evr
  evr=$(cat "$target/evr")
  check "$1-evr" test "$(rpm -q --qf '%{EPOCHNUM}:%{VERSION}-%{RELEASE}' "$PACKAGE")" = "$evr"
  check "$1-rpm-verify" rpm -V "$PACKAGE"
  check "$1-app-bytes" hash_is /usr/bin/halcyon "$(json "$target/fedora-build.json" app_sha256)"
  check "$1-panel-bytes" hash_is /usr/lib/cosmic-ext-halcyon-islands/cosmic-panel-islands "$HOST_SHA256"
  check "$1-installation-record" test \
    "$(json "/usr/share/$PACKAGE/installation.json" package_version)" = "$evr"
}

build() {
  "${dnf[@]}" --setopt=install_weak_deps=False install \
    gcc gcc-c++ make cmake pkgconf-pkg-config clang-devel git-core patch cpio curl \
    libxkbcommon-devel wayland-devel expat-devel fontconfig-devel freetype-devel \
    libinput-devel systemd-devel dbus-devel libseat-devel mesa-libEGL-devel \
    vulkan-loader-devel openssl-devel rpm-build rpm-sign createrepo_c gnupg2 python3
  curl --proto '=https' --tlsv1.2 -fsSL -o /tmp/rustup-init \
    "https://static.rust-lang.org/rustup/archive/$RUSTUP_VERSION/x86_64-unknown-linux-gnu/rustup-init"
  echo "$RUSTUP_SHA256  /tmp/rustup-init" | sha256sum -c -
  chmod 0755 /tmp/rustup-init
  RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo /tmp/rustup-init -y --no-modify-path \
    --profile minimal --default-toolchain "$RUST_TOOLCHAIN"
  export RUSTUP_HOME=/opt/rustup PATH=/opt/cargo/bin:$PATH RUSTUP_TOOLCHAIN=$RUST_TOOLCHAIN

  # Application: locked and offline from the vendored closure, empty Cargo home.
  mkdir -p /tmp/empty-cargo-home
  (cd "$source_tree" && CARGO_HOME=/tmp/empty-cargo-home CARGO_TARGET_DIR=/build/target \
    cargo build --release --locked --offline --frozen)
  install -D -m 0755 /build/target/release/halcyon "$target/app/halcyon"
  check offline-build-downloaded-nothing \
    test ! -e /tmp/empty-cargo-home/registry -a ! -e /tmp/empty-cargo-home/git

  # Panel: the previous release's qualified Fedora build, reused unchanged.
  # The startup catalog comes from this tree's policy (/work/fcN/catalog.json).
  local db islands rpm_name new old
  db=$(mktemp -d)
  rpmkeys --dbpath "$db" --import "$official_key"
  check previous-rpm-official-signature signatures_ok rpmkeys --dbpath "$db" --checksig "$work/$PREVIOUS_RPM"
  rm -rf "$target/previous-payload"
  mkdir -p "$target/previous-payload"
  (cd "$target/previous-payload" && rpm2cpio "$work/$PREVIOUS_RPM" | cpio -idm --quiet)
  islands=$target/previous-payload/usr/lib/cosmic-ext-halcyon-islands
  check previous-panel-matches-policy hash_is "$islands/cosmic-panel-islands" "$HOST_SHA256"
  check previous-embedded-key-is-official \
    cmp -s "$target/previous-payload/usr/share/$PACKAGE/updates-rpm/archive-key.asc" "$official_key"
  if [ "$failed" != 0 ]; then
    echo 'Refusing to package unverified inputs.' >&2
    exit 1
  fi

  python3 -I "$tools/container_acceptance.py" provenance --fedora "$fedora" \
    --app "$target/app/halcyon" --host "$islands/cosmic-panel-islands" \
    --rust "$RUST_TOOLCHAIN" --out "$target/fedora-build.json"

  # Throwaway signing key. Packages signed with it must never be published.
  export GNUPGHOME=$target/gnupg
  rm -rf "$GNUPGHOME"
  install -d -m 0700 "$GNUPGHOME"
  gpg --batch --pinentry-mode loopback --passphrase '' --quick-gen-key \
    'Halcyon throwaway acceptance key, never publish <acceptance@invalid>' rsa3072 sign 7d
  fingerprint=$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" {print $10; exit}')
  gpg --batch --armor --export "$fingerprint" >"$target/test-key.asc"

  # The package embeds the official public key, as a release does, so
  # enrollment behaves as it will for users. Only the repository uses the
  # throwaway key.
  rpm_name=$PACKAGE-$VERSION-$RPM_RELEASE.fc$fedora.x86_64.rpm
  rm -rf "$target/rpm" "$target/repo"
  (cd "$source_tree" && PYTHONDONTWRITEBYTECODE=1 python3 packaging/build-rpm.py \
    --binary "$target/app/halcyon" \
    --islands-binary "$islands/cosmic-panel-islands" \
    --islands-manifest "$islands/companion-manifest.json" \
    --islands-licenses "$target/previous-payload/usr/share/doc/cosmic-ext-halcyon-islands/THIRD_PARTY_LICENSES.txt" \
    --islands-catalog "$target/catalog.json" \
    --provenance "$target/fedora-build.json" --public-key "$official_key" \
    --version "$VERSION" --release "$RPM_RELEASE" --fedora "$fedora" --out "$target/rpm")
  (cd "$source_tree" && PYTHONDONTWRITEBYTECODE=1 python3 scripts/rpm/prepare-repository.py \
    --package "$target/rpm/x86_64/$rpm_name" --public-key "$target/test-key.asc" \
    --gnupg-home "$GNUPGHOME" --fingerprint "$fingerprint" --fedora "$fedora" \
    --out "$target/repo") >"$target/repository-candidate.json"
  pass signed-repository-prepared

  new=$(rpm -qp --qf '%{EPOCHNUM}:%{VERSION}-%{RELEASE}' "$target/rpm/x86_64/$rpm_name")
  old=$(rpm -qp --qf '%{EPOCHNUM}:%{VERSION}-%{RELEASE}' "$work/$PREVIOUS_RPM")
  printf '%s\n' "$new" >"$target/evr"
  check evr-sorts-after-previous test "$(rpm --eval "%{lua:print(rpm.vercmp('$new', '$old'))}")" = 1
  rpm -qlp "$target/rpm/x86_64/$rpm_name" >"$target/rpm-files.txt"
  check payload-has-build-record grep -qx "/usr/share/doc/$PACKAGE/fedora-build.json" "$target/rpm-files.txt"
  check payload-has-license-texts grep -qx "/usr/share/doc/$PACKAGE/THIRD_PARTY_LICENSES.txt" "$target/rpm-files.txt"
  check payload-has-panel grep -qx /usr/lib/cosmic-ext-halcyon-islands/cosmic-panel-islands "$target/rpm-files.txt"
}

negative() {
  local db package
  db=$(mktemp -d)
  rpmkeys --dbpath "$db" --import "$target/test-key.asc"
  package=$(echo "$target/repo/fedora/$fedora/x86_64/"*.rpm)
  check signed-package-verifies signatures_ok rpmkeys --dbpath "$db" --checksig "$package"
  cp "$package" /tmp/tampered.rpm
  flip_byte /tmp/tampered.rpm
  refuse tampered-package-rejected signatures_ok rpmkeys --dbpath "$db" --checksig /tmp/tampered.rpm

  rm -rf /tmp/tampered-repo
  cp -a "$target/repo/fedora/$fedora/x86_64" /tmp/tampered-repo
  python3 -I - /tmp/tampered-repo/repodata/repomd.xml <<'PY'
import re, sys
path = sys.argv[1]
text = open(path).read()
changed = re.sub(r'<revision>[^<]*</revision>', '<revision>1</revision>', text, count=1)
if changed == text:
    changed = text.replace('?>', '?>\n<!-- tampered -->', 1)
assert changed != text
open(path, 'w').write(changed)
PY
  mkdir -p /etc/fb25-tampered.repos.d /etc/fb25-missing.repos.d
  sed -e 's#^\[acceptance\]#[tampered]#' -e 's#^baseurl=.*#baseurl=file:///tmp/tampered-repo/#' \
    /etc/fb25-acceptance.repos.d/acceptance.repo >/etc/fb25-tampered.repos.d/tampered.repo
  sed -e 's#^\[acceptance\]#[missing]#' -e 's#^baseurl=.*#baseurl=file:///nonexistent-repository/#' \
    /etc/fb25-acceptance.repos.d/acceptance.repo >/etc/fb25-missing.repos.d/missing.repo
  refuse tampered-metadata-rejected dnf -y --setopt=reposdir=/etc/fb25-tampered.repos.d makecache
  refuse unavailable-repository-rejected dnf -y --setopt=reposdir=/etc/fb25-missing.repos.d makecache
}

fresh() {
  "${dnf[@]}" install python3
  repository
  check install "${dnf[@]}" "${acceptance[@]}" install "$PACKAGE"
  installed install
  check install-enrolls-source cmp -s /etc/yum.repos.d/halcyon.repo "/usr/share/$PACKAGE/updates-rpm/source.repo"
  check install-enrolls-key cmp -s /etc/pki/rpm-gpg/RPM-GPG-KEY-halcyon "/usr/share/$PACKAGE/updates-rpm/archive-key.asc"
  check install-receipt-complete test "$(phase)" = complete

  snapshot >/tmp/before-reinstall.json
  # Enrollment just added the live source; offer only the build under test.
  check reinstall "${dnf[@]}" "${acceptance[@]}" --setopt=halcyon.enabled=0 reinstall "$PACKAGE"
  installed reinstall
  snapshot >/tmp/after-reinstall.json
  check reinstall-keeps-enrollment cmp -s /tmp/before-reinstall.json /tmp/after-reinstall.json

  check remove "${dnf[@]}" --setopt=clean_requirements_on_remove=False remove "$PACKAGE"
  check remove-disables-source test ! -e /etc/yum.repos.d/halcyon.repo
  check remove-keeps-recovery-copy compgen -G '/etc/yum.repos.d/halcyon.repo.removed.*'
  check remove-keeps-public-key test -f /etc/pki/rpm-gpg/RPM-GPG-KEY-halcyon
  check remove-records-decision test "$(phase)" = disabled
  check install-after-remove "${dnf[@]}" "${acceptance[@]}" install "$PACKAGE"
  check install-after-remove-keeps-source-off test ! -e /etc/yum.repos.d/halcyon.repo
  check install-after-remove-keeps-decision test "$(phase)" = disabled

  negative
}

upgrade() {
  local mode=$1
  "${dnf[@]}" install python3
  rpm --import "$official_key"
  check previous-signature-verifies signatures_ok rpmkeys --checksig "$work/$PREVIOUS_RPM"
  check install-previous "${dnf[@]}" install "$work/$PREVIOUS_RPM"
  check previous-receipt-complete test "$(phase)" = complete

  # User files the package must never touch.
  mkdir -p /root/.config/halcyon /root/.local/share/halcyon
  printf '{"fixture": "preferences"}\n' >/root/.config/halcyon/fixture.json
  printf 'fixture note\n' >/root/.local/share/halcyon/fixture-note.txt
  sha256sum /root/.config/halcyon/fixture.json /root/.local/share/halcyon/fixture-note.txt >/tmp/user-files.sha256

  if [ "$mode" = disabled ]; then
    sed -i 's/^enabled=1$/enabled=0/' /etc/yum.repos.d/halcyon.repo
    check administrator-disabled-source grep -qx 'enabled=0' /etc/yum.repos.d/halcyon.repo
  fi
  cp /etc/yum.repos.d/halcyon.repo /tmp/source-before
  snapshot >/tmp/before-upgrade.json

  repository
  # The enrolled source points at the live repository; leave it out so only
  # the build under test is offered.
  check upgrade "${dnf[@]}" "${acceptance[@]}" --setopt=halcyon.enabled=0 upgrade "$PACKAGE"
  installed upgrade
  snapshot >/tmp/after-upgrade.json
  check upgrade-keeps-enrollment cmp -s /tmp/before-upgrade.json /tmp/after-upgrade.json
  check upgrade-keeps-source-bytes cmp -s /tmp/source-before /etc/yum.repos.d/halcyon.repo
  check upgrade-keeps-user-files sha256sum -c --quiet /tmp/user-files.sha256
}

case $stage in
  build) build ;;
  fresh) fresh ;;
  upgrade) upgrade untouched ;;
  upgrade-disabled) upgrade disabled ;;
  *) echo "Unknown stage: $stage" >&2; exit 2 ;;
esac
exit "$failed"
