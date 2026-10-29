#!/usr/bin/env python3
"""Distribution/workflow boundary tests; all signing secrets are disposable fixtures."""
import io
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tarfile
import unittest
from unittest.mock import patch

import archive
import live
import smoke
import test_archive as fixtures


class LiveDistribution(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixtures.Acceptance.setUpClass()

    @classmethod
    def tearDownClass(cls):
        fixtures.Acceptance.tearDownClass()

    def setUp(self):
        self.f = fixtures.Acceptance()
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.root = self.f.root
        self.inputs = self.root/'distribution.json'
        def asset(path, kind):
            return dict(kind=kind, sha256=archive.sha(path), bytes=path.stat().st_size)
        self.data = dict(format='halcyon-apt-distribution', version=1,
            archive_url=fixtures.enrollment.URI, pages_max_bytes=live.MAX_SITE,
            public_key_sha256=archive.sha(self.f.public),
            releases=[dict(tag='fixture', release='0.2.0', policy='UNIT-FIXTURE-NOT-NATIVE',
                assets={self.f.main.name: asset(self.f.main, 'main-deb'),
                        self.f.islands.name: asset(self.f.islands, 'islands-deb')})],
            bootstrap=dict(tag='fixture-bootstrap', version='1.0',
                assets={p.name: asset(p, 'enrollment-deb') for p in self.f.bootstrap}))

    def prepared(self, previous=False):
        self.inputs.write_bytes(archive.canonical(self.data))
        digest = archive.sha(self.inputs)
        work = self.root/'prepared'
        work.mkdir()
        retained = {f'releases/distribution-{digest}/distribution.json': self.inputs,
                    f'releases/distribution-{digest}/release-assets.json': archive.canonical([
                        dict(url=live.asset_url(release['tag'], name), **record)
                        for release in [*self.data['releases'], self.data['bootstrap']]
                        for name, record in release['assets'].items()])}
        old = self.f.signed if previous else None
        archive.stage([self.f.main, self.f.islands, *self.f.bootstrap], retained, work/'unsigned',
                      self.f.epoch+60, old, self.f.digest if previous else None,
                      dict(policy='published-practical-release', distribution_sha256=digest))
        if previous:
            (work/'previous-proof').mkdir()
            for name in ('archive.json', 'archive.json.asc'):
                shutil.copyfile(old/name, work/'previous-proof'/name)
        return work, digest

    def sign(self, work, digest):
        secret = archive.run(['gpg', '--batch', '--homedir', str(self.f.base/'key-a'),
                              '--armor', '--export-secret-subkeys']).decode()
        signed = self.root/'signed-live'
        live.sign_prepared(work, self.inputs, digest, self.f.public, self.f.subkey, secret, signed)
        return signed

    def test_signing_only_export_works_and_full_primary_is_rejected(self):
        work, digest = self.prepared()
        secret = archive.run(['gpg', '--batch', '--homedir', str(self.f.base/'key-a'),
                              '--armor', '--export-secret-keys']).decode()
        with self.assertRaisesRegex(ValueError, 'certification_secret_forbidden'):
            live.sign_prepared(work, self.inputs, digest, self.f.public, self.f.subkey,
                               secret, self.root/'rejected')
        signed = self.sign(work, digest)
        archive.verify_signed(signed, archive.sha(signed/'archive.json'), self.f.public)

    def test_parent_approval_pin_is_required(self):
        work, _ = self.prepared()
        for approved in (None, '', '0'*64):
            with self.assertRaisesRegex(ValueError, 'parent_distribution_approval_pin_required'):
                live.verify(work/'unsigned', self.inputs, approved, self.f.public)

    def test_bundled_transition_reaches_signed_indexes_and_public_candidates(self):
        cache = self.root/'release-cache'
        cache.mkdir()
        main = fixtures.package(cache, archive.PACKAGES[0], '0.5.0')
        bridge = fixtures.package(cache, archive.PACKAGES[1], '0.5.0',
                                  depends='cosmic-ext-halcyon (>= 0.5.0)')
        release = self.data['releases'][0]
        release['release'] = '0.5.0'
        release['assets'] = {
            p.name: dict(kind=kind, sha256=archive.sha(p), bytes=p.stat().st_size)
            for p, kind in ((main, 'main-deb'), (bridge, 'transition-deb'))}
        # Synthetic packages exercise distribution only, never native approval.
        manifest = cache/'release-manifest.json'
        manifest.write_bytes(archive.canonical({'fixture': 'not-native-approval'}))
        release['assets'][manifest.name] = dict(kind='release-manifest',
            sha256=archive.sha(manifest), bytes=manifest.stat().st_size)
        boot = self.root/'bootstrap-cache'
        boot.mkdir()
        for p in self.f.bootstrap:
            shutil.copyfile(p, boot/p.name)
        manifest = boot/'bootstrap-manifest.json'
        manifest.write_bytes(archive.canonical(dict(package_version='1.0',
            public_key_sha256=archive.sha(self.f.public),
            artifacts=self.data['bootstrap']['assets'])))
        self.data['bootstrap']['assets'][manifest.name] = dict(kind='bootstrap-manifest',
            sha256=archive.sha(manifest), bytes=manifest.stat().st_size)
        self.inputs.write_bytes(archive.canonical(self.data))
        work = self.root/'bundled-prepared'
        with patch.object(live.release_gate, 'verify_published', return_value={
                'release': release['release'], 'compatibility_policy': release['policy']}):
            result = live.prepare(self.inputs, self.f.public, work, self.f.epoch+60,
                                  bootstrap=True, release_cache=cache, bootstrap_cache=boot)
        signed = self.sign(work, result['distribution_sha256'])
        site = self.root/'bundled-site'
        live.pages(signed, self.inputs, self.f.public, site)
        server = fixtures.Server(site)
        self.addCleanup(server.close)
        result = smoke.check(server.uri, self.inputs, self.f.public)
        self.assertEqual(result['candidate_packages'], {
            archive.PACKAGES[0]: archive.sha(main), archive.PACKAGES[1]: archive.sha(bridge)})
        self.assertTrue(result['bootstrap_only_installed'])
        self.assertTrue(result['missing_key_rejected'])

    def test_previous_download_authenticates_descriptor_and_all_retained_bytes(self):
        fetched = self.root/'previous-download'
        live.fetch_previous(self.f.server.uri, fetched, self.f.public, live.MAX_SITE)
        self.assertEqual(archive.sha(fetched/'archive.json'), self.f.digest)
        path = self.f.served/'archive.json'
        path.write_bytes(path.read_bytes()+b'\n')
        with self.assertRaises(Exception):
            live.fetch_previous(self.f.server.uri, self.root/'tampered', self.f.public, live.MAX_SITE)

    def test_download_hash_mismatch_refused(self):
        with self.assertRaisesRegex(ValueError, 'release_or_archive_asset_changed'):
            live.download(self.f.server.uri+'/archive.json', self.root/'bad', 1000000, '0'*64)
        self.assertFalse((self.root/'bad').exists())

    def test_retention_removal_refused_at_signing_boundary(self):
        work, digest = self.prepared(previous=True)
        unsigned = work/'unsigned'
        live.verify(unsigned, self.inputs, digest, self.f.public, work/'previous-proof')
        data = json.loads((unsigned/'archive.json').read_bytes())
        name = next(n for n in data['files'] if '/by-hash/' in n)
        (unsigned/name).write_bytes(b'replaced old index')
        data['files'][name] = archive.sha(unsigned/name)
        (unsigned/'archive.json').write_bytes(archive.canonical(data))
        with self.assertRaisesRegex(ValueError, 'retained_bytes_removed_or_replaced'):
            live.verify(unsigned, self.inputs, digest, self.f.public, work/'previous-proof')

    def test_extra_package_cannot_hide_in_prepared_artifact(self):
        work, digest = self.prepared()
        unsigned = work/'unsigned'
        extra = fixtures.package(self.root, archive.PACKAGES[0], '999.0')
        record = archive.control(extra)
        record['path'] = f'pool/main/{record["package"]}/{record["sha256"]}.deb'
        archive.copy_file(extra, unsigned/record['path'])
        data = json.loads((unsigned/'archive.json').read_bytes())
        data['packages'].append(record)
        data['files'][record['path']] = record['sha256']
        (unsigned/'archive.json').write_bytes(archive.canonical(data))
        with self.assertRaisesRegex(ValueError, 'unreviewed_extra_package'):
            live.verify(unsigned, self.inputs, digest, self.f.public)

    def test_modified_indexes_cannot_be_signed_under_correct_package_inventory(self):
        work, digest = self.prepared()
        unsigned = work/'unsigned'
        path = unsigned/archive.SUITE/'Release'
        path.write_bytes(path.read_bytes().replace(b'Acquire-By-Hash: yes', b'Acquire-By-Hash: no'))
        data = json.loads((unsigned/'archive.json').read_bytes())
        data['files'][str(archive.SUITE/'Release')] = archive.sha(path)
        (unsigned/'archive.json').write_bytes(archive.canonical(data))
        with self.assertRaisesRegex(ValueError, 'generated_index_or_release_mismatch'):
            live.verify(unsigned, self.inputs, digest, self.f.public)

    def test_pages_size_limit_and_signed_input_binding(self):
        self.data['pages_max_bytes'] = 1
        work, digest = self.prepared()
        signed = self.sign(work, digest)
        with self.assertRaisesRegex(ValueError, 'pages_size_budget_exceeded'):
            live.pages(signed, self.inputs, self.f.public, self.root/'oversize')
        self.inputs.write_bytes(self.inputs.read_bytes()+b'\n')
        with self.assertRaisesRegex(ValueError, 'pages_inputs_differ'):
            live.pages(signed, self.inputs, self.f.public, self.root/'changed')

    def test_signed_site_acceptance_bootstrap_optional_islands_and_missing_key(self):
        work, digest = self.prepared()
        signed = self.sign(work, digest)
        site = self.root/'site'
        size = live.pages(signed, self.inputs, self.f.public, site)
        self.assertLess(size, live.MAX_SITE)
        server = fixtures.Server(site)
        self.addCleanup(server.close)
        result = smoke.check(server.uri, self.inputs, self.f.public)
        self.assertEqual(result['result'], 'passed')
        self.assertTrue(result['bootstrap_only_installed'])
        self.assertTrue(result['missing_key_rejected'])

    def test_bootstrap_bytes_and_checksums_match_published_rust_identity(self):
        source = (fixtures.ROOT/'src/settings/updates/identity.rs').read_text()
        source_text = json.loads(re.search(r'const SOURCE_TEXT: &str = ("(?:[^"\\]|\\.)*");', source)[1])
        prefs_text = json.loads(re.search(r'const PREFS_TEXT: &str = ("(?:[^"\\]|\\.)*");', source)[1])
        self.assertEqual(fixtures.enrollment.configuration(), (source_text, prefs_text))
        for deb in self.f.bootstrap:
            payload = archive.run(['dpkg-deb', '--fsys-tarfile', str(deb)])
            control = archive.run(['dpkg-deb', '--ctrl-tarfile', str(deb)])
            with tarfile.open(fileobj=io.BytesIO(control)) as tar:
                sums = tar.extractfile('./md5sums').read().decode()
            with tarfile.open(fileobj=io.BytesIO(payload)) as tar:
                import hashlib
                for member in tar.getmembers():
                    if member.isfile():
                        raw = tar.extractfile(member).read()
                        self.assertIn(hashlib.md5(raw).hexdigest()+'  '+member.name.removeprefix('./'), sums)
                        if member.name == '.'+fixtures.enrollment.SOURCE:
                            self.assertEqual(raw.decode(), source_text)
                        if member.name == '.'+fixtures.enrollment.PREFERENCES:
                            self.assertEqual(raw.decode(), prefs_text)

    def test_workflow_secret_scope_and_daily_trigger(self):
        import yaml
        text = (fixtures.ROOT/'.github/workflows/apt-publish.yml').read_text()
        workflow = yaml.safe_load(text)
        triggers = workflow.get('on', workflow.get(True))
        self.assertEqual(set(triggers), {'workflow_dispatch', 'schedule'})
        self.assertEqual(workflow['jobs']['sign']['environment'], 'halcyon-apt')
        for job in workflow['jobs'].values():
            self.assertIn("github.ref == 'refs/heads/main'", job['if'])
            self.assertIn("default_branch == 'main'", job['if'])
        for step in workflow['jobs']['prepare']['steps']:
            self.assertNotIn('secrets.', str(step))
        self.assertEqual(text.count('secrets.HALCYON_APT_SIGNING_KEY'), 1)
        self.assertIn('vars.HALCYON_APT_DISTRIBUTION_SHA256', text)
        self.assertFalse(workflow['concurrency']['cancel-in-progress'])


if __name__ == '__main__':
    if os.geteuid() != 0:
        os.execvp('unshare', ['unshare', '--user', '--map-root-user', sys.executable, '-B', __file__, *sys.argv[1:]])
    unittest.main()
