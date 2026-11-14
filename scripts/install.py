#!/usr/bin/env python3
"""Install or upgrade Halcyon for the current user, from a source build.

Installs the same layout as the Debian package (packaging/README.md) under
`~/.local`: the program, its controller and helpers, the built-in wallpapers,
the launcher, icons and the user service. Preferences, notes, looks, accounts
and undo records are never touched.

An install from before the rename (Desktop Cards) is taken over: its service
is switched off in favour of `halcyon.service` (kept enabled or running as it
was), its launcher is removed, and `desktop-cards` keeps working as a shortcut
to `halcyon-ctl`. Replaced programs are kept in a dated backup folder.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

SOURCE = Path(__file__).resolve().parent.parent
PACKAGE = 'cosmic-ext-halcyon'
APP_ID = 'io.github.profilaz3.Halcyon'
SERVICE = 'halcyon.service'
ICON_SIZES = (48, 64, 128, 256, 512)
LEGACY_SERVICE = 'cosmic-desktop-cards.service'
LEGACY_LAUNCHER = 'io.github.workstation.DesktopCards.desktop'
# Each backup holds the whole program (about 100 MB); older ones go.
KEEP_BACKUPS = 2


def default_binary():
    target = Path(os.environ.get('CARGO_TARGET_DIR', SOURCE / 'target'))
    if not target.is_absolute():
        target = SOURCE / target
    return target / 'release' / 'halcyon'


def place(path, data, mode):
    """Writes `data` to `path` atomically; returns whether it changed."""
    path = Path(path)
    try:
        if path.read_bytes() == data and (path.stat().st_mode & 0o777) == mode:
            return False
    except FileNotFoundError:
        pass
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f'.{path.name}.', delete=False) as file:
        file.write(data)
        file.flush()
        os.fsync(file.fileno())
        temporary = Path(file.name)
    try:
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
    return True


def unit(prefix):
    template = (SOURCE / 'packaging/systemd/halcyon.service').read_text()
    return template.replace('/usr/bin/halcyon', str(prefix / 'bin/halcyon'))


def launcher(prefix, suffix=""):
    text = (SOURCE / 'packaging' / f'{APP_ID}{suffix}.desktop').read_text()
    lines = []
    for line in text.splitlines():
        if line.startswith('Exec=halcyon'):
            line = 'Exec=' + str(prefix / 'bin') + '/' + line[len('Exec='):]
        lines.append(line)
    return '\n'.join(lines) + '\n'


def files(binary, prefix, google_client):
    """(destination, bytes, mode) for every installed file."""
    share = prefix / 'share'
    out = [
        (prefix / 'bin/halcyon', binary.read_bytes(), 0o755),
        (prefix / 'bin/halcyon-ctl', (SOURCE / 'scripts/desktop-cards').read_bytes(), 0o755),
        (share / PACKAGE / 'google-calendar-setup.html',
         (SOURCE / 'docs/google-calendar-setup.html').read_bytes(), 0o644),
        (share / 'applications' / f'{APP_ID}.desktop', launcher(prefix).encode(), 0o644),
        (share / 'applications' / f'{APP_ID}.Activity.desktop', launcher(prefix, '.Activity').encode(), 0o644),
        (share / 'metainfo' / f'{APP_ID}.metainfo.xml',
         (SOURCE / 'packaging' / f'{APP_ID}.metainfo.xml').read_bytes(), 0o644),
    ]
    for name in ('calendar-sync.py', 'project-status.py'):
        out.append((prefix / 'lib' / PACKAGE / name, (SOURCE / 'scripts' / name).read_bytes(), 0o755))
    for wallpaper in sorted((SOURCE / 'assets/wallpapers').glob('*.jpg')):
        out.append((share / 'backgrounds/halcyon' / wallpaper.name, wallpaper.read_bytes(), 0o644))
    # Ridge's prepared PNG must match the embedded depth reference, just as in
    # the Debian package. Do not convert or omit it in a source installation.
    ridge = SOURCE / 'assets/wallpapers/halcyon-ridge.png'
    out.append((share / 'backgrounds/halcyon' / ridge.name, ridge.read_bytes(), 0o644))
    for size in ICON_SIZES:
        icon = SOURCE / 'packaging/icons' / str(size) / f'{APP_ID}.png'
        out.append((share / f'icons/hicolor/{size}x{size}/apps' / f'{APP_ID}.png', icon.read_bytes(), 0o644))
    if google_client is not None:
        out.append((share / PACKAGE / 'google-calendar-client.json', google_client.read_bytes(), 0o644))
    return out


def prune_backups(folder, keep, count=KEEP_BACKUPS):
    """Removes all but the newest `count` install backups in `folder`,
    never `keep` (the one just made). Their names start with the UTC time,
    so they sort oldest first. Other folders there are left alone."""
    backups = sorted(path for path in folder.iterdir()
                     if path.name.endswith('-install') and path.is_dir() and not path.is_symlink())
    for old in backups[:-count] if count else backups:
        if old != keep:
            shutil.rmtree(old)


def system_run(args, check=False):
    return subprocess.run(args, check=check, capture_output=True, text=True)


def install(binary, home, *, systemd=True, google_client=None, run=system_run):
    binary = Path(binary).resolve(strict=True)
    home = Path(home)
    prefix = home / '.local'
    planned = files(binary, prefix, google_client)
    # Replaced programs are kept, so an upgrade can be rolled back by hand.
    replaced = [path for path, data, _ in planned[:2] if path.exists() and path.read_bytes() != data]
    backup = None
    if replaced:
        stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
        backup = home / '.local/share/halcyon/backups' / f'{stamp}-install'
        backup.mkdir(parents=True, exist_ok=True)
        for path in replaced:
            shutil.copy2(path, backup / path.name)
        prune_backups(backup.parent, keep=backup)
    first = not (prefix / 'bin/halcyon').exists()
    for path, data, mode in planned:
        place(path, data, mode)
    unit_changed = place(home / '.config/systemd/user' / SERVICE, unit(prefix).encode(), 0o644)

    # Take over an install from before the rename.
    legacy_unit = home / '.config/systemd/user' / LEGACY_SERVICE
    was_enabled = was_running = False
    if legacy_unit.exists():
        if systemd:
            was_enabled = run(['systemctl', '--user', 'is-enabled', '--quiet', LEGACY_SERVICE]).returncode == 0
            was_running = run(['systemctl', '--user', 'is-active', '--quiet', LEGACY_SERVICE]).returncode == 0
            run(['systemctl', '--user', 'disable', '--now', LEGACY_SERVICE])
        legacy_unit.unlink()
        unit_changed = True
    (home / '.local/share/applications' / LEGACY_LAUNCHER).unlink(missing_ok=True)
    shim = prefix / 'bin/desktop-cards'
    if shim.exists() and not shim.is_symlink():
        shim.unlink()
    if not shim.exists():
        shim.symlink_to('halcyon-ctl')

    if systemd:
        if unit_changed:
            run(['systemctl', '--user', 'daemon-reload'], check=True)
        if was_enabled:
            run(['systemctl', '--user', 'enable', SERVICE], check=True)
        if was_running:
            run(['systemctl', '--user', 'restart', SERVICE], check=True)
        elif run(['systemctl', '--user', 'is-active', '--quiet', SERVICE]).returncode == 0:
            run(['systemctl', '--user', 'try-restart', SERVICE], check=True)
        run(['update-desktop-database', str(prefix / 'share/applications')])
    return {'first': first, 'backup': backup, 'took_over': was_enabled or was_running}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--binary', type=Path, default=default_binary(),
                        help='release build to install (default: %(default)s)')
    parser.add_argument('--google-oauth-client', type=Path,
                        help='publisher Google Desktop app JSON (optional)')
    parser.add_argument('--no-systemd', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--home', type=Path, default=Path.home(), help=argparse.SUPPRESS)
    args = parser.parse_args()
    result = install(args.binary, args.home, systemd=not args.no_systemd,
                     google_client=args.google_oauth_client)
    print(f'Installed Halcyon in {args.home / ".local"}')
    if result['backup']:
        print(f'Previous version kept in {result["backup"]}')
    if result['took_over']:
        print('Desktop Cards is now Halcyon; your settings, notes and looks came along.')
    elif result['first']:
        print('Open Halcyon from the app launcher to set it up.')


if __name__ == '__main__':
    try:
        main()
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
