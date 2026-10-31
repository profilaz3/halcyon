"""Final artifact audit for the explicit supported-family product policy.

This is not qualify.promotion and never creates a native-session result. A parent
approval binds FINAL artifact bytes after packaging. New native ELFs require a
reviewed final-package acceptance in the release source; historical acceptance
never qualifies a rebuilt binary.
"""
import json
import re
from pathlib import Path
import subprocess
import tempfile
import sys
from bundle_contract import (HELPERS, digest, identity, read_json, verify_bundle,
                             read_catalog, release_catalog)
from runtime_compatibility import POLICY, validate_bundle

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'packaging'))
from version_policy import verify_pair, verify_bundled
ACCEPTANCE=ROOT/'companion/compatibility/evidence/accepted-runtime-40071768899.json'
FINAL_ACCEPTANCES={'0.3.0': ROOT/'companion/compatibility/evidence/accepted-final-030.json'}
# Explicit frozen release reviews; candidate metadata cannot choose its scope.
FINAL_REVIEWS={
    '0.4.0': ROOT/'companion/compatibility/evidence/reviewed-final-040.json',
    '0.4.1': ROOT/'companion/compatibility/evidence/reviewed-final-041.json',
    '0.5.0': ROOT/'companion/compatibility/evidence/reviewed-final-050.json',
    '0.5.5': ROOT/'companion/compatibility/evidence/reviewed-final-055.json',
    '0.5.6': ROOT/'companion/compatibility/evidence/reviewed-final-056.json',
}
LIMITATIONS=['physical-hardware-not-qualified','cosmic-1.10-not-native-qualified',
             'new-family-startup-helpers-not-native-qualified','not-full-native-matrix']
FINAL_LIMITATIONS=['physical-hardware-not-qualified','cosmic-1.10-not-native-qualified',
                   'not-full-native-matrix']


def final_package_review(root, data, bindings):
    """Check the exact packages and the completed checks recorded in release source."""
    path=FINAL_REVIEWS.get(data.get('release'))
    if path is None or not path.is_file():
        raise ValueError('final_package_review_pending')
    record=read_json(path)
    bundled = data.get('packaging') == 'bundled-islands'
    observations = dict(installation='passed', settings='passed')
    if bundled:
        if data.get('packaging') != 'bundled-islands':
            raise ValueError('bundled_final_review_requires_bundled_package')
        observations.update(panel='passed', penumbra='passed', cleanup='passed')
    required=dict(format='halcyon-final-package-review',version=1,
                  decision='approve-release',release=data['release'],
                  policy_sha256=identity(POLICY),bindings=bindings,
                  limitations=FINAL_LIMITATIONS,
                  observations=observations)
    if any(record.get(k)!=v for k,v in required.items()):
        raise ValueError('final_package_review_mismatch')
    scope=record.get('scope',{})
    environment = ('private-cosmic-and-isolated-package-upgrade' if bundled
                   else 'native-settings-and-offline-package-upgrade')
    host_key = 'compositor_sha256' if bundled else 'image_sha256'
    if (scope.get('environment')!=environment
            or scope.get('cosmic_family')!='1.11'
            or not isinstance(scope.get(host_key),str)
            or re.fullmatch('[0-9a-f]{64}',scope[host_key]) is None):
        raise ValueError('final_package_review_scope_missing')
    evidence=record.get('evidence',{}); name=evidence.get('file','')
    artifact=data['artifacts'].get(name,{})
    if (not name or Path(name).name!=name or artifact.get('kind')!='native-evidence'
            or artifact.get('sha256')!=evidence.get('sha256')
            or digest(root/name)!=evidence.get('sha256')):
        raise ValueError('final_package_review_evidence_mismatch')
    if not isinstance(record.get('review_reference'),str) or not record['review_reference'].strip():
        raise ValueError('final_package_review_reference_missing')
    return dict(final_package_review_sha256=digest(path),
                release_evidence_sha256=evidence['sha256'],
                release_scope=scope,limitations=FINAL_LIMITATIONS)


def final_native_acceptance(root, data, bindings):
    """Only maintainer-reviewed source records can admit new final package bytes."""
    path=FINAL_ACCEPTANCES.get(data.get('release'))
    if path is None or not path.is_file():
        raise ValueError('final_native_acceptance_pending')
    record=read_json(path)
    required=dict(format='halcyon-final-package-native-acceptance',version=1,
                  decision='approve-focused-native',release=data['release'],
                  policy_sha256=identity(POLICY),bindings=bindings,
                  limitations=FINAL_LIMITATIONS)
    if any(record.get(k)!=v for k,v in required.items()):
        raise ValueError('final_native_acceptance_mismatch')
    scope=record.get('scope',{})
    if (scope.get('environment')!='complete-login-session'
            or scope.get('cosmic_family')!='1.11'
            or scope.get('graphics')!='virtio-llvmpipe'
            or not isinstance(scope.get('image_sha256'),str)
            or re.fullmatch('[0-9a-f]{64}',scope['image_sha256']) is None):
        raise ValueError('final_native_scope_missing')
    observations=record.get('observations',{})
    for name in ('installation','settings','startup','input','cleanup','stock-recovery','relogin'):
        if observations.get(name)!='passed':
            raise ValueError('final_native_observations_incomplete')
    evidence=record.get('evidence',{})
    name=evidence.get('file','')
    artifact=data['artifacts'].get(name,{})
    if (not name or Path(name).name!=name or artifact.get('kind')!='native-evidence'
            or artifact.get('sha256')!=evidence.get('sha256')
            or digest(root/name)!=evidence.get('sha256')):
        raise ValueError('final_native_evidence_mismatch')
    if not isinstance(record.get('review_reference'),str) or not record['review_reference'].strip():
        raise ValueError('final_native_review_missing')
    return dict(final_native_acceptance_sha256=digest(path),
                final_native_evidence_sha256=evidence['sha256'],
                native_scope=scope,limitations=FINAL_LIMITATIONS)


def verify_download_pin(root, data, pin=None):
    """The embedded download intent must name the exact companion being released."""
    if data.get('packaging') == 'bundled-islands':
        main = [n for n, a in data['artifacts'].items() if a['kind'] == 'main-deb']
        if len(main) != 1 or data['islands_package'] != main[0]:
            raise ValueError('bundled_companion_must_be_in_main')
        transitions = [n for n, a in data['artifacts'].items() if a['kind'] == 'transition-deb']
        if len(transitions) != 1:
            raise ValueError('legacy_upgrade_bridge_required')
        verify_bundled(ROOT, root/main[0], root/transitions[0])
        return
    if pin is None:
        pin = read_json(ROOT/'packaging/islands/download.json')
    name = data['islands_package']
    artifact = data['artifacts'][name]
    version = pin.get('package_version')
    if not isinstance(version, str) or re.fullmatch(r'[0-9][A-Za-z0-9.+~-]*', version) is None:
        raise ValueError('release_download_pin_mismatch')
    required = dict(format='halcyon-islands-download', version=1,
                    main_version=data['release'], package_version=version,
                    tag='v'+version, sha256=digest(root/name),
                    bytes=(root/name).stat().st_size)
    if (any(pin.get(k) != v for k,v in required.items())
            or name != 'cosmic-ext-halcyon-islands_'+pin['package_version']+'_amd64.deb'
            or artifact.get('kind') != 'islands-deb'
            or artifact.get('sha256') != pin['sha256']):
        raise ValueError('release_download_pin_mismatch')


def verify_activation_cli(path):
    value = digest(path)
    if value != digest(ROOT/'packaging/islands/halcyon-islands'):
        raise ValueError('activation_cli_source_mismatch')
    return value


def audit(root, data, expected):
    """Return a review request; no approval is inferred or written."""
    verify_download_pin(root, data)
    main=[n for n,v in data['artifacts'].items() if v['kind']=='main-deb']
    islands=data['islands_package']
    if len(main)!=1: raise ValueError('exact_main_package_required')
    if data.get('packaging') != 'bundled-islands':
        verify_pair(ROOT, root/main[0], root/islands)
    with tempfile.TemporaryDirectory(prefix='halcyon-release-audit-') as temporary:
        work=Path(temporary)
        for name,destination in ((islands,work/'islands'),(main[0],work/'main')):
            subprocess.run(['dpkg-deb','-x',str((root/name).resolve()),str(destination)],check=True,timeout=60)
        library=work/'islands/usr/lib/cosmic-ext-halcyon-islands'
        manifest,catalog=verify_bundle(library,'companion-manifest.json')
        validate_bundle(manifest,catalog)
        if data.get('packaging') == 'bundled-islands' and (
                manifest.get('package_owner') != 'cosmic-ext-halcyon'
                or manifest.get('package_version') != data['release']):
            raise ValueError('bundled_payload_identity_mismatch')
        main_elf=digest(work/'main/usr/bin/halcyon')
        cli = verify_activation_cli(work/'islands/usr/bin/halcyon-islands')
        bindings=dict(packages={
            'main':dict(sha256=digest(root/main[0]),elf_sha256=main_elf),
            'islands':dict(sha256=digest(root/islands),elf_sha256=manifest['binary_sha256'])},
            payload_sha256=manifest['payload_sha256'],catalog_sha256=digest(library/'catalog.json'),
            helpers={name:digest(library/name) for name in HELPERS}, activation_cli_sha256=cli)
        fresh=(final_package_review(root,data,bindings)
               if data.get('release') in FINAL_REVIEWS
               else final_native_acceptance(root,data,bindings))
        # Scripts and policy are audited against this frozen release source,
        # never mislabeled as integration.5's tested helper payload.
        for name in HELPERS:
            if digest(library/name)!=digest(ROOT/'companion'/name): raise ValueError('release_helper_source_mismatch')
        expected_catalog = release_catalog(read_catalog(ROOT/'companion/compatibility/catalog.json'))
        if digest(library/'catalog.json')!=identity(expected_catalog):
            raise ValueError('release_catalog_source_mismatch')
        app=work/'main/usr/lib/cosmic-ext-halcyon'
        for name in ('bundle_contract.py','journal.py','runtime_identity.py','runtime_target.py','runtime_compatibility.py',
                     'panel_configuration.py','verification.py','panel_launcher.py'):
            if digest(app/name)!=digest(library/name): raise ValueError('mixed_status_helpers')
        if digest(app/'islands-status.py')!=digest(library/'install_status.py'):
            raise ValueError('mixed_status_helpers')
        return dict(format='halcyon-practical-release-approval',version=1,
            release_manifest_sha256=expected,policy_id=POLICY['id'],policy_sha256=identity(POLICY),
            historical_acceptance_sha256=digest(ACCEPTANCE),
            payload_sha256=manifest['payload_sha256'],catalog_sha256=digest(library/'catalog.json'),
            artifacts={n:v['sha256'] for n,v in data['artifacts'].items()},**fresh)


def verify(root,data,expected):
    required=audit(root,data,expected)
    approval=read_json(root/'approval.json')
    if (any(approval.get(k)!=v for k,v in required.items()) or approval.get('decision')!='approve'
            or not isinstance(approval.get('review_reference'),str) or not approval['review_reference'].strip()):
        raise ValueError('practical_release_not_explicitly_approved')


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts',type=Path,required=True)
    args=parser.parse_args();manifest=args.artifacts/'release-manifest.json'
    data=read_json(manifest)
    # Request generation also enforces the real artifact byte bindings.
    from release_gate import artifacts
    artifacts(args.artifacts,data)
    print(json.dumps({**audit(args.artifacts,data,digest(manifest)),
                      'decision':'PENDING','review_reference':''},indent=2))
