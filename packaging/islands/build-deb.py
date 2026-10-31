#!/usr/bin/env python3
"""Package an already built, reviewed Islands binary without activating it."""
import argparse
import datetime
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
COMPANION = HERE.parent.parent / 'companion'
sys.path.insert(0, str(COMPANION))
sys.path.insert(0, str(HERE.parent))
from version_policy import companion_depends
import panel_launcher
from bundle_contract import (HELPERS, canonical, identity, verify_bundle,
                             valid_catalog, read_catalog_bytes, digest, release_catalog)

PACKAGE = 'cosmic-ext-halcyon-islands'
MAINTAINER = 'profilaz3 <261673162+profilaz3@users.noreply.github.com>'


def stage_file(source, destination, mode=0o644):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(mode)


def stage_payload(staging, binary, manifest, version, licenses=None, catalog=None, *, owner=PACKAGE, epoch=None, strip_binary=True):
    """The same verified payload serves legacy packages and the bundled app."""
    from bundle_contract import PACKAGE_OWNERS
    if owner not in PACKAGE_OWNERS:
        raise ValueError('bundle_package_owner')
    epoch = int(os.environ.get('SOURCE_DATE_EPOCH', '1792627200')) if epoch is None else epoch
    date = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc)
    library = staging / 'usr/lib' / PACKAGE
    stage_file(binary, library / 'cosmic-panel-islands', 0o755)
    stage_file(manifest, library / 'companion-manifest.json')
    initial = json.loads((library / 'companion-manifest.json').read_bytes())
    if initial.get('protocol') != 1 or not re.fullmatch('[0-9a-f]{40}', initial.get('upstream', '')) or initial.get('binary_sha256') != hashlib.sha256(binary.read_bytes()).hexdigest():
        raise ValueError('Binary or manifest fails the selector checksum, upstream or protocol checks')
    # Shipped without debug symbols. Only symbols go, so the reviewed
    # build's behavior is unchanged; the manifest records the new hash.
    staged = library / 'cosmic-panel-islands'
    if staged.read_bytes()[:4] == b'\x7fELF':
        if licenses is None or not licenses.is_file() or not licenses.stat().st_size:
            raise ValueError('Native releases require --licenses with dependency notices')
        if strip_binary:
            subprocess.run(['strip', '--strip-unneeded', str(staged)], check=True)
        record = json.loads((library / 'companion-manifest.json').read_text())
        record['binary_sha256'] = hashlib.sha256(staged.read_bytes()).hexdigest()
        (library / 'companion-manifest.json').write_text(json.dumps(record, indent=2) + '\n')
    record = json.loads((library / 'companion-manifest.json').read_text())
    for name in HELPERS:
        stage_file(COMPANION / name, library / name, 0o755 if name in ('install.py', 'panel_launcher.py', 'selector.py') else 0o644)
    catalog_data = read_catalog_bytes(catalog or COMPANION / 'compatibility/catalog.json')
    admitted = release_catalog(json.loads(catalog_data))
    catalog_data = canonical(admitted)
    (library / 'catalog.json').write_bytes(catalog_data)
    (library / 'catalog.json').chmod(0o644)
    del catalog_data
    files = {p.name: {'sha256': digest(p), 'mode': p.stat().st_mode & 0o777}
             for p in library.iterdir() if p.name != 'companion-manifest.json'}
    payload = identity({k: v for k, v in files.items() if k != 'catalog.json'})
    if not admitted.get('startup_policy') and any(r['payload_sha256'] != payload for r in admitted['cohorts']):
        raise ValueError('Catalog belongs to a different finalized payload; retain a separate immutable package for that cohort')
    record.update(format='halcyon-islands-bundle', version=2, package_version=version, package_owner=owner,
                  files=files, payload_sha256=payload)
    for key in ('packages', 'supported_packages', 'source_policy'):
        record.pop(key, None)
    (library / 'companion-manifest.json').write_bytes(canonical(record) + b'\n')
    verify_bundle(library, 'companion-manifest.json')
    stage_file(HERE / 'halcyon-islands-bootstrap.desktop', staging / 'etc/xdg/autostart/halcyon-islands-bootstrap.desktop')
    stage_file(HERE / 'halcyon-islands', staging / 'usr/bin/halcyon-islands', 0o755)
    docs = staging / 'usr/share/doc' / PACKAGE
    stage_file(HERE / 'copyright', docs / 'copyright')
    if licenses is not None:
        stage_file(licenses, docs / 'THIRD_PARTY_LICENSES.txt')
    stage_file(HERE / 'README.md', docs / 'README')
    changelog = (f'{PACKAGE} ({version}) unstable; urgency=medium\n\n'
                 '  * Package the optional, source-qualified Islands panel.\n\n'
                 f' -- {MAINTAINER}  {date.strftime("%a, %d %b %Y %H:%M:%S %z")}\n')
    (docs / 'changelog.gz').write_bytes(gzip.compress(changelog.encode(), mtime=0))
    man = staging / 'usr/share/man/man1/halcyon-islands.1.gz'
    man.parent.mkdir(parents=True, exist_ok=True)
    man.write_bytes(gzip.compress((HERE / 'halcyon-islands.1').read_bytes(), mtime=0))
    return library


def build(binary, manifest, version, out, licenses=None, catalog=None):
    if not re.fullmatch(r'[0-9][A-Za-z0-9.+:~\-]*', version):
        raise ValueError('Invalid Debian version')
    subprocess.run(['dpkg', '--validate-version', version], check=True)
    epoch = int(os.environ.get('SOURCE_DATE_EPOCH', str(int(datetime.datetime.now().timestamp()))))
    date = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc)
    out.mkdir(parents=True, exist_ok=True)
    filename = f'{PACKAGE}_{version.split(":", 1)[-1]}_amd64.deb'
    with tempfile.TemporaryDirectory(prefix='islands-build-', dir=out) as temporary:
        staging = Path(temporary) / 'package'
        library = stage_payload(staging, binary, manifest, version, licenses, catalog, epoch=epoch)
        staged = library / 'cosmic-panel-islands'
        debian = Path(temporary) / 'debian'
        debian.mkdir()
        (debian / 'control').write_text(
            f'Source: {PACKAGE}\nSection: utils\nPriority: optional\n'
            f'Maintainer: {MAINTAINER}\n\nPackage: {PACKAGE}\n'
            'Architecture: amd64\nDescription: Optional Islands panel\n')
        shlibs = subprocess.run(
            ['dpkg-shlibdeps', '-O', f'-e{library / "cosmic-panel-islands"}'],
            cwd=Path(temporary), capture_output=True, text=True)
        native = next((line.removeprefix('shlibs:Depends=').strip()
                       for line in shlibs.stdout.splitlines()
                       if line.startswith('shlibs:Depends=')), '')
        if staged.read_bytes()[:4] == b'\x7fELF' and (shlibs.returncode != 0 or not native):
            raise ValueError('Cannot determine native library dependencies: ' + shlibs.stderr.strip())
        dependencies = ([native] if shlibs.returncode == 0 and native else ['libc6']) + [
            'python3', companion_depends(HERE.parent.parent)] + [
            name for name in panel_launcher.PACKAGES]
        size = sum(path.stat().st_size for path in staging.rglob('*') if path.is_file())
        control = staging / 'DEBIAN/control'
        control.parent.mkdir()
        (control.parent / 'conffiles').write_text('/etc/xdg/autostart/halcyon-islands-bootstrap.desktop\n')
        control.write_text(
            f'Package: {PACKAGE}\nVersion: {version}\nArchitecture: amd64\n'
            f'Maintainer: {MAINTAINER}\nSection: utils\nPriority: optional\n'
            f'Installed-Size: {(size + 1023) // 1024}\n'
            f'Depends: {", ".join(dependencies)}\n'
            'Homepage: https://github.com/profilaz3/halcyon\n'
            'Description: optional source-qualified floating panel islands for Halcyon\n'
            ' A patched COSMIC panel that splits the panel into floating islands.\n'
            ' The launcher uses the standard panel if source or runtime checks fail.\n'
            ' Installing the add-on never holds back COSMIC system updates.\n'
            ' Activation is per user with halcyon-islands enable, at the next login.\n'
            ' Installing this package does not activate or restart a panel.\n')
        staging.chmod(0o755)
        for path in staging.rglob('*'):
            if path.is_dir():
                path.chmod(0o755)
            elif path not in (library / 'cosmic-panel-islands', library / 'install.py',
                              library / 'panel_launcher.py', library / 'selector.py', staging / 'usr/bin/halcyon-islands'):
                path.chmod(0o644)
            os.utime(path, (epoch, epoch))
        os.utime(staging, (epoch, epoch))
        artifact = Path(temporary) / filename
        subprocess.run(['dpkg-deb', '--root-owner-group', '--build', str(staging), str(artifact)],
                       env={**os.environ, 'SOURCE_DATE_EPOCH': str(epoch)}, check=True)
        checksum = hashlib.sha256(artifact.read_bytes()).hexdigest()
        destination = out / filename
        if destination.exists() and hashlib.sha256(destination.read_bytes()).hexdigest() != checksum:
            raise ValueError('Refusing to replace immutable package version with different bytes')
        artifact.replace(destination)
        destination.with_suffix('.deb.sha256').write_text(f'{checksum}  {filename}\n')
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True, help='prebuilt cosmic-panel-islands')
    parser.add_argument('--manifest', type=Path, required=True, help='reviewed companion-manifest.json')
    parser.add_argument('--version', required=True)
    parser.add_argument('--catalog', type=Path, help='reviewed local cohort catalog; default has no new admissions')
    parser.add_argument('--licenses', type=Path, help='dependency license report (required for native binaries)')
    parser.add_argument('--out', type=Path, required=True, help='output directory')
    args = parser.parse_args()
    try:
        print(build(args.binary, args.manifest, args.version, args.out, args.licenses, args.catalog))
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f'Cannot build Islands package: {error}\n')


if __name__ == '__main__':
    main()
