#!/usr/bin/env python3
"""Empty upgrade bridge for installations made before Islands was bundled."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from version_policy import require_main_version


def build(version, out):
    require_main_version(Path(__file__).resolve().parents[2], version)
    out.mkdir(parents=True, exist_ok=True)
    destination = out.resolve() / f'cosmic-ext-halcyon-islands_{version}_amd64.deb'
    with tempfile.TemporaryDirectory(prefix='halcyon-transition-') as directory:
        root = Path(directory)
        control = root / 'DEBIAN'
        control.mkdir()
        (control / 'control').write_text(
            f'Package: cosmic-ext-halcyon-islands\nVersion: {version}\nArchitecture: amd64\n'
            'Maintainer: profilaz3 <261673162+profilaz3@users.noreply.github.com>\n'
            'Section: oldlibs\nPriority: optional\n'
            'Depends: cosmic-ext-halcyon (>= 0.5.0)\n'
            'X-Halcyon-Islands: transitional\n'
            'Description: upgrade bridge for bundled Halcyon Islands\n'
            ' Islands is included in Halcyon. This empty package can be removed\n'
            ' after upgrading; no settings or user files are changed.\n')
        subprocess.run(['dpkg-deb', '--root-owner-group', '--build', str(root), str(destination)], check=True)
    destination.with_suffix('.deb.sha256').write_text(f'{hashlib.sha256(destination.read_bytes()).hexdigest()}  {destination.name}\n')
    return destination


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    print(build(args.version, args.out))
