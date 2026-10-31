import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import distribution as subject


class DistributionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def archive(self, members):
        archive = self.root/'candidate.tar.gz'
        with tarfile.open(archive, 'w:gz') as stream:
            for name, kind in members:
                item = tarfile.TarInfo(name)
                if kind == 'link':
                    item.type = tarfile.SYMTYPE; item.linkname = '/etc/passwd'
                    stream.addfile(item)
                else:
                    item.size = 1; stream.addfile(item, io.BytesIO(b'x'))
        return archive

    def test_archive_rejects_traversal_links_and_duplicate_entries(self):
        for members in ([('../escape', 'file')], [('link', 'link')],
                        [('same', 'file'), ('same', 'file')], [('/absolute', 'file')]):
            with self.subTest(members=members), tempfile.TemporaryDirectory() as destination:
                with self.assertRaises(ValueError):
                    subject.unpack(self.archive(members), Path(destination))
        self.assertFalse((self.root.parent/'escape').exists())

    def test_archive_checks_decompressed_size(self):
        with patch.object(subject, 'MAX_BYTES', 0):
            with self.assertRaisesRegex(ValueError, 'size_limit'):
                subject.unpack(self.archive([('file', 'file')]), self.root/'out')

    def fixture(self):
        root = self.root/'repo'; root.mkdir()
        package = subject.PREFIX+'cosmic-ext-halcyon-0.5.5-1.fc43.x86_64.rpm'
        names = [package, subject.PREFIX+'RPM-GPG-KEY-halcyon',
                 subject.PREFIX+'repodata/repomd.xml', subject.PREFIX+'repodata/repomd.xml.asc']
        files = {}
        for name in names:
            path = root/name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(name)
            files[name] = dict(sha256=subject.digest(path), bytes=path.stat().st_size)
        source = dict(file='source.tar.gz', kind='corresponding-source', sha256='a'*64, bytes=123)
        evidence = dict(file='evidence.zip', kind='native-evidence', sha256='b'*64, bytes=456)
        record = dict(format='halcyon-rpm-release', version=1, release='0.5.5', target='fedora43-x86_64',
                      package=package, observations=dict.fromkeys(subject.CHECKS, 'passed'),
                      review_reference='fixture', source=source, evidence=evidence, files=files)
        manifest=root/'release.json'; manifest.write_text(json.dumps(record))
        (root/'release.json.asc').write_text('fixture')
        assets={b['file']:{k:b[k] for k in ('kind','sha256','bytes')} for b in (source,evidence)}
        assets[Path(package).name]={'kind':'main-rpm',**files[package]}
        release=dict(release='0.5.5',assets=assets)
        pin=dict(manifest_sha256=subject.digest(manifest),signer='C'*40)
        return root, release, pin, package

    def test_review_must_match_the_approved_manifest(self):
        root,release,pin,_=self.fixture()
        (root/'release.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'review_changed'):
            subject.verify(root,release,pin,self.root/'key')

    def test_tampered_package_is_rejected_before_signature_tools(self):
        root,release,pin,package=self.fixture()
        (root/package).write_text('changed')
        with patch.object(subject,'signature'), patch.object(subject,'run') as run:
            with self.assertRaisesRegex(ValueError,'file_changed'):
                subject.verify(root,release,pin,self.root/'key')
            run.assert_not_called()

    def test_extra_file_and_wrong_source_cannot_enter_the_site(self):
        root,release,pin,_=self.fixture()
        with patch.object(subject,'signature'):
            (root/'unreviewed.rpm').write_text('extra')
            with self.assertRaisesRegex(ValueError,'inventory_mismatch'):
                subject.verify(root,release,pin,self.root/'key')
            (root/'unreviewed.rpm').unlink()
            release['assets']['source.tar.gz']['sha256']='d'*64
            with self.assertRaisesRegex(ValueError,'source_or_evidence_binding'):
                subject.verify(root,release,pin,self.root/'key')

    def test_download_and_dnf_package_must_be_identical(self):
        root,release,pin,package=self.fixture()
        release['assets'][Path(package).name]['sha256']='e'*64
        with patch.object(subject,'signature'):
            with self.assertRaisesRegex(ValueError,'download_and_update_bytes_differ'):
                subject.verify(root,release,pin,self.root/'key')

    def test_historical_deb_only_feed_does_not_create_an_rpm_directory(self):
        self.assertEqual(subject.stage({},None,self.root,None,None),0)
        self.assertFalse((self.root/'rpm').exists())

    def dual_fixture(self):
        root, release, pin, package = self.fixture()
        record = json.loads((root/'release.json').read_text())
        first = {k:record.pop(k) for k in ('target', 'package', 'observations')}
        second = dict(target='fedora44-x86_64', package=package.replace('/43/', '/44/').replace('.fc43.', '.fc44.'), observations=first['observations'].copy())
        record.update(version=2, targets={'43':first, '44':second})
        for old, metadata in list(record['files'].items()):
            name = old.replace('/43/', '/44/').replace('.fc43.', '.fc44.')
            path = root/name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(name)
            record['files'][name] = dict(sha256=subject.digest(path), bytes=path.stat().st_size)
        release['assets'][Path(second['package']).name] = dict(kind='main-rpm', **record['files'][second['package']])
        self.repin(root, record, pin)
        return root, release, pin, record

    def repin(self, root, record, pin):
        (root/'release.json').write_text(json.dumps(record))
        pin['manifest_sha256'] = subject.digest(root/'release.json')

    def test_both_targets_are_verified_in_one_review(self):
        root, release, pin, record = self.dual_fixture()
        with patch.object(subject, 'signature') as signature, patch.object(subject, 'run', return_value='signatures OK') as run:
            self.assertEqual(subject.verify(root, release, pin, self.root/'key'), record)
            self.assertEqual(signature.call_count, 3)
            checked = [c.args[0][-1] for c in run.call_args_list if '--checksig' in c.args[0]]
            self.assertEqual(set(checked), {str(root/t['package']) for t in record['targets'].values()})

    def test_new_review_cannot_drop_fedora43_or_add_unreviewed_platforms(self):
        root, release, pin, record = self.dual_fixture()
        original = record['targets']
        for targets in ({'44':original['44']}, {'43':original['43']}, {**original, '45':original['44']}):
            record['targets'] = targets; self.repin(root, record, pin)
            with patch.object(subject, 'signature'), patch.object(subject, 'run') as run:
                with self.assertRaisesRegex(ValueError, 'review_incomplete'):
                    subject.verify(root, release, pin, self.root/'key')
                run.assert_not_called()

    def test_each_platform_requires_its_own_matching_package_and_complete_review(self):
        root, release, pin, record = self.dual_fixture()
        target = record['targets']['44']; original = target.copy()
        for replacement in (dict(package=record['targets']['43']['package']), dict(target='fedora43-x86_64'), dict(observations={**target['observations'], 'panel':'pending'})):
            record['targets']['44'] = {**original, **replacement}; self.repin(root, record, pin)
            with patch.object(subject, 'signature'):
                with self.assertRaisesRegex(ValueError, 'review_incomplete'):
                    subject.verify(root, release, pin, self.root/'key')

    def test_second_platform_tampering_is_rejected(self):
        root, release, pin, record = self.dual_fixture()
        (root/record['targets']['44']['package']).write_text('tampered')
        with patch.object(subject, 'signature'), patch.object(subject, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'file_changed'):
                subject.verify(root, release, pin, self.root/'key')
            run.assert_not_called()

    def test_historical_fedora43_review_keeps_its_original_contract(self):
        root, release, pin, package = self.fixture()
        with patch.object(subject, 'signature') as signature, patch.object(subject, 'run', return_value='signatures OK') as run:
            record = subject.verify(root, release, pin, self.root/'key')
            self.assertEqual(record['version'], 1)
            self.assertEqual(signature.call_count, 2)
            checked = [c.args[0][-1] for c in run.call_args_list if '--checksig' in c.args[0]]
            self.assertEqual(checked, [str(root/package)])
