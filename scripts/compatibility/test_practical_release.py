"""Final-artifact policy gate. Local repack fixtures, never native qualification."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from bundle_contract import HELPERS,canonical,digest
import practical_release as policy
import release_gate

class Release(unittest.TestCase):
    def test_bundled_review_requires_current_panel_and_penumbra_observations(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            proof = root/'checks.zip'; proof.write_bytes(b'SYNTHETIC TEST ONLY')
            reviewed = root/'reviewed.json'
            bindings = {'packages': {'main': {'sha256': '1'*64}, 'islands': {'elf_sha256': '2'*64}}}
            data = dict(release='0.5.0', packaging='bundled-islands',
                        artifacts={'checks.zip':dict(kind='native-evidence',sha256=digest(proof))})
            record = dict(format='halcyon-final-package-review', version=1,
                decision='approve-release', release='0.5.0',
                policy_sha256=policy.identity(policy.POLICY), bindings=bindings,
                limitations=policy.FINAL_LIMITATIONS,
                observations=dict(installation='passed', settings='passed', panel='passed',
                                  penumbra='passed', cleanup='passed'),
                scope=dict(environment='private-cosmic-and-isolated-package-upgrade',
                           cosmic_family='1.11', compositor_sha256='3'*64),
                evidence=dict(file='checks.zip',sha256=digest(proof)), review_reference='SYNTHETIC')
            with patch.dict(policy.FINAL_REVIEWS, {'0.5.0': reviewed}, clear=True):
                reviewed.write_bytes(canonical(record))
                self.assertEqual(policy.final_package_review(root,data,bindings)['final_package_review_sha256'],digest(reviewed))
                for missing in ('installation', 'settings', 'panel', 'penumbra', 'cleanup'):
                    changed = {**record, 'observations': {k:v for k,v in record['observations'].items() if k != missing}}
                    reviewed.write_bytes(canonical(changed))
                    with self.assertRaises(ValueError): policy.final_package_review(root,data,bindings)
                reviewed.write_bytes(canonical(record))
                with self.assertRaises(ValueError): policy.final_package_review(root,{**data,'packaging':'split'},bindings)
                with self.assertRaises(ValueError): policy.final_package_review(root,data,{'packages':{}})
                proof.write_bytes(b'changed evidence')
                with self.assertRaises(ValueError): policy.final_package_review(root,data,bindings)

    def test_bundled_manifest_requires_one_payload_and_fresh_acceptance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ('main.deb', 'bridge.deb', 'source.tar'):
                (root/name).write_bytes(b'SYNTHETIC ARTIFACT ONLY')
            data = dict(release='0.5.0', packaging='bundled-islands', islands_package='main.deb',
                artifacts={name:dict(kind=kind,sha256=digest(root/name)) for name,kind in
                    [('main.deb','main-deb'),('bridge.deb','transition-deb'),('source.tar','corresponding-source')]})
            self.assertEqual(release_gate.artifacts(root,data), ('main.deb','main.deb'))
            with patch.object(policy, 'verify_bundled') as gate:
                policy.verify_download_pin(root,data)
                gate.assert_called_once_with(policy.ROOT,root/'main.deb',root/'bridge.deb')
            with self.assertRaisesRegex(ValueError, 'acceptance_pending'):
                policy.final_native_acceptance(root,data,{'packages':{}})
            with self.assertRaises(ValueError):
                release_gate.artifacts(root,{**data,'islands_package':'bridge.deb'})
            del data['artifacts']['bridge.deb']
            with self.assertRaisesRegex(ValueError, 'legacy_upgrade_bridge_required'):
                policy.verify_download_pin(root,data)

    def test_activation_cli_matches_the_source_embedded_by_settings(self):
        with tempfile.TemporaryDirectory() as temporary:
            cli = Path(temporary)/'halcyon-islands'
            cli.write_bytes((policy.ROOT/'packaging/islands/halcyon-islands').read_bytes())
            self.assertEqual(policy.verify_activation_cli(cli), digest(cli))
            cli.write_bytes(cli.read_bytes()+b'\n# changed helper\n')
            with self.assertRaisesRegex(ValueError, 'activation_cli_source_mismatch'):
                policy.verify_activation_cli(cli)
    def test_release_gate_checks_cargo_and_real_main_control_before_approval(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'Cargo.toml').write_text('[package]\nversion="0.4.0"\n')
            tree = root/'tree'; (tree/'DEBIAN').mkdir(parents=True)
            for name in ('islands.deb', 'source.tar'):
                (root/name).write_bytes(b'fixture only')
            cases = [('cosmic-ext-halcyon','0.4.0','amd64','0.4.0',True),
                     ('cosmic-ext-halcyon','0.4.0-1','amd64','0.4.0',False),
                     ('cosmic-ext-halcyon','0.4.1','amd64','0.4.0',False),
                     ('cosmic-ext-halcyon-dev','0.4.0','amd64','0.4.0',False),
                     ('cosmic-ext-halcyon','0.4.0','arm64','0.4.0',False),
                     ('cosmic-ext-halcyon','0.4.1','amd64','0.4.1',False)]
            for package, version, arch, release, valid in cases:
                with self.subTest(package=package, version=version, arch=arch, release=release):
                    (tree/'DEBIAN/control').write_text(f'Package: {package}\nVersion: {version}\nArchitecture: {arch}\nMaintainer: Fixture <f@example.invalid>\nDescription: isolated fixture\n')
                    subprocess.run(['dpkg-deb','--build',str(tree),str(root/'main.deb')],check=True,stdout=subprocess.DEVNULL)
                    data = dict(format='halcyon-release',version=1,release=release,
                        compatibility_policy=policy.POLICY['id'],islands_package='islands.deb',
                        artifacts={name:dict(kind=kind,sha256=digest(root/name)) for name,kind in
                                   [('main.deb','main-deb'),('islands.deb','islands-deb'),('source.tar','corresponding-source')]})
                    manifest=root/'release-manifest.json'; manifest.write_bytes(canonical(data))
                    with patch.object(release_gate,'ROOT',root), patch.object(policy,'verify') as verify:
                        if valid:
                            self.assertEqual(release_gate.verify(root,digest(manifest)),data)
                            verify.assert_called_once()
                        else:
                            with self.assertRaises(ValueError): release_gate.verify(root,digest(manifest))
                            verify.assert_not_called()
    def test_scoped_final_review_binds_artifacts_and_only_its_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); proof=root/'checks.zip'; proof.write_bytes(b'SYNTHETIC TEST ONLY')
            bindings={'packages':{'main':{'sha256':'1'*64},'islands':{'sha256':'2'*64}}}
            data=dict(release='0.4.0',artifacts={'checks.zip':dict(kind='native-evidence',sha256=digest(proof))})
            record=dict(format='halcyon-final-package-review',version=1,
                decision='approve-release',release='0.4.0',
                policy_sha256=policy.identity(policy.POLICY),bindings=bindings,
                limitations=policy.FINAL_LIMITATIONS,
                observations=dict(installation='passed',settings='passed'),
                scope=dict(environment='native-settings-and-offline-package-upgrade',cosmic_family='1.11',image_sha256='3'*64),
                evidence=dict(file='checks.zip',sha256=digest(proof)),review_reference='SYNTHETIC')
            reviewed=root/'reviewed.json'
            with patch.dict(policy.FINAL_REVIEWS,{'0.4.0':reviewed},clear=True):
                with self.assertRaisesRegex(ValueError,'review_pending'):
                    policy.final_package_review(root,data,bindings)
                reviewed.write_bytes(canonical(record))
                result=policy.final_package_review(root,data,bindings)
                self.assertEqual(result['final_package_review_sha256'],digest(reviewed))
                self.assertNotIn('final_native_acceptance_sha256',result)
                with self.assertRaisesRegex(ValueError,'review_pending'):
                    policy.final_package_review(root,{**data,'release':'0.4.1'},bindings)
                with self.assertRaisesRegex(ValueError,'acceptance_pending'):
                    policy.final_native_acceptance(root,data,bindings)
                for key,value in [('decision','PENDING'),('release','0.4.1'),
                    ('policy_sha256','0'*64),('bindings',{}),('limitations',[]),
                    ('scope',{}),('evidence',{}),('review_reference',' '),
                    ('observations',{'installation':'passed','settings':'passed','startup':'passed'})]:
                    with self.subTest(key=key):
                        reviewed.write_bytes(canonical({**record,key:value}))
                        with self.assertRaises(ValueError):policy.final_package_review(root,data,bindings)
                reviewed.write_bytes(canonical(record));proof.write_bytes(b'changed evidence')
                with self.assertRaisesRegex(ValueError,'review_evidence_mismatch'):
                    policy.final_package_review(root,data,bindings)

    def test_candidate_download_pin_binds_exact_companion_before_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            name = 'cosmic-ext-halcyon-islands_0.4.0_amd64.deb'
            artifact = root/name
            artifact.write_bytes(b'SYNTHETIC PIN TEST ONLY')
            pin = dict(format='halcyon-islands-download', version=1,
                       main_version='0.4.0', package_version='0.4.0', tag='v0.4.0',
                       sha256=digest(artifact), bytes=artifact.stat().st_size)
            data = dict(release='0.4.0', islands_package=name,
                        artifacts={name:dict(kind='islands-deb',sha256=digest(artifact))})
            policy.verify_download_pin(root, data, pin)
            for key, value in [('format','unknown'), ('version',2), ('main_version','0.3.0'),
                               ('package_version','0.3.0'), ('tag','v0.3.0'),
                               ('sha256','0'*64), ('bytes',1)]:
                with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'download_pin_mismatch'):
                    policy.verify_download_pin(root, data, {**pin,key:value})
            artifact.write_bytes(b'CHANGED BYTES')
            with self.assertRaisesRegex(ValueError, 'download_pin_mismatch'):
                policy.verify_download_pin(root, data, pin)

    def test_fresh_native_acceptance_requires_reviewed_source_and_exact_artifacts(self):
        # Schema/byte-binding test only. This fixture is never native evidence.
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); proof=root/'native.zip'; proof.write_bytes(b'SYNTHETIC TEST ONLY')
            bindings=dict(packages={'main':{'sha256':'1'*64,'elf_sha256':'2'*64},
                                    'islands':{'sha256':'3'*64,'elf_sha256':'4'*64}},
                          payload_sha256='5'*64,catalog_sha256='6'*64,helpers={'fixture.py':'7'*64})
            data=dict(release='0.3.0',artifacts={'native.zip':dict(kind='native-evidence',sha256=digest(proof))})
            record=dict(format='halcyon-final-package-native-acceptance',version=1,
                        decision='approve-focused-native',release='0.3.0',
                        policy_sha256=policy.identity(policy.POLICY),bindings=bindings,
                        limitations=policy.FINAL_LIMITATIONS,
                        scope=dict(environment='complete-login-session',cosmic_family='1.11',
                                   graphics='virtio-llvmpipe',image_sha256='8'*64),
                        observations={k:'passed' for k in ('installation','settings','startup','input','cleanup','stock-recovery','relogin')},
                        evidence=dict(file='native.zip',sha256=digest(proof)),review_reference='SYNTHETIC TEST ONLY')
            accepted=root/'maintainer-record.json'
            with patch.dict(policy.FINAL_ACCEPTANCES,{'0.3.0':accepted},clear=True):
                with self.assertRaisesRegex(ValueError,'acceptance_pending'):
                    policy.final_native_acceptance(root,data,bindings)
                accepted.write_bytes(canonical(record))
                result=policy.final_native_acceptance(root,data,bindings)
                self.assertEqual(result['final_native_acceptance_sha256'],digest(accepted))
                self.assertEqual(result['final_native_evidence_sha256'],digest(proof))
                changes=[('decision','PENDING'),('release','0.2.5'),('policy_sha256','0'*64),
                         ('bindings',{}),('limitations',[]),('scope',{}),('observations',{}),
                         ('evidence',{}),('review_reference',' ')]
                for key,value in changes:
                    with self.subTest(key=key):
                        accepted.write_bytes(canonical({**record,key:value}))
                        with self.assertRaises(ValueError):policy.final_native_acceptance(root,data,bindings)
                for name in record['observations']:
                    accepted.write_bytes(canonical({**record,'observations':{**record['observations'],name:'not-run'}}))
                    with self.assertRaisesRegex(ValueError,'observations_incomplete'):
                        policy.final_native_acceptance(root,data,bindings)
                accepted.write_bytes(canonical(record));proof.write_bytes(b'changed raw evidence')
                with self.assertRaisesRegex(ValueError,'evidence_mismatch'):
                    policy.final_native_acceptance(root,data,bindings)

    def test_auditfix_published_renewal_is_hash_bound_and_separate_from_admission(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            records = {}
            for name, kind in [('main.deb', 'main-deb'), ('islands.deb', 'islands-deb'),
                               ('source.tar', 'corresponding-source'), ('evidence.zip', 'native-evidence')]:
                (root/name).write_bytes(('fixture ' + name).encode())
                records[name] = dict(kind=kind, sha256=digest(root/name))
            data = dict(format='halcyon-release', version=1, release='0.2.5',
                        compatibility_policy='cosmic-1.10-1.11-startup-v1',
                        islands_package='islands.deb', artifacts=records)
            (root/'release-manifest.json').write_bytes(canonical(data))
            (root/'approval.json').write_bytes(b'fixture historical approval')
            expected = digest(root/'release-manifest.json')
            pins = {expected: dict(release=data['release'], policy=data['compatibility_policy'],
                                   approval_sha256=digest(root/'approval.json'))}
            with patch.object(release_gate, 'PUBLISHED_RELEASES', pins):
                self.assertEqual(release_gate.verify_published(root, expected), data)
                # Even pinned publications cannot enter new-artifact admission.
                with self.assertRaisesRegex(ValueError, 'unknown_release_policy'):
                    release_gate.verify(root, expected)
                for name in records:
                    original = (root/name).read_bytes()
                    (root/name).write_bytes(b'changed')
                    with self.assertRaisesRegex(ValueError, 'artifact_mismatch'):
                        release_gate.verify_published(root, expected)
                    (root/name).write_bytes(original)
                (root/'approval.json').write_bytes(b'changed approval')
                with self.assertRaisesRegex(ValueError, 'published_release_approval_mismatch'):
                    release_gate.verify_published(root, expected)
                data['release'] = '0.3.0'
                (root/'release-manifest.json').write_bytes(canonical(data))
                with self.assertRaisesRegex(ValueError, 'reviewed_manifest_digest_mismatch'):
                    release_gate.verify_published(root, expected)
                with self.assertRaisesRegex(ValueError, 'unknown_release_policy'):
                    release_gate.verify_published(root, digest(root/'release-manifest.json'))

    def test_exact_approval_fields_and_decision_are_required(self):
        # Approval mechanics only; this synthetic request is not native evidence.
        required = dict(format='halcyon-practical-release-approval', version=1,
                        policy_id=policy.POLICY['id'], policy_sha256='1'*64,
                        release_manifest_sha256='2'*64, payload_sha256='3'*64,
                        catalog_sha256='4'*64, historical_acceptance_sha256='5'*64,
                        artifacts={'main.deb':'6'*64,'islands.deb':'7'*64},
                        limitations=['SYNTHETIC TEST ONLY'])
        with tempfile.TemporaryDirectory() as temporary, patch.object(policy,'audit',return_value=required):
            root=Path(temporary); approval=root/'approval.json'
            accepted={**required,'decision':'approve','review_reference':'SYNTHETIC TEST ONLY'}
            approval.write_bytes(canonical(accepted))
            policy.verify(root,{},required['release_manifest_sha256'])
            for key,value in [('artifacts',{}),('policy_id','old-policy'),('policy_sha256','0'*64),
                              ('payload_sha256','0'*64),('catalog_sha256','0'*64),
                              ('historical_acceptance_sha256','0'*64),('limitations',[]),
                              ('release_manifest_sha256','0'*64),('decision','PENDING'),
                              ('review_reference',' ')]:
                with self.subTest(key=key):
                    approval.write_bytes(canonical({**accepted,key:value}))
                    with self.assertRaisesRegex(ValueError,'not_explicitly_approved'):
                        policy.verify(root,{},required['release_manifest_sha256'])

    def test_dispatch_uses_current_policy_and_checks_immutable_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = {}
            for name, kind in [('main.deb', 'main-deb'), ('islands.deb', 'islands-deb'), ('source.tar', 'corresponding-source')]:
                (root/name).write_bytes(b'synthetic dispatch fixture')
                artifacts[name] = dict(kind=kind, sha256=digest(root/name))
            data = dict(format='halcyon-release', version=1, release='0.3.0',
                        compatibility_policy=policy.POLICY['id'], islands_package='islands.deb', artifacts=artifacts)
            manifest = root/'release-manifest.json'
            def write():
                manifest.write_bytes(canonical(data))
                return digest(manifest)
            expected = write()
            with patch.object(policy, 'verify') as verify, patch.object(release_gate, 'promotion', side_effect=AssertionError('wrong verifier')), patch.object(release_gate, 'verify_main_package') as version_check:
                self.assertEqual(release_gate.verify(root, expected), data)
                version_check.assert_called_once_with(release_gate.ROOT, root/'main.deb', data['release'])
                verify.assert_called_once_with(root, data, expected)
                verify.side_effect = ValueError('practical_release_not_explicitly_approved')
                with self.assertRaisesRegex(ValueError, 'not_explicitly_approved'):
                    release_gate.verify(root, expected)
                verify.reset_mock()
                for wrong in ('cosmic-1.10-1.11-startup-v1', 'unknown-policy'):
                    data['compatibility_policy'] = wrong
                    with self.assertRaisesRegex(ValueError, 'unknown_release_policy'):
                        release_gate.verify(root, write())
                verify.assert_not_called()
                data['compatibility_policy'] = policy.POLICY['id']; expected = write()
                (root/'islands.deb').write_bytes(b'changed archive')
                with self.assertRaisesRegex(ValueError, 'artifact_mismatch'):
                    release_gate.verify(root, expected)
                verify.assert_not_called()
                with self.assertRaisesRegex(ValueError, 'reviewed_manifest_digest_mismatch'):
                    release_gate.verify(root, '0'*64)

    def test_explicit_final_approval_not_native_matrix_or_old_deb_reuse(self):
        receipt=os.environ.get('HALCYON_POLICY_TEST_RECEIPT')
        if not receipt:self.skipTest('Set retained integration.5 receipt; no network')
        record=json.loads(Path(receipt).read_bytes())
        with tempfile.TemporaryDirectory(prefix='halcyon-policy-package-fixture-') as work:
            root=Path(work)
            for kind in ('main','islands'):
                p=Path(record['packages'][kind]['deb_path'])
                self.assertEqual(digest(p),record['packages'][kind]['sha256'])
                subprocess.run(['dpkg-deb','-x',str(p),str(root/kind)],check=True,timeout=30)
            library=root/'islands/usr/lib/cosmic-ext-halcyon-islands'
            spec=importlib.util.spec_from_file_location('policy_builder',policy.ROOT/'packaging/islands/build-deb.py')
            builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
            if digest(library/'cosmic-panel-islands') != policy.POLICY['binary_sha256']:
                # integration.5 predates startup-v2's native change. It cannot
                # be repackaged under today's policy or provide positive native
                # approval. Exercise that rejection without rewriting its pins.
                with self.assertRaisesRegex(ValueError,'startup_native_source_or_interface'):
                    builder.build(library/'cosmic-panel-islands',library/'companion-manifest.json',
                        '0.2.5~policy.fixture.1',root/'out',
                        licenses=root/'islands/usr/share/doc/cosmic-ext-halcyon-islands/THIRD_PARTY_LICENSES.txt')
                self.assertEqual(list((root/'out').glob('*.deb')),[])
                return
            package=builder.build(library/'cosmic-panel-islands',library/'companion-manifest.json',
                '0.2.5~policy.fixture.1',root/'out',
                licenses=root/'islands/usr/share/doc/cosmic-ext-halcyon-islands/THIRD_PARTY_LICENSES.txt')
            app=root/'main/usr/lib/cosmic-ext-halcyon'
            for name in HELPERS:
                if name=='selector.py':continue
                shutil.copyfile(policy.ROOT/'companion'/name,app/('islands-status.py' if name=='install_status.py' else name))
            # Disposable packaging fixture, deliberately not a release main deb.
            control=root/'main/DEBIAN';control.mkdir()
            (control/'control').write_text('Package: cosmic-ext-halcyon\nVersion: 0.2.5~policy.fixture.1\nArchitecture: amd64\nMaintainer: Fixture <fixture@example.invalid>\nDescription: NOT RELEASE; policy gate fixture\n')
            main=root/'out/main.deb'
            subprocess.run(['dpkg-deb','--root-owner-group','--build',str(root/'main'),str(main)],check=True,timeout=30,stdout=subprocess.DEVNULL)
            root=root/'out';source=root/'source-fixture.txt';source.write_text('NOT CORRESPONDING SOURCE; SYNTHETIC POLICY GATE TEST ONLY')
            data=dict(format='halcyon-release',version=1,release='0.2.5',compatibility_policy=policy.POLICY['id'],
                islands_package=package.name,artifacts={p.name:dict(kind=k,sha256=digest(p)) for p,k in (
                    (package,'islands-deb'),(main,'main-deb'),(source,'corresponding-source'))})
            manifest=root/'release-manifest.json';manifest.write_bytes(canonical(data));expected=digest(manifest)
            request=policy.audit(root,data,expected)
            self.assertNotEqual(request['payload_sha256'],'f3bb6c779355b9fb4180a18c84dd7453e8d00b51720edc7376d8535bceb15635')
            approval=root/'approval.json';approval.write_bytes(canonical({**request,'decision':'PENDING'}))
            with self.assertRaises(ValueError):release_gate.verify(root,expected)
            accepted={**request,'decision':'approve','review_reference':'SYNTHETIC TEST ONLY'}
            approval.write_bytes(canonical(accepted));approval.chmod(0o644)
            with patch('release_gate.promotion',side_effect=AssertionError('must not manufacture full matrix')):
                self.assertEqual(release_gate.verify(root,expected),data)
            for key,value in [('artifacts',{}),('payload_sha256','0'*64),('historical_acceptance_sha256','0'*64),('limitations',[]),('release_manifest_sha256','0'*64)]:
                approval.write_bytes(canonical({**accepted,key:value}))
                with self.assertRaises(ValueError):policy.verify(root,data,expected)
            approval.write_bytes(canonical(accepted))
            # Repack outside reviewed byte inventory cannot inherit approval.
            with package.open('ab') as f:f.write(b'changed final archive')
            with self.assertRaisesRegex(ValueError,'artifact_mismatch'):release_gate.verify(root,expected)

if __name__=='__main__':unittest.main()
