#!/usr/bin/env python3
"""Build the pinned companion independently of Desktop Cards' UI runtime."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

REVISION = 'ca20ce3185b3b4404f429d59d429c40b58a2ddef'
ARCHIVE_SHA256 = '27854c7d3fc54cb29bc866b8421a078aa6b396ea73e2d88fe4ec0cf4fdd55704'


def prepare(archive, destination, revision=REVISION, archive_sha256=ARCHIVE_SHA256, *, penumbra=False):
    if hashlib.sha256(archive.read_bytes()).hexdigest() != archive_sha256:
        raise ValueError('Upstream archive hash does not match the reviewed pin')
    if destination.exists():
        raise ValueError('Choose a new empty build directory')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as staging:
        with tarfile.open(archive) as source:
            source.extractall(staging, filter='data')
        extracted = Path(staging) / ('cosmic-panel-' + revision)
        shutil.move(extracted, destination)
    patch = Path(__file__).with_name('patches') / '0001-dock-islands.patch'
    subprocess.run(['patch', '--batch', '--forward', '--fuzz=0', '-p1', '-i', str(patch.resolve())], cwd=destination, check=True)
    if penumbra:
        patch = patch.with_name('0002-penumbra-panel.patch')
        subprocess.run(['patch', '--batch', '--forward', '--fuzz=0', '-p1', '-i', str(patch.resolve())], cwd=destination, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path)
    parser.add_argument('--revision', default=REVISION)
    parser.add_argument('--archive-sha256', default=ARCHIVE_SHA256)
    parser.add_argument('--workdir', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--offline', action='store_true')
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--with-penumbra', action='store_true', help='Include the independent Penumbra gradient mod')
    parser.add_argument('--jobs', type=int, default=6)
    args = parser.parse_args()
    import re
    if not re.fullmatch('[0-9a-f]{40}', args.revision) or not re.fullmatch('[0-9a-f]{64}', args.archive_sha256):
        parser.error('full reviewed source revision and archive SHA256 required')
    archive = args.archive
    if archive is None:
        if args.offline:
            parser.error('--offline needs the verified --archive')
        archive = args.workdir.parent / ('cosmic-panel-' + args.revision + '.tar.gz')
        archive.parent.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve('https://codeload.github.com/pop-os/cosmic-panel/tar.gz/' + args.revision, archive)
    prepare(archive.resolve(), args.workdir.resolve(), args.revision, args.archive_sha256, penumbra=args.with_penumbra)
    if not args.prepare_only:
        env = {**os.environ, 'RUSTUP_TOOLCHAIN': '1.93.0', 'CARGO_TARGET_DIR': str(args.target_dir.resolve())}
        command = ['cargo', 'build', '--release', '--locked', '-j', str(args.jobs)]
        if args.offline:
            command.append('--offline')
        subprocess.run(command, cwd=args.workdir, env=env, check=True)


if __name__ == '__main__':
    main()
