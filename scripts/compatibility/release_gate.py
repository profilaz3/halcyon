#!/usr/bin/env python3
"""Verify reviewed immutable artifacts before publication; never rebuild."""
import argparse
import json
from pathlib import Path
import sys
from qualify import promotion
from bundle_contract import digest
from runtime_compatibility import POLICY
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'packaging'))
from version_policy import verify_main_package

# Already published bytes, retained solely for APT metadata renewal. This is
# deliberately independent of distribution.json and candidate-supplied policy.
# New artifacts must pass verify() with the current policy and fresh approval.
PUBLISHED_RELEASES = {
    '5b6fcabfe19fc620bd7d42b8f9774c98c1619732c35ecb7de15840c899a17365': {
        'release': '0.5.5',
        'policy': 'cosmic-1.10-1.11-startup-v4',
        'approval_sha256': '19d84b905891128173ce1feaed96ec3144a1394afb4b29ffb37089f32c55f55f',
    },
    'e3d6d14e4a4073fdf8956ed8558f75471a72009bc6b5dced7562439664403c5d': {
        'release': '0.5.0',
        'policy': 'cosmic-1.10-1.11-startup-v4',
        'approval_sha256': 'd867d30274873679226d01b41c11a2a58d7751c83fa804df6b846513e1bc440c',
    },
    '3f1e2bf222da9c0f80b4e652be3927a49f1bfc2faa930a3a6b279dfe67b09e56': {
        'release': '0.4.1',
        'policy': 'cosmic-1.10-1.11-startup-v3',
        'approval_sha256': 'fa92f7f826a80f16a525738b58fb5632d0f4dd665a90ff7efd1a1f913b784951',
    },
    '7b06a2331dbf250ba7e8d6a91ac8c2851569c57b5e7ea37624621daa1291c7a8': {
        'release': '0.4.0',
        'policy': 'cosmic-1.10-1.11-startup-v3',
        'approval_sha256': '705e5f66b5f04cd25e8841f0616029652c52b09fac8f09536234bf7df8e951d7',
    },
    '6e5facd7eb8cf5cda71d57022ee8f33728148ba102229d66e20c8d8ed4827351': {
        'release': '0.3.0',
        'policy': 'cosmic-1.10-1.11-startup-v2',
        'approval_sha256': '0d9ffca4e176200818a7a7ea5be2b8edce0d017369c50797b4afdbee668e12c3',
    },
    'ea237af6b49635dcb9f0fe03e1523df6aaf66fd332a2729c8a335b8d82cff292': {
        'release': '0.2.5',
        'policy': 'cosmic-1.10-1.11-startup-v1',
        'approval_sha256': '563ac8ecc85fb1d6f29e66e062c7d88393d2cdc67002c05785d42a798eb0e5ab',
    },
}


def verify_published(root, expected):
    """Renew immutable historical publications without admitting new old-policy bytes."""
    published = PUBLISHED_RELEASES.get(expected)
    if published is None:
        return verify(root, expected)
    manifest = root/'release-manifest.json'
    if digest(manifest) != expected:
        raise ValueError('reviewed_manifest_digest_mismatch')
    data = json.loads(manifest.read_bytes())
    if (data.get('format') != 'halcyon-release' or data.get('version') != 1
            or data.get('release') != published['release']
            or data.get('compatibility_policy') != published['policy']):
        raise ValueError('published_release_identity_mismatch')
    artifacts(root, data)  # The pinned manifest binds every package/source/evidence byte.
    if digest(root/'approval.json') != published['approval_sha256']:
        raise ValueError('published_release_approval_mismatch')
    return data


def verify(root, expected):
    manifest=root/'release-manifest.json'
    if digest(manifest)!=expected:raise ValueError('reviewed_manifest_digest_mismatch')
    data=json.loads(manifest.read_bytes())
    if data.get('format')!='halcyon-release' or data.get('version')!=1 or data.get('release')=='0.2.0':
        raise ValueError('release_manifest_invalid')
    package, main = artifacts(root,data)
    if data.get('compatibility_policy') == POLICY['id']:
        verify_main_package(ROOT, root/main, data['release'])
        from practical_release import verify as verify_practical
        verify_practical(root,data,expected)
        return data
    if data.get('compatibility_policy') is not None: raise ValueError('unknown_release_policy')
    verify_main_package(ROOT, root/main, data['release'])
    promotion(root/package,root/'native-evidence',root/'approval.json',root/main)
    return data

def artifacts(root,data):
    kinds=set()
    for name,record in data['artifacts'].items():
        if Path(name).name!=name or digest(root/name)!=record['sha256']:raise ValueError('artifact_mismatch')
        kinds.add(record['kind'])
    bundled = data.get('packaging') == 'bundled-islands'
    required = {'main-deb', 'corresponding-source'} | (set() if bundled else {'islands-deb'})
    if not required <= kinds: raise ValueError('release_incomplete')
    main = [name for name, record in data['artifacts'].items() if record['kind'] == 'main-deb']
    if len(main) != 1:
        raise ValueError('exact_main_package_required')
    package = data['islands_package']
    if bundled:
        if package != main[0] or 'islands-deb' in kinds:
            raise ValueError('bundled_companion_must_be_in_main')
    elif data['artifacts'][package]['kind'] != 'islands-deb':
        raise ValueError('wrong_companion')
    return package, main[0]


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--artifacts',type=Path,required=True);p.add_argument('--manifest-sha256',required=True)
    a=p.parse_args()
    try:verify(a.artifacts,a.manifest_sha256)
    except (OSError,ValueError,KeyError,TypeError):sys.exit('Release blocked: immutable artifacts or reviewed native evidence incomplete')
