"""Verify an immutable, reviewed RPM repository before adding it to Pages.

APT and DNF share one atomic Pages deployment. A daily APT renewal must retain
the exact signed RPM repository pinned by the same distribution approval.
"""
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

PREFIX = 'fedora/43/x86_64/'  # Immutable version-1 review contract.
TARGETS = ('43', '44')
MAX_BYTES = 256 * 1024 * 1024
CHECKS = ('package', 'installation', 'upgrade', 'source', 'panel', 'settings', 'signatures')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def safe_name(name):
    path = Path(name)
    if (not name or path.is_absolute() or '..' in path.parts
            or path.as_posix() != name or '\\' in name):
        raise ValueError('unsafe_rpm_repository_path')
    return path


def unpack(archive, destination):
    """No links, duplicates, special files or unbounded decompression."""
    seen, total = set(), 0
    with tarfile.open(archive, 'r:gz') as source:
        for item in source:
            name = item.name.rstrip('/') if item.isdir() else item.name
            path = safe_name(name)
            if name in seen or len(seen) >= 1000 or not (item.isdir() or item.isfile()):
                raise ValueError('unexpected_rpm_repository_member')
            seen.add(name)
            total += item.size
            if item.size < 0 or total > MAX_BYTES:
                raise ValueError('rpm_repository_size_limit')
            target = destination/path
            if item.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(item) as incoming, target.open('xb') as outgoing:
                    shutil.copyfileobj(incoming, outgoing)
    return total


def run(command):
    return subprocess.run(command, check=True, capture_output=True, text=True, timeout=60).stdout


def signature(public, signed, fingerprint):
    with tempfile.TemporaryDirectory(prefix='halcyon-rpm-signature-') as home:
        status = run(['gpgv', '--homedir', home, '--status-fd', '1', '--keyring',
                      str(public.resolve()), str(signed)+'.asc', str(signed)])
    signers = [line.split()[2] for line in status.splitlines()
               if line.startswith('[GNUPG:] VALIDSIG ')]
    if signers != [fingerprint]:
        raise ValueError('rpm_repository_signer_mismatch')


def verify(root, release, pin, public):
    manifest = root/'release.json'
    if digest(manifest) != pin['manifest_sha256']:
        raise ValueError('rpm_release_review_changed')
    signature(public, manifest, pin['signer'])
    record = json.loads(manifest.read_bytes())
    version = release['release']
    if (record.get('format') != 'halcyon-rpm-release' or record.get('release') != version
            or not record.get('review_reference')):
        raise ValueError('rpm_release_review_incomplete')
    if record.get('version') == 1:
        targets = {'43': {k: record.get(k) for k in ('target', 'package', 'observations')}}
    elif record.get('version') == 2 and set(record.get('targets', {})) == set(TARGETS):
        targets = record['targets']
    else:
        raise ValueError('rpm_release_review_incomplete')
    packages = []
    prefixes = []
    for fedora, target in targets.items():
        prefix = f'fedora/{fedora}/x86_64/'
        package = prefix+f'cosmic-ext-halcyon-{version}-1.fc{fedora}.x86_64.rpm'
        if target != dict(target=f'fedora{fedora}-x86_64', package=package,
                          observations=dict.fromkeys(CHECKS, 'passed')):
            raise ValueError('rpm_release_review_incomplete')
        prefixes.append(prefix)
        packages.append(package)
    for key, kind in (('source', 'corresponding-source'), ('evidence', 'native-evidence')):
        bound = record[key]
        expected = release['assets'].get(bound['file'])
        if expected != {k: bound[k] for k in ('kind', 'sha256', 'bytes')} or bound['kind'] != kind:
            raise ValueError('rpm_source_or_evidence_binding_mismatch')
    files = record['files']
    for prefix, package in zip(prefixes, packages):
        if not {package, prefix+'repodata/repomd.xml', prefix+'repodata/repomd.xml.asc',
                prefix+'RPM-GPG-KEY-halcyon'} <= files.keys():
            raise ValueError('rpm_repository_incomplete')
    actual = {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()}
    if actual != set(files) | {'release.json', 'release.json.asc'}:
        raise ValueError('rpm_repository_inventory_mismatch')
    for name, expected in files.items():
        safe_name(name)
        if not name.startswith(tuple(prefixes)) or digest(root/name) != expected['sha256'] or (root/name).stat().st_size != expected['bytes']:
            raise ValueError('rpm_repository_file_changed')
    if {name for name in files if name.endswith('.rpm')} != set(packages):
        raise ValueError('unexpected_rpm_package')
    for prefix, package in zip(prefixes, packages):
        asset = release['assets'].get(Path(package).name)
        if asset != {'kind': 'main-rpm', **files[package]}:
            raise ValueError('rpm_download_and_update_bytes_differ')
        signature(public, root/(prefix+'repodata/repomd.xml'), pin['signer'])
    # Trust only the reviewed archive key, never a key supplied by the archive.
    with tempfile.TemporaryDirectory(prefix='halcyon-rpm-package-trust-') as temporary:
        work = Path(temporary)
        key = work/'public.asc'
        key.write_text(run(['gpg', '--homedir', str(work), '--batch', '--no-default-keyring', '--keyring',
                            str(public.resolve()), '--armor', '--export']))
        db = work/'rpmdb'
        run(['rpmkeys', '--dbpath', str(db), '--import', str(key)])
        for package in packages:
            result = run(['rpmkeys', '--dbpath', str(db), '--checksig', str(root/package)])
            if 'signatures OK' not in result:
                raise ValueError('rpm_package_signature_failed')
    return record


def stage(data, public, out, download, asset_url):
    pin = data.get('rpm_repository')
    if pin is None:
        return 0  # Historical DEB-only distributions retain their contract.
    if (not re.fullmatch('[a-f0-9]{64}', pin.get('manifest_sha256', ''))
            or not re.fullmatch('[A-F0-9]{40}', pin.get('signer', ''))):
        raise ValueError('rpm_distribution_pin_invalid')
    releases = [r for r in data['releases'] if r['tag'] == pin['tag']]
    if len(releases) != 1:
        raise ValueError('rpm_release_not_in_approved_distribution')
    release = releases[0]
    record = release['assets'][pin['file']]
    if record['kind'] != 'rpm-repository' or not 0 < record['bytes'] <= MAX_BYTES:
        raise ValueError('rpm_repository_asset_invalid')
    with tempfile.TemporaryDirectory(prefix='halcyon-rpm-distribution-') as temporary:
        work = Path(temporary)
        archive = work/'repository.tar.gz'
        download(asset_url(pin['tag'], pin['file']), archive, record['bytes'], record['sha256'], record['bytes'])
        root = work/'verified'; root.mkdir()
        unpack(archive, root)
        verify(root, release, pin, public)
        shutil.copytree(root, out/'rpm')
    return sum(p.stat().st_size for p in (out/'rpm').rglob('*') if p.is_file())
