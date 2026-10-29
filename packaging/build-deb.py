#!/usr/bin/env python3
"""Build the /usr Halcyon package without installing or modifying source files."""

import argparse
import gzip
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent))
from version_policy import require_main_version, main_breaks, bundled
PACKAGE = "cosmic-ext-halcyon"
APP_ID = "io.github.profilaz3.Halcyon"
MAINTAINER = "profilaz3 <261673162+profilaz3@users.noreply.github.com>"
HOMEPAGE = "https://github.com/profilaz3/halcyon"
ICON_SIZES = (48, 64, 128, 256, 512)


def update_defaults(root, control):
    """Ship offline enrollment inputs; retain separate bootstrap ownership."""
    updates = REPO / "packaging/updates"
    spec = importlib.util.spec_from_file_location("halcyon_enrollment", updates / "build-deb.py")
    enrollment = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(enrollment)
    key = (updates / "halcyon-archive-keyring.gpg").read_bytes()
    manifest = json.loads((updates / "archive-key.json").read_text())
    if hashlib.sha256(key).hexdigest() != manifest["sha256"]:
        raise ValueError("archive public key does not match reviewed manifest")
    source, prefs = enrollment.configuration()
    target = root / "usr/share/cosmic-ext-halcyon/updates"
    target.mkdir(parents=True)
    for name, data in (("source.sources", source.encode()), ("preferences", prefs.encode()),
                       ("archive-keyring.gpg", key)):
        (target / name).write_bytes(data)
    helper = (updates / "defaults.py").read_text()
    for name in ("postinst", "postrm"):
        # postrm still works after /usr payloads have been removed. If Python
        # was removed too, retain public trust/config files without breaking APT.
        optional = 'command -v python3 >/dev/null 2>&1 || exit 0\n' if name == "postrm" else ""
        (control / name).write_text('#!/bin/sh\nset -eu\n' + optional +
            '/usr/bin/python3 -I - "$@" <<\'HALCYON_UPDATES_PY\'\n' + helper +
            '\nHALCYON_UPDATES_PY\n')
        (control / name).chmod(0o755)


def install(source, destination, mode=0o644):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(mode)


def dependencies(binary, scratch, islands=None):
    """Give dpkg-shlibdeps its own Debian source context, never the checkout."""
    debian = scratch / "debian"
    debian.mkdir()
    (debian / "control").write_text(
        f"Source: {PACKAGE}\nSection: x11\nPriority: optional\n"
        f"Maintainer: {MAINTAINER}\n\nPackage: {PACKAGE}\n"
        "Architecture: amd64\nDescription: Desktop widgets and themes for COSMIC\n"
    )
    detected = ""
    if shutil.which("dpkg-shlibdeps"):
        result = subprocess.run(
            ["dpkg-shlibdeps", "-O", "-e" + str(binary)] + (["-e" + str(islands)] if islands else []),
            cwd=scratch, capture_output=True, text=True,
        )
        if result.returncode == 0:
            for line in result.stdout.splitlines():
                if line.startswith("shlibs:Depends="):
                    detected = line.partition("=")[2].strip()
        if not detected and result.stderr:
            print(result.stderr.strip(), file=sys.stderr)
    if not detected and islands:
        raise ValueError("Cannot determine dependencies of the bundled native executables")
    if not detected:
        print("Warning: no shared-library dependencies detected; using "
              "fallback libc6, libxkbcommon0.", file=sys.stderr)
        detected = "libc6, libxkbcommon0"
    if islands:
        import panel_launcher
        detected += ", " + ", ".join(panel_launcher.PACKAGES)
    return detected + ", python3, git, systemd"


def default_wallpapers():
    for path in (REPO / "assets/wallpapers-jpg", REPO / "assets/wallpapers"):
        if any(path.glob("*.jpg")):
            return path
    return REPO / "assets/wallpapers-jpg"


def build(args):
    release_updates = getattr(args, "release_updates", False)
    development = getattr(args, "development", False)
    if release_updates == development:
        raise ValueError("choose --release-updates for release packaging or --development for an isolated package identity; use scripts/install.py for per-user source installs")
    package = PACKAGE if release_updates else PACKAGE + "-dev"
    if not re.fullmatch(r"[0-9][A-Za-z0-9.+~\-]*", args.version):
        raise ValueError("version must be a Debian version without an epoch or path separators")
    if release_updates:
        require_main_version(REPO, args.version)
        breaks = main_breaks(REPO)
    bundled_islands = release_updates and bundled(REPO)
    if bundled_islands and any(getattr(args, key, None) is None for key in
                               ('islands_binary', 'islands_manifest', 'islands_licenses')):
        raise ValueError("Bundled releases require --islands-binary, --islands-manifest and --islands-licenses")
    binary = args.binary.resolve(strict=True)
    if not binary.is_file():
        raise ValueError("binary must be a regular file")
    wallpaper_dir = args.wallpapers if args.wallpapers is not None else default_wallpapers()
    wallpapers = sorted(path for path in wallpaper_dir.glob("*.jpg") if path.is_file())
    if not wallpapers:
        raise ValueError(f"no <name>.jpg wallpapers in {wallpaper_dir}; use --wallpapers")
    if args.google_client:
        args.google_client.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=True)
    output = args.out.resolve() / f"{package}_{args.version}_amd64.deb"

    with tempfile.TemporaryDirectory(prefix="halcyon-deb-") as temp:
        scratch = Path(temp)
        root = scratch / "package"
        usr = root / "usr"
        install(binary, usr / "bin/halcyon", 0o755)
        install(REPO / "scripts/desktop-cards", usr / "bin/halcyon-ctl", 0o755)
        install(REPO / "companion/panel_launcher.py", usr / "lib" / PACKAGE / "panel_launcher.py")
        install(REPO / "companion/install_status.py", usr / "lib" / PACKAGE / "islands-status.py", 0o755)
        for helper in ("bundle_contract.py", "journal.py", "runtime_identity.py", "panel_configuration.py", "runtime_target.py", "verification.py", "runtime_compatibility.py"):
            install(REPO / "companion" / helper, usr / "lib" / PACKAGE / helper)
        for name in ("calendar-sync.py", "project-status.py"):
            install(REPO / "scripts" / name, usr / "lib" / PACKAGE / name, 0o755)
        install(REPO / "docs/google-calendar-setup.html",
                usr / "share" / PACKAGE / "google-calendar-setup.html")
        if args.google_client:
            install(args.google_client, usr / "share" / PACKAGE / "google-calendar-client.json")
        install(REPO / "assets/wallpapers/halcyon-ridge.png", usr / "share/backgrounds/halcyon/halcyon-ridge.png")
        for wallpaper in wallpapers:
            install(wallpaper, usr / "share/backgrounds/halcyon" / wallpaper.name)
        install(REPO / "packaging/systemd/halcyon.service",
                usr / "lib/systemd/user/halcyon.service")
        for suffix, directory in (("desktop", "applications"), ("metainfo.xml", "metainfo")):
            name = f"{APP_ID}.{suffix}"
            install(REPO / "packaging" / name, usr / "share" / directory / name)
        for size in ICON_SIZES:
            name = f"{APP_ID}.png"
            install(REPO / "packaging/icons" / str(size) / name,
                    usr / f"share/icons/hicolor/{size}x{size}/apps" / name)
        docs = usr / "share/doc" / package
        for name in ("LICENSE", "THIRD_PARTY.md", "ASSETS.md"):
            install(REPO / name, docs / name)
        install(REPO / "packaging/debian/copyright", docs / "copyright")
        bundle = REPO / "packaging/licenses/THIRD_PARTY_LICENSES.txt"
        if bundle.is_file():
            install(bundle, docs / bundle.name)
        else:
            print("Warning: dependency license bundle absent; generate it with "
                  "packaging/collect-licenses.py before distributing.", file=sys.stderr)
        # Equivalent to gzip -n: omit the input filename and timestamp.
        with (docs / "changelog.gz").open("wb") as stream:
            with gzip.GzipFile(filename="", mode="wb", fileobj=stream, mtime=0) as compressed:
                compressed.write((REPO / "packaging/debian/changelog").read_bytes())
        for name in ("halcyon.1", "halcyon-ctl.1"):
            destination = usr / "share/man/man1" / (name + ".gz")
            destination.parent.mkdir(parents=True, exist_ok=True)
            with destination.open("wb") as stream:
                with gzip.GzipFile(filename="", mode="wb", fileobj=stream, mtime=0) as compressed:
                    compressed.write((REPO / "packaging/man" / name).read_bytes())

        islands_binary = None
        if bundled_islands:
            spec = importlib.util.spec_from_file_location("halcyon_islands_package", REPO / "packaging/islands/build-deb.py")
            islands = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(islands)
            library = islands.stage_payload(root, args.islands_binary, args.islands_manifest,
                args.version, args.islands_licenses, getattr(args, "islands_catalog", None), owner=PACKAGE)
            islands_binary = library / "cosmic-panel-islands"
        control = root / "DEBIAN"
        control.mkdir()
        if release_updates:
            update_defaults(root, control)
        if bundled_islands:
            (control / "conffiles").write_text("/etc/xdg/autostart/halcyon-islands-bootstrap.desktop\n")
        depends = dependencies(binary, scratch, islands_binary)
        # Debian Policy: regular files rounded up to KiB, plus 1 KiB per directory.
        installed_size = 1 + sum(
            math.ceil(path.stat().st_size / 1024) if path.is_file() else 1
            for path in root.rglob("*") if control not in path.parents
        )
        (control / "control").write_text(
            f"Package: {package}\nVersion: {args.version}\nArchitecture: amd64\n"
            + (f"Conflicts: {PACKAGE}\nReplaces: {PACKAGE}\n" if development else f"Breaks: {breaks}\n")
            + (f"Replaces: {breaks}\nX-Halcyon-Islands: bundled\n" if bundled_islands else "") +
            "Section: x11\nPriority: optional\n"
            f"Maintainer: {MAINTAINER}\nHomepage: {HOMEPAGE}\n"
            f"Installed-Size: {installed_size}\nDepends: {depends}\n"
            "Recommends: cosmic-session\n"
            "Description: Desktop widgets and themes for COSMIC\n"
            " Halcyon puts widgets on your COSMIC desktop and gives the whole\n"
            " desktop a coordinated look: wallpaper, colors, windows, dock and panel.\n"
            " .\n"
            " Choose six starter looks and thirteen widget styles. Widgets include\n"
            " clock, date, weather, system monitor, calendar, projects, music, notes\n"
            " and a focus timer. Changes can be undone, and your applets, their order\n"
            " and your panel position stay yours.\n"
            + (" Development package; no release update enrollment.\n" if development else
               " Fresh installations enable the signed Halcyon system update source.\n"
               " Existing disabled or administrator-modified sources are preserved.\n")
        )
        (control / "md5sums").write_text("".join(
            f"{hashlib.md5(path.read_bytes()).hexdigest()}  {path.relative_to(root)}\n"
            for path in sorted(usr.rglob("*")) if path.is_file()
        ))
        root.chmod(0o755)
        for path in root.rglob("*"):
            path.chmod(0o755 if path.is_dir() or path.stat().st_mode & 0o111 else 0o644)
        env = os.environ.copy()
        env.setdefault("SOURCE_DATE_EPOCH", "1792627200")  # 2026-10-22 00:00:00 UTC
        subprocess.run(
            ["dpkg-deb", "--build", "--root-owner-group", str(root), str(output)],
            check=True, env=env,
        )
    checksum = output.with_suffix(output.suffix + ".sha256")
    checksum.write_text(f"{hashlib.sha256(output.read_bytes()).hexdigest()}  {output.name}\n")
    output.chmod(0o644)
    checksum.chmod(0o644)
    print(output)
    print(checksum)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--release-updates", action="store_true",
                      help="release identity with signed APT enrollment on clean install")
    mode.add_argument("--development", action="store_true",
                      help="distinct -dev identity, no APT enrollment; conflicts with release")
    parser.add_argument("--version", required=True)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--google-client", type=Path)
    parser.add_argument("--wallpapers", type=Path,
                        help="directory of <name>.jpg files (default: assets/wallpapers-jpg)")
    parser.add_argument("--islands-binary", type=Path)
    parser.add_argument("--islands-manifest", type=Path)
    parser.add_argument("--islands-licenses", type=Path)
    parser.add_argument("--islands-catalog", type=Path)
    args = parser.parse_args()
    try:
        build(args)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"build-deb: {error}\n")


if __name__ == "__main__":
    main()
