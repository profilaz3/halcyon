#!/usr/bin/env python3
"""Admit reviewed exact artifacts and stage an unsigned deterministic APT archive."""
import argparse
import json
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'compatibility'))
import release_gate
from archive import PACKAGES, control, inventory, sha, stage


def admit(root, reviewed_sha):
    """Review digest arrives out-of-band, never from untrusted candidate output.

    Source provenance is an independently reviewed declaration, not a claim
    that this verifier can reconstruct or attest a build from a source tarball.
    """
    inventory(root)  # Reject symlink/special-file traversal before other reads.
    review_path = root/'distribution-review.json'
    if sha(review_path) != reviewed_sha:
        raise ValueError('distribution_review_digest_mismatch')
    review = json.loads(review_path.read_bytes())
    if (review.get('format') != 'halcyon-distribution-review' or review.get('version') != 1
            or review.get('decision') != 'approve' or not review.get('review_reference')):
        raise ValueError('distribution_review_required')
    manifest = release_gate.verify(root, review['release_manifest_sha256'])
    if (inventory(root/'native-evidence') != review['native_evidence'] or
            sha(root/'approval.json') != review['native_approval_sha256']):
        raise ValueError('reviewed_native_evidence_changed')
    records = manifest['artifacts']
    provenance = root/'build-provenance.json'
    if records['build-provenance.json']['kind'] != 'build-provenance':
        raise ValueError('source_provenance_required')
    mapping = json.loads(provenance.read_bytes())
    if mapping.get('format') != 'halcyon-build-provenance' or mapping.get('version') != 1:
        raise ValueError('source_provenance_schema')
    packages = []
    names = set()
    for filename, record in records.items():
        if record['kind'] not in ('main-deb', 'islands-deb', 'transition-deb', 'enrollment-deb'):
            continue
        path = root/filename
        deb = control(path)
        if deb['package'] in names:
            raise ValueError('one_current_version_per_package_required')
        names.add(deb['package'])
        expected_kind = {'cosmic-ext-halcyon': 'main-deb',
                         'cosmic-ext-halcyon-islands': ('transition-deb' if manifest.get('packaging') == 'bundled-islands' else 'islands-deb')}.get(deb['package'], 'enrollment-deb')
        if record['kind'] != expected_kind:
            raise ValueError('package_kind_mismatch')
        binding = mapping['packages'][filename]
        if (binding['sha256'] != sha(path) or not re.fullmatch('[a-f0-9]{40}', binding['source_commit'])
                or not binding['sources'] or not binding['recipe']):
            raise ValueError('source_binding_invalid')
        for source in binding['sources']:
            if records[source]['kind'] != 'corresponding-source':
                raise ValueError('corresponding_source_required')
        if records[binding['recipe']]['kind'] != 'build-recipe':
            raise ValueError('reviewed_build_recipe_required')
        packages.append(path)
    if names != set(PACKAGES):
        raise ValueError('complete_bootstrap_artifacts_required')
    return packages, review


def build(root, reviewed_sha, out, epoch, previous=None, previous_sha=None):
    packages, review = admit(root, reviewed_sha)
    # Retain the raw observations, source archives, provenance, approval and
    # exact manifest alongside every release, not merely a passing summary.
    prefix = 'releases/' + review['release_manifest_sha256'] + '/'
    retained = {prefix+name: root/name for name in inventory(root)}
    return stage(packages, retained, out, epoch, previous, previous_sha,
                 dict(review_sha256=reviewed_sha, release_manifest_sha256=review['release_manifest_sha256']))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--artifacts', type=Path, required=True)
    p.add_argument('--review-sha256', required=True)
    p.add_argument('--previous', type=Path)
    p.add_argument('--previous-sha256')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--epoch', type=int, required=True)
    a = p.parse_args()
    if bool(a.previous) != bool(a.previous_sha256):
        p.error('--previous and --previous-sha256 must be supplied together')
    print(build(a.artifacts, a.review_sha256, a.out, a.epoch, a.previous, a.previous_sha256))
