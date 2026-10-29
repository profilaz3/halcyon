#!/usr/bin/env python3
"""Prepare, verify and sign the approved release feed. Never rebuild application packages."""
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
from urllib.parse import quote, urlsplit
from urllib.request import Request, urlopen
from urllib.error import HTTPError

import archive
from build_archive import release_gate

ROOT = Path(__file__).resolve().parents[2]
PUBLIC = ROOT/'packaging/updates/halcyon-archive-keyring.gpg'
INPUTS = ROOT/'scripts/apt/distribution.json'
RELEASE_BASE = 'https://github.com/profilaz3/halcyon/releases/download/'
MAX_SITE = 850_000_000
PACKAGE_KINDS = ('main-deb', 'islands-deb', 'transition-deb', 'enrollment-deb')


def safe_path(name):
    path = Path(name)
    if not name or path.is_absolute() or '..' in path.parts or path.as_posix() != name:
        raise ValueError('unsafe_archive_path')
    return path


def asset_url(tag, name):
    if not re.fullmatch(r'[A-Za-z0-9._-]+', tag) or safe_path(name).name != name:
        raise ValueError('unsafe_release_asset')
    return RELEASE_BASE+tag+'/'+quote(name)


def download(url, target, limit, expected=None, size=None):
    parsed = urlsplit(url)
    if parsed.scheme != 'https' and not (parsed.scheme == 'http' and parsed.hostname == '127.0.0.1'):
        raise ValueError('https_required')
    if parsed.username or parsed.password or parsed.fragment:
        raise ValueError('credential_free_url_required')
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(target.name+'.partial')
    try:
        request = Request(url, headers={'User-Agent': 'Halcyon-APT-maintenance/1', 'Cache-Control': 'no-cache'})
        started = time.monotonic()
        with urlopen(request, timeout=30) as response, temporary.open('wb') as stream:
            if parsed.scheme == 'https' and not response.geturl().startswith('https://'):
                raise ValueError('https_redirect_required')
            total = 0
            while chunk := response.read(1024*1024):
                total += len(chunk)
                if total > limit or time.monotonic()-started > 600:
                    raise ValueError('download_bound_exceeded')
                stream.write(chunk)
        if (size is not None and total != size) or (expected and archive.sha(temporary) != expected):
            raise ValueError('release_or_archive_asset_changed')
        os.replace(temporary, target)
    finally:
        temporary.unlink(missing_ok=True)


def load_inputs(path, public):
    data = json.loads(path.read_bytes())
    if (data.get('format') != 'halcyon-apt-distribution' or data.get('version') != 1
            or data['archive_url'] != 'https://profilaz3.github.io/halcyon/apt'
            or not 0 < data['pages_max_bytes'] <= MAX_SITE
            or data['public_key_sha256'] != archive.sha(public)):
        raise ValueError('distribution_identity_invalid')
    for release in [*data['releases'], data['bootstrap']]:
        for name, record in release['assets'].items():
            asset_url(release['tag'], name)
            if not re.fullmatch('[a-f0-9]{64}', record['sha256']) or not 0 < record['bytes'] <= 512*1024*1024:
                raise ValueError('release_asset_identity_invalid')
    return data


def fetch_assets(release, out, cache=None):
    out.mkdir(parents=True, exist_ok=False)
    for name, record in release['assets'].items():
        cached = cache/name if cache else None
        if cached and cached.is_file():
            if archive.sha(cached) != record['sha256'] or cached.stat().st_size != record['bytes']:
                raise ValueError('local_release_asset_changed')
            archive.copy_file(cached, out/name)
        else:
            download(asset_url(release['tag'], name), out/name, record['bytes'], record['sha256'], record['bytes'])


def descriptor_proof(proof, public):
    """Authenticate retention metadata, even after Release's update window expired.

    This restores unchanged payload/index bytes; it does not make expired APT
    metadata acceptable to clients. The new Release always gets a fresh Date.
    """
    with tempfile.TemporaryDirectory(prefix='halcyon-retention-trust-') as home:
        archive.run(['gpgv', '--homedir', home, '--keyring', str(public.resolve()),
                     str(proof/'archive.json.asc'), str(proof/'archive.json')])
    data = json.loads((proof/'archive.json').read_bytes())
    if data.get('format') != 'halcyon-apt-stage' or data.get('version') != 1:
        raise ValueError('previous_archive_schema')
    if len(data['files']) > 100000:
        raise ValueError('previous_archive_inventory_too_large')
    for name, digest in data['files'].items():
        safe_path(name)
        if not re.fullmatch('[a-f0-9]{64}', digest):
            raise ValueError('previous_archive_hash_invalid')
    return data


def fetch_previous(uri, out, public, limit):
    out.mkdir(parents=True, exist_ok=False)
    download(uri+'/archive.json', out/'archive.json', 16*1024*1024)
    download(uri+'/archive.json.asc', out/'archive.json.asc', 65536)
    data = descriptor_proof(out, public)
    total = 0
    for name, digest in data['files'].items():
        download(uri+'/'+quote(name, safe='/'), out/name, min(512*1024*1024, limit-total), digest)
        total += (out/name).stat().st_size
        if total > limit:
            raise ValueError('previous_site_budget_exceeded')
    archive.verify_tree(out, archive.sha(out/'archive.json'))
    return data


def retained_path(name):
    return name.startswith(('pool/', 'releases/')) or '/by-hash/SHA256/' in name


def verify(root, inputs, approved, public, previous_proof=None):
    data = load_inputs(inputs, public)
    if not approved or archive.sha(inputs) != approved:
        raise ValueError('parent_distribution_approval_pin_required')
    expected = archive.sha(root/'archive.json')
    staged = archive.verify_tree(root, expected)
    if staged['review'] != {'policy': 'published-practical-release', 'distribution_sha256': approved}:
        raise ValueError('staged_distribution_binding_mismatch')
    if not time.time()-7*86400 < staged['epoch'] <= time.time()+300:
        raise ValueError('archive_expired_or_future')
    prefix = f'releases/distribution-{approved}/'
    if archive.sha(root/prefix/'distribution.json') != approved:
        raise ValueError('retained_distribution_mismatch')
    known = set()
    if staged['previous']:
        if previous_proof is None or archive.sha(previous_proof/'archive.json') != staged['previous']:
            raise ValueError('authenticated_previous_generation_required')
        old = descriptor_proof(previous_proof, public)
        for name, digest in old['files'].items():
            if retained_path(name) and staged['files'].get(name) != digest:
                raise ValueError('retained_bytes_removed_or_replaced')
        known.update((r['package'], r['version'], r['architecture'], r['sha256']) for r in old['packages'])
    current = set()
    for release in [*data['releases'], data['bootstrap']]:
        for name, record in release['assets'].items():
            if record['kind'] not in PACKAGE_KINDS:
                continue
            matching = [r for r in staged['packages'] if r['sha256'] == record['sha256']]
            if len(matching) != 1:
                raise ValueError('reviewed_package_missing')
            r = matching[0]
            actual = archive.control(root/r['path'])
            if any(r[k] != actual[k] for k in actual):
                raise ValueError('pool_control_mismatch')
            current.add(actual['package'])
            known.add((r['package'], r['version'], r['architecture'], r['sha256']))
        # Original approval/provenance stay byte-identical, with original limits.
        for name, record in release['assets'].items():
            if record['kind'] in ('release-manifest', 'release-approval', 'release-notes', 'source-provenance', 'bootstrap-manifest'):
                if archive.sha(root/prefix/release['tag']/name) != record['sha256']:
                    raise ValueError('retained_release_review_changed')
    if current != set(archive.PACKAGES):
        raise ValueError('complete_four_package_distribution_required')
    for r in staged['packages']:
        if (r['package'], r['version'], r['architecture'], r['sha256']) not in known:
            raise ValueError('unreviewed_extra_package')
        if r['package'] not in archive.PACKAGES or r['path'] != f'pool/main/{r["package"]}/{r["sha256"]}.deb':
            raise ValueError('unexpected_pool_path')
        if staged['files'].get(r['path']) != r['sha256']:
            raise ValueError('package_inventory_mismatch')
    pool = {r['path'] for r in staged['packages']}
    if {name for name in staged['files'] if name.startswith('pool/')} != pool:
        raise ValueError('unreviewed_pool_object')
    links = [dict(url=asset_url(release['tag'], name), **record)
             for release in [*data['releases'], data['bootstrap']]
             for name, record in release['assets'].items()]
    if (root/prefix/'release-assets.json').read_bytes() != archive.canonical(links):
        raise ValueError('source_or_evidence_links_changed')
    # A package inventory alone does not bind what APT sees. Reconstruct indexes,
    # AppStream and the dated Release from approved pool bytes in the trusted
    # verifier, so a modified preparation artifact cannot advertise extra/new
    # versions or an unbounded Valid-Until. No application binary is executed.
    with tempfile.TemporaryDirectory(prefix='halcyon-index-recheck-') as temporary:
        rebuilt = Path(temporary)/'expected'
        archive.stage([root/r['path'] for r in staged['packages']], {}, rebuilt, staged['epoch'])
        expected_files = json.loads((rebuilt/'archive.json').read_bytes())['files']
        for name, digest in expected_files.items():
            if name.startswith('dists/') and staged['files'].get(name) != digest:
                raise ValueError('generated_index_or_release_mismatch')
    return expected


def prepare(inputs, public, work, epoch, bootstrap=False, release_cache=None, bootstrap_cache=None):
    data = load_inputs(inputs, public)
    work.mkdir(parents=True, exist_ok=False)
    previous = None
    previous_sha = None
    if bootstrap and release_cache is None:
        # Initial public deployment is explicit, not a fallback on a network or
        # signature failure. Local cached builds are preparation only.
        try:
            with urlopen(data['archive_url']+'/archive.json', timeout=30):
                raise ValueError('bootstrap_would_discard_existing_archive')
        except HTTPError as error:
            if error.code != 404:
                raise
    if not bootstrap:
        previous = work/'previous'
        fetch_previous(data['archive_url'], previous, public, data['pages_max_bytes'])
        previous_sha = archive.sha(previous/'archive.json')
        proof = work/'previous-proof'
        proof.mkdir()
        for name in ('archive.json', 'archive.json.asc'):
            shutil.copyfile(previous/name, proof/name)
    packages, retained = [], {}
    inputs_sha = archive.sha(inputs)
    prefix = f'releases/distribution-{inputs_sha}/'
    retained[prefix+'distribution.json'] = inputs
    links = []
    for release in [*data['releases'], data['bootstrap']]:
        directory = work/'downloads'/release['tag']
        is_bootstrap = release is data['bootstrap']
        fetch_assets(release, directory, bootstrap_cache if is_bootstrap else release_cache)
        if not is_bootstrap:
            verified = release_gate.verify_published(directory, release['assets']['release-manifest.json']['sha256'])
            if verified.get('compatibility_policy') != release['policy'] or verified['release'] != release['release']:
                raise ValueError('published_release_policy_mismatch')
        else:
            manifest = json.loads((directory/'bootstrap-manifest.json').read_bytes())
            if (manifest['package_version'] != release['version'] or manifest['public_key_sha256'] != archive.sha(public)
                    or any(manifest['artifacts'][n] != r for n, r in release['assets'].items() if n != 'bootstrap-manifest.json')):
                raise ValueError('bootstrap_binding_mismatch')
        for name, record in release['assets'].items():
            if record['kind'] in PACKAGE_KINDS:
                packages.append(directory/name)
            if record['kind'] in ('release-manifest', 'release-approval', 'release-notes', 'source-provenance', 'bootstrap-manifest'):
                retained[prefix+release['tag']+'/'+name] = directory/name
            links.append(dict(url=asset_url(release['tag'], name), **record))
    retained[prefix+'release-assets.json'] = archive.canonical(links)
    unsigned = work/'unsigned'
    archive.stage(packages, retained, unsigned, epoch, previous, previous_sha,
                  dict(policy='published-practical-release', distribution_sha256=inputs_sha))
    # Technical verification is not parent approval; only the protected signing
    # job accepts the independently configured approval SHA from its environment.
    verify(unsigned, inputs, inputs_sha, public, work/'previous-proof' if previous else None)
    archive.run(['appstreamcli', 'validate', '--no-net', str(unsigned/archive.SUITE/'main/dep11/Components-amd64.xml')])
    size = sum(p.stat().st_size for p in unsigned.rglob('*') if p.is_file())
    if size+5*1024*1024 > data['pages_max_bytes']:
        raise ValueError('pages_size_budget_exceeded')
    return dict(archive_sha256=archive.sha(unsigned/'archive.json'), distribution_sha256=inputs_sha,
                unsigned_bytes=size, previous_sha256=previous_sha)


def sign_prepared(prepared, inputs, approved, public, fingerprint, secret, out):
    """Only called in the protected job; secret is never written to an artifact."""
    unsigned = prepared/'unsigned'
    proof = prepared/'previous-proof' if (prepared/'previous-proof').is_dir() else None
    expected = verify(unsigned, inputs, approved, public, proof)
    if not re.fullmatch('[A-F0-9]{40}', fingerprint):
        raise ValueError('explicit_signing_fingerprint_required')
    with tempfile.TemporaryDirectory(prefix='halcyon-signing-only-') as temporary:
        home = Path(temporary)
        home.chmod(0o700)
        try:
            archive.run(['gpg', '--batch', '--homedir', str(home), '--import'], input=secret.encode())
            secret = None
            listing = archive.run(['gpg', '--batch', '--homedir', str(home), '--with-colons', '--list-secret-keys']).decode()
            # Structural check: dummy certification secret (#), usable signing
            # subkey (+). Never rely on a translated diagnostic string.
            rows = [line.split(':') for line in listing.splitlines() if line.startswith(('sec:', 'ssb:'))]
            if not rows or any(row[0] == 'sec' and row[14] != '#' for row in rows):
                raise ValueError('certification_secret_forbidden_in_publisher')
            if not any(row[0] == 'ssb' and row[14] == '+' for row in rows):
                raise ValueError('signing_subkey_missing')
            archive.sign(unsigned, expected, out, home, fingerprint, public)
        finally:
            subprocess.run(['gpgconf', '--homedir', str(home), '--kill', 'gpg-agent'],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    return expected


def pages(signed, inputs, public, out):
    data = load_inputs(inputs, public)
    expected = archive.sha(signed/'archive.json')
    archive.verify_signed(signed, expected, public)
    if json.loads((signed/'archive.json').read_bytes())['review']['distribution_sha256'] != archive.sha(inputs):
        raise ValueError('pages_inputs_differ_from_signed_distribution')
    out.mkdir(parents=True, exist_ok=False)
    shutil.copytree(signed, out/'apt')
    (out/'.nojekyll').touch()
    downloads = out/'apt/bootstrap'
    downloads.mkdir()
    hashes = []
    for name, record in data['bootstrap']['assets'].items():
        if record['kind'] == 'enrollment-deb':
            match = next(r for r in json.loads((signed/'archive.json').read_bytes())['packages'] if r['sha256'] == record['sha256'])
            shutil.copyfile(signed/match['path'], downloads/name)
            hashes.append(f'{record["sha256"]}  {name}\n')
    (downloads/'SHA256SUMS').write_text(''.join(hashes))
    shutil.copyfile(public, downloads/'halcyon-archive-keyring.gpg')
    shutil.copyfile(inputs, downloads/'distribution.json')
    links = []
    seen = set()
    for record_path in sorted(signed.glob('releases/*/release-assets.json')):
        for record in json.loads(record_path.read_bytes()):
            if record['url'] in seen:
                continue
            seen.add(record['url'])
            name = record['url'].rsplit('/', 1)[-1]
            links.append(f'<li><a href="{html.escape(record["url"])}">{html.escape(name)}</a>'
                         f' — SHA256 <code>{record["sha256"]}</code></li>')
    page = ('<!doctype html><html lang="en"><meta charset="utf-8"><title>Halcyon system updates</title>'
            '<h1>Halcyon system updates</h1><p>Packages for noble/main, amd64. '
            'Halcyon includes Islands and sets up system updates on first installation. '
            'Existing update-source preferences are respected. Enable Islands in Dock &amp; panel when wanted.</p>'
            '<p><a href="https://github.com/profilaz3/halcyon/blob/main/docs/DISTRIBUTION.md">'
            'Enrollment, fingerprint verification and removal instructions</a></p>'
            '<p>After enrollment use normal system updates or COSMIC Store → Updates. '
            'Store search availability is separate; this is not a global default Store listing.</p>'
            '<h2>Retained release packages, corresponding source and reviewed evidence</h2><ul>'
            +''.join(links)+'</ul></html>')
    (out/'index.html').write_text(page)
    total = sum(p.stat().st_size for p in out.rglob('*') if p.is_file())
    if total > data['pages_max_bytes']:
        raise ValueError('pages_size_budget_exceeded')
    return total


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('prepare', 'sign', 'pages', 'verify'))
    p.add_argument('--inputs', type=Path, default=INPUTS)
    p.add_argument('--public-keyring', type=Path, default=PUBLIC)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--out', type=Path)
    p.add_argument('--epoch', type=int)
    p.add_argument('--bootstrap', action='store_true')
    p.add_argument('--release-cache', type=Path)
    p.add_argument('--bootstrap-cache', type=Path)
    p.add_argument('--approved-inputs-sha256')
    p.add_argument('--signing-fingerprint')
    a = p.parse_args()
    if a.action == 'sign':
        # Remove from subprocess environment before any verifier/gpg invocation.
        secret = os.environ.pop('HALCYON_APT_SIGNING_KEY', None)
        if not secret or not a.out:
            p.error('sign requires the protected signing secret and --out')
        print(sign_prepared(a.work, a.inputs, a.approved_inputs_sha256, a.public_keyring,
                            a.signing_fingerprint or '', secret, a.out))
    elif a.action == 'prepare':
        print(json.dumps(prepare(a.inputs, a.public_keyring, a.work, a.epoch or int(time.time()),
                                 a.bootstrap, a.release_cache, a.bootstrap_cache), sort_keys=True))
    elif a.action == 'verify':
        print(verify(a.work/'unsigned', a.inputs, a.approved_inputs_sha256, a.public_keyring,
                     a.work/'previous-proof' if (a.work/'previous-proof').exists() else None))
    else:
        if not a.out:
            p.error('pages requires --out')
        print(json.dumps({'pages_bytes': pages(a.work, a.inputs, a.public_keyring, a.out)}))


if __name__ == '__main__':
    main()
