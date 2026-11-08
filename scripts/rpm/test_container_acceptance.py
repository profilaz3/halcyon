"""Offline tests for the container stage; they never start a container or download."""

import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
spec = importlib.util.spec_from_file_location('container_acceptance', HERE/'container_acceptance.py')
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)
STAGE = HERE/'container-stage.sh'
PREVIOUS = 'cosmic-ext-halcyon-0.6.1-1.fc{}.x86_64.rpm'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def companion_module(name):
    sys.path.insert(0, str(ROOT/'companion'))
    try:
        return acceptance.load(name, ROOT/'companion'/f'{name}.py')
    finally:
        sys.path.remove(str(ROOT/'companion'))


class ReleaseAssetTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.release = self.root/'release'
        self.release.mkdir()
        files = {PREVIOUS.format(43): b'fc43 rpm', PREVIOUS.format(44): b'fc44 rpm',
                 PREVIOUS.format(43) + '.sha256': b'sum',
                 'release-provenance.json': json.dumps(dict(release='0.6.1')).encode()}
        for name, data in files.items():
            (self.release/name).write_bytes(data)
        self.write_sums()

    def write_sums(self):
        lines = [f'{digest(path.read_bytes())}  {path.name}'
                 for path in sorted(self.release.iterdir()) if path.name != 'SHA256SUMS']
        (self.release/'SHA256SUMS').write_text('\n'.join(lines) + '\n')

    def test_sums_select_exactly_one_rpm_per_target(self):
        sums = acceptance.parse_sums((self.release/'SHA256SUMS').read_text() + 'not a sum line\n')
        self.assertEqual(acceptance.previous_rpm(sums, '0.6.1', '43'), (PREVIOUS.format(43), '1'))
        with self.assertRaisesRegex(ValueError, 'found 0'):
            acceptance.previous_rpm(sums, '0.6.0', '43')
        sums['cosmic-ext-halcyon-0.6.1-2.fc43.x86_64.rpm'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'found 2'):
            acceptance.previous_rpm(sums, '0.6.1', '43')

    def test_local_assets_are_copied_and_checked(self):
        provenance, rpms = acceptance.previous_assets('0.6.1', self.root/'out', ('43', '44'), self.release)
        self.assertEqual(provenance['release'], '0.6.1')
        self.assertEqual(rpms['44'], (PREVIOUS.format(44), '1'))
        self.assertEqual((self.root/'out'/PREVIOUS.format(43)).read_bytes(), b'fc43 rpm')

    def test_tampered_asset_is_rejected(self):
        (self.release/PREVIOUS.format(44)).write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'does not match SHA256SUMS'):
            acceptance.previous_assets('0.6.1', self.root/'out', ('43', '44'), self.release)

    def test_provenance_of_another_release_is_rejected(self):
        (self.release/'release-provenance.json').write_text(json.dumps(dict(release='0.6.0')))
        self.write_sums()
        with self.assertRaisesRegex(ValueError, 'another release'):
            acceptance.previous_assets('0.6.1', self.root/'out', ('43',), self.release)


class PanelGuardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        policies = companion_module('runtime_compatibility')
        build = acceptance.load('halcyon_companion_build_test', ROOT/'companion/build.py')
        patches = {path.name: acceptance.sha256(path)
                   for path in sorted((ROOT/'companion/patches').glob('*.patch'))}
        cls.provenance = dict(release='0.6.1', native=dict(upstream=build.REVISION, patches=patches),
                              platforms={f'fedora{n}': dict(host_sha256=getattr(policies, f'FEDORA{n}_POLICY')
                                                            ['binary_sha256']) for n in ('43', '44')})
        cls.policies = policies

    def mutated(self, change):
        provenance = json.loads(json.dumps(self.provenance))
        change(provenance)
        return provenance

    def test_unchanged_panel_is_reused_with_the_current_policies(self):
        selected = acceptance.panel_guard(self.provenance, ('43', '44'))
        self.assertEqual(selected['43'], self.policies.FEDORA43_POLICY)
        self.assertEqual(selected['44'], self.policies.FEDORA44_POLICY)

    def test_changed_panel_source_or_policy_is_refused(self):
        changes = (lambda p: p['native'].update(upstream='0' * 40),
                   lambda p: p['native']['patches'].update({'0002-penumbra-panel.patch': '0' * 64}),
                   lambda p: p['platforms']['fedora44'].update(host_sha256='0' * 64))
        for change in changes:
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, 'companion/build.py'):
                acceptance.panel_guard(self.mutated(change), ('43', '44'))

    def test_catalog_is_valid_and_matches_the_rpm_target(self):
        contract = companion_module('bundle_contract')
        for fedora in ('43', '44'):
            text = acceptance.catalog(getattr(self.policies, f'FEDORA{fedora}_POLICY'))
            record = json.loads(text)
            sys.path.insert(0, str(ROOT/'companion'))
            try:
                self.assertEqual(contract.release_catalog(record), record)
            finally:
                sys.path.remove(str(ROOT/'companion'))
            self.assertEqual(text.encode(), contract.canonical(record) + b'\n')
            self.assertEqual(record['startup_policy']['platform'],
                             dict(id='fedora', version=fedora, architecture='x86_64'))


class ContainerTests(unittest.TestCase):
    def test_default_release_sorts_after_the_previous_package(self):
        self.assertEqual(acceptance.default_release('0.6.1', '0.6.1', '1'), '1.1.test')
        self.assertEqual(acceptance.default_release('0.6.2', '0.6.1', '1'), '0.1.test')

    def test_rootful_engines_hand_files_back_and_rootless_engines_do_not(self):
        rootful = acceptance.stage_environment('0.6.1', '1.1.test', PREVIOUS.format(43), 'a' * 64, False)
        rootless = acceptance.stage_environment('0.6.1', '1.1.test', PREVIOUS.format(43), 'a' * 64, True)
        self.assertIn('HOST_UID', rootful)
        self.assertNotIn('HOST_UID', rootless)
        self.assertEqual(rootless['PREVIOUS_RPM'], 'previous/' + PREVIOUS.format(43))
        command = acceptance.container_command(['podman'], 'img:43', Path('/out'), 'fresh', '43', rootless)
        self.assertEqual(command[:7], ['podman', 'run', '--rm', '-v', '/out:/work:Z',
                                       '-v', '/out/dnf-cache/fc43:/var/cache/libdnf5:Z'])
        self.assertEqual(command[-5:], ['img:43', 'bash', '/work/tools/container-stage.sh', 'fresh', '43'])
        self.assertIn('RPM_RELEASE=1.1.test', command)

    def test_engine_detection(self):
        def info(output, code=0, stderr=''):
            return subprocess.CompletedProcess([], code, stdout=output, stderr=stderr)

        with patch.object(acceptance.subprocess, 'run', return_value=info('true\n')):
            self.assertTrue(acceptance.rootless(['podman']))
        with patch.object(acceptance.subprocess, 'run',
                          side_effect=[info('', 1), info('["name=seccomp","name=rootless"]\n')]):
            self.assertTrue(acceptance.rootless(['docker']))
        denied = info('', 1, 'permission denied while trying to connect to the docker API\n')
        with patch.object(acceptance.subprocess, 'run', side_effect=[denied, denied]):
            with self.assertRaisesRegex(ValueError, 'permission denied'):
                acceptance.rootless(['docker'])

    def test_dry_run_prints_every_stage_and_creates_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)/'acceptance'
            printed = io.StringIO()
            with contextlib.redirect_stdout(printed):
                code = acceptance.main(['run', '--previous', '0.6.1', '--out', str(out),
                                        '--engine', 'podman', '--dry-run'])
            self.assertEqual(code, 0)
            self.assertFalse(out.exists())
        text = printed.getvalue()
        for fedora in ('43', '44'):
            self.assertIn(f'podman pull registry.fedoraproject.org/fedora:{fedora}', text)
            for stage in acceptance.STAGES:
                self.assertIn(f'/work/tools/container-stage.sh {stage} {fedora}', text)


class StageScriptTests(unittest.TestCase):
    def test_syntax(self):
        subprocess.run(['bash', '-n', str(STAGE)], check=True)

    def test_stages_and_environment_match_the_orchestrator(self):
        text = STAGE.read_text()
        self.assertEqual(tuple(re.findall(r'^  ([a-z-]+)\) [a-z]+', text, re.M)), acceptance.STAGES)
        for key in acceptance.stage_environment('0', '1', 'x.rpm', 'a' * 64, False):
            self.assertRegex(text, r'\$\{?' + key + r'\b', key)
        for name in ('build-rpm.py', 'prepare-repository.py'):
            self.assertTrue((ROOT/('packaging' if name == 'build-rpm.py' else 'scripts/rpm')/name).is_file())

    def test_acceptance_repository_stays_out_of_the_enrollment_folder(self):
        text = STAGE.read_text()
        self.assertNotIn('/etc/yum.repos.d/acceptance', text)
        self.assertIn('--setopt=halcyon.enabled=0', text)

    def test_os_release_never_replaces_the_stage_environment(self):
        # os-release defines VERSION; sourcing it in the script's own shell
        # replaced the product version and build-rpm.py refused it.
        text = STAGE.read_text()
        self.assertNotRegex(text, r'(?m)^\s*(?:\.|source)\s+/etc/os-release')
        with tempfile.TemporaryDirectory() as folder:
            release = Path(folder)/'os-release'
            release.write_text('ID=fedora\nVERSION_ID=43\nVERSION="43 (Container Image)"\n')
            line = next(line for line in text.splitlines() if '/etc/os-release' in line and 'read' in line)
            script = 'VERSION=0.6.1\n' + line.replace('/etc/os-release', str(release)) + \
                '\nprintf "%s|%s|%s" "$os_id" "$os_version" "$VERSION"\n'
            result = subprocess.run(['bash', '-c', script], check=True, capture_output=True, text=True)
        self.assertEqual(result.stdout, 'fedora|43|0.6.1')


class RecordTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)

    def test_build_record_has_the_release_shape(self):
        (self.root/'app').write_bytes(b'app')
        (self.root/'host').write_bytes(b'host')
        record = acceptance.provenance_record('43', self.root/'app', self.root/'host', '1.93.0')
        self.assertEqual(set(record), {'format', 'version', 'fedora', 'architecture', 'app_sha256',
                                       'host_sha256', 'rust', 'locked', 'offline'})
        self.assertEqual(record['format'], 'halcyon-fedora-build')
        self.assertEqual(record['app_sha256'], digest(b'app'))

    def test_snapshot_reports_enrollment_files_phase_and_removed_copies(self):
        self.assertEqual(acceptance.snapshot(self.root),
                         dict(files=dict.fromkeys(acceptance.ENROLLMENT), phase=None, removed=[]))
        for name in acceptance.ENROLLMENT[:2]:
            (self.root/name.lstrip('/')).parent.mkdir(parents=True, exist_ok=True)
            (self.root/name.lstrip('/')).write_bytes(b'x')
        receipt = self.root/acceptance.ENROLLMENT[2].lstrip('/')
        receipt.parent.mkdir(parents=True)
        receipt.write_text(json.dumps(dict(phase='disabled')))
        (self.root/'etc/yum.repos.d/halcyon.repo.removed.1').write_text('')
        state = acceptance.snapshot(self.root)
        self.assertEqual(state['files'][acceptance.ENROLLMENT[0]], digest(b'x'))
        self.assertEqual((state['phase'], state['removed']), ('disabled', ['halcyon.repo.removed.1']))

    def test_source_tree_holds_committed_files_only(self):
        repo = self.root/'repo'
        repo.mkdir()
        git = ['git', '-C', str(repo), '-c', 'user.name=Test', '-c', 'user.email=test@invalid']
        subprocess.run([*git, 'init', '-q'], check=True)
        (repo/'tracked').write_text('committed\n')
        subprocess.run([*git, 'add', 'tracked'], check=True)
        subprocess.run([*git, 'commit', '-q', '-m', 'fixture'], check=True)
        (repo/'tracked').write_text('edited\n')
        (repo/'untracked').write_text('new\n')
        commit, dirty, main = acceptance.source_tree(self.root/'out', root=repo)
        self.assertRegex(commit, '^[0-9a-f]{40}$')
        self.assertTrue(dirty)
        self.assertEqual((main/'tracked').read_text(), 'committed\n')
        self.assertFalse((main/'untracked').exists())

    def test_supplied_license_bundle_is_used(self):
        main = self.root/'source/main'
        (main/'packaging/licenses').mkdir(parents=True)
        (self.root/'bundle.txt').write_text('notices\n')
        self.assertEqual(acceptance.licenses(main, self.root/'bundle.txt'), 'supplied')
        self.assertEqual((main/'packaging/licenses/THIRD_PARTY_LICENSES.txt').read_text(), 'notices\n')


if __name__ == '__main__':
    unittest.main()
