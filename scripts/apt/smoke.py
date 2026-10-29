#!/usr/bin/env python3
"""Read-only public feed acceptance using disposable APT/dpkg state, never host APT.

Installs ONLY the two small bootstrap packages under a private root. Application
packages are downloaded and checked, never installed or executed by this smoke.
"""
import argparse
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace

import archive
import live
from test_archive import Apt, enrollment


def check(uri, inputs, public):
    mapping = Path('/proc/self/uid_map').read_text().split()
    if os.geteuid() != 0 or mapping[:1] != ['0'] or mapping[1] == '0' or mapping[2] != '1':
        raise ValueError('unprivileged_user_namespace_required')
    data = live.load_inputs(inputs, public)
    with tempfile.TemporaryDirectory(prefix='halcyon-public-apt-') as temporary:
        root = Path(temporary)
        apt = Apt(root/'client', SimpleNamespace(uri=uri), public)
        packages = []
        for name, record in data['bootstrap']['assets'].items():
            if record['kind'] == 'enrollment-deb':
                path = root/name
                live.download(uri+'/bootstrap/'+name, path, record['bytes'], record['sha256'], record['bytes'])
                packages.append(path)
        apt.source.unlink()
        (apt.root/'etc/apt/preferences.d/halcyon').unlink()
        apt.dpkg('--install', *packages)
        if apt.public.read_bytes() != public.read_bytes():
            raise ValueError('installed_public_key_mismatch')
        # Only relocate absolute Signed-By for the disposable root. Trust bytes,
        # repository URI and four-package Origin preferences remain unchanged.
        apt.source.write_text(apt.source.read_text().replace(enrollment.KEY, str(apt.public)).replace(enrollment.URI, uri))
        apt.cmd('update')
        expected = {}
        for release in data['releases']:
            for name, record in release['assets'].items():
                if record['kind'] in ('main-deb', 'islands-deb', 'transition-deb'):
                    package_name = 'cosmic-ext-halcyon' if record['kind'] == 'main-deb' else 'cosmic-ext-halcyon-islands'
                    policy = apt.cmd('policy', package_name, tool='apt-cache')
                    if f'Candidate: {release["release"]}' not in policy:
                        raise ValueError('public_candidate_mismatch')
                    expected[package_name] = record['sha256']
        # Real authenticated APT package download, without application install.
        old_cwd = Path.cwd()
        try:
            os.chdir(root)
            apt.cmd('download', 'cosmic-ext-halcyon')
        finally:
            os.chdir(old_cwd)
        downloaded = list(root.glob('cosmic-ext-halcyon_*.deb'))
        if len(downloaded) != 1 or archive.sha(downloaded[0]) != expected['cosmic-ext-halcyon']:
            raise ValueError('public_main_download_mismatch')
        status = (apt.root/'var/lib/dpkg/status').read_text()
        if 'Package: cosmic-ext-halcyon-islands\n' in status or 'Package: cosmic-ext-halcyon\n' in status:
            raise ValueError('bootstrap_unexpectedly_installed_application')
        # Trust failure is real APT behavior: no configured key means failure.
        apt.public.unlink()
        apt.cmd('update', ok=False)
        return dict(result='passed', scope='disposable-public-apt', candidate_packages=expected,
                    bootstrap_only_installed=True, missing_key_rejected=True,
                    store_search='not-native-ui-tested')


if __name__ == '__main__':
    if os.geteuid() != 0:
        os.execvp('unshare', ['unshare', '--user', '--map-root-user', sys.executable, '-B', __file__, *sys.argv[1:]])
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--uri', default='https://profilaz3.github.io/halcyon/apt')
    p.add_argument('--inputs', type=Path, default=live.INPUTS)
    p.add_argument('--public-keyring', type=Path, default=live.PUBLIC)
    a = p.parse_args()
    print(json.dumps(check(a.uri, a.inputs.resolve(), a.public_keyring.resolve()), sort_keys=True))
