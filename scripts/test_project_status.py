"""Repository scans never run helpers configured by a discovered project."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('projects', Path(__file__).with_name('project-status.py'))
projects = importlib.util.module_from_spec(spec)
spec.loader.exec_module(projects)


class ProjectBudget(unittest.TestCase):
    def test_slow_repositories_share_budget_and_preserve_fast_results(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ['fast', *[f'slow{i}' for i in range(12)]]]
            for path in paths:
                path.mkdir()
            def status(path, deadline=None):
                if path.name != 'fast':
                    projects.command([sys.executable, '-c', 'import time; time.sleep(5)'],
                                     deadline=deadline)
                return b'# branch.head main\0', path
            started = time.monotonic()
            with patch.object(projects, 'discover', return_value=paths), \
                 patch.object(projects, 'listeners', return_value=[]), \
                 patch.object(projects, 'git_command', side_effect=ValueError('no identity')), \
                 patch.object(projects, 'status', side_effect=status):
                rows = projects.scan([], budget=0.3)['projects']
            self.assertLess(time.monotonic() - started, 1.5)
            self.assertEqual(len(rows), len(paths))
            self.assertEqual([r['name'] for r in rows if not r['error']], ['Fast'])
            self.assertEqual(sum(r['error'] for r in rows), 12)

    def test_exhausted_budget_does_not_launch_more_git_commands(self):
        with patch.object(projects.subprocess, 'run') as run:
            with self.assertRaises(subprocess.TimeoutExpired):
                projects.git_command(Path('/unused'), ['status'], deadline=time.monotonic() - 1)
            run.assert_not_called()


class ProjectRequestInputs(unittest.TestCase):
    def test_request_snapshot_survives_a_to_b_and_a_to_b_to_a(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / 'config.json'
            requested = [str(Path(directory) / 'A')]
            for current in ([str(Path(directory) / 'B')], requested):
                config.write_text(json.dumps({'project_roots': current}))
                with patch.dict(os.environ, {'DESKTOP_CARDS_CONFIG': str(config)}), \
                     patch.object(projects, 'discover', return_value=[]) as discover, \
                     patch.object(projects, 'listeners', side_effect=AssertionError('No process scan')):
                    self.assertEqual(projects.scan(requested)['projects'], [])
                    self.assertEqual(discover.call_args.args, (requested,))
                    self.assertIsInstance(discover.call_args.kwargs["deadline"], float)

    def test_app_wire_uses_snapshot_and_echoes_its_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / 'config.json'
            config.write_text('broken mutable configuration must not be read')
            requested = [str(Path(directory) / 'absent-A')]
            result = subprocess.run(
                [sys.executable, projects.__file__, '--roots-stdin'],
                input=json.dumps(requested).encode(), capture_output=True, timeout=5,
                env={**os.environ, 'DESKTOP_CARDS_CONFIG': str(config)})
            self.assertEqual(result.returncode, 0, result.stderr)
            reply = json.loads(result.stdout)
            self.assertEqual(reply['requested_roots'], requested)
            self.assertEqual(reply['projects'], [])

    def test_request_input_is_bounded_and_typed(self):
        self.assertEqual(projects.read_roots(io.BytesIO(b'[]')), [])
        for payload in (b'null', b'{}', b'[1]', b'"folder"',
                        json.dumps(['x'] * 129).encode(), b' ' * (1024 * 1024 + 1)):
            with self.subTest(size=len(payload)), self.assertRaises(ValueError):
                projects.read_roots(io.BytesIO(payload))


class SafeProjectStatus(unittest.TestCase):
    def test_auditfix_project_metadata_rejects_special_and_oversized_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'safe-project'
            root.mkdir()
            manifest = root / 'package.json'
            probe = '''import importlib.util, pathlib, resource, sys
resource.setrlimit(resource.RLIMIT_AS, (96*1024*1024, 96*1024*1024))
spec=importlib.util.spec_from_file_location('projects',sys.argv[1])
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
print(m.project_title(pathlib.Path(sys.argv[2])))
'''
            for kind in ('zero', 'fifo', 'oversized', 'valid'):
                with self.subTest(kind=kind):
                    manifest.unlink(missing_ok=True)
                    if kind == 'zero':
                        manifest.symlink_to('/dev/zero')
                    elif kind == 'fifo':
                        os.mkfifo(manifest)
                    elif kind == 'oversized':
                        manifest.write_bytes(b' ' * (128*1024+1))
                    else:
                        manifest.write_text('{"displayName":"A real title"}')
                    result = subprocess.run([sys.executable, '-c', probe, projects.__file__, str(root)],
                                            capture_output=True, text=True, timeout=3)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(), 'A real title' if kind == 'valid' else 'Safe Project')

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / 'project'
        self.repo.mkdir()
        env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
        env.update(HOME=str(self.root), XDG_CONFIG_HOME=str(self.root / 'config'),
                   GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull)
        environment = patch.dict(os.environ, env, clear=True)
        environment.start()
        self.addCleanup(environment.stop)
        self.git('init', '-q', '-b', 'main')
        (self.repo / 'tracked').write_text('original content\n')
        self.git('add', 'tracked')
        self.git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                 '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')
        self.marker = self.root / 'executed'
        self.helper = self.root / 'helper'
        self.helper.write_text('#!/bin/sh\nprintf executed >> ' + shlex.quote(str(self.marker)) + '\nexit 1\n')
        self.helper.chmod(0o755)

    def git(self, *args):
        return subprocess.run(['git', '-C', str(self.repo), *args], check=True,
                              capture_output=True).stdout

    def scan(self):
        config = self.root / 'projects.json'
        config.write_text(json.dumps({'project_roots': [str(self.repo)]}))
        with patch.dict(os.environ, {'DESKTOP_CARDS_CONFIG': str(config)}), \
                patch.object(projects, 'listeners', return_value=[]):
            result = json.loads(json.dumps(projects.scan()))
        self.assertIsInstance(result['updated_at'], str)
        self.assertTrue(result['projects'])
        for project in result['projects']:
            for field in ['path', 'name', 'title', 'repository_id', 'group', 'branch']:
                self.assertIsInstance(project[field], str)
            for field in ['changed', 'untracked', 'conflicts', 'ahead', 'behind']:
                self.assertIsInstance(project[field], int)
            for field in ['upstream', 'error']:
                self.assertIsInstance(project[field], bool)
            self.assertIsInstance(project['recent'], (int, float))
            self.assertIsInstance(project['servers'], list)
        self.assertFalse(self.marker.exists(), 'repository helper executed')
        return result['projects']

    def test_empty_or_missing_roots_skip_process_scan_but_later_projects_are_detected(self):
        config = self.root / 'empty-config.json'
        empty = self.root / 'empty'
        empty.mkdir()
        with patch.dict(os.environ, {'DESKTOP_CARDS_CONFIG': str(config)}), \
                patch.object(projects, 'listeners', return_value=[]) as listeners:
            for roots in ([], [str(empty)], [str(self.root / 'unmounted')]):
                config.write_text(json.dumps({'project_roots': roots}))
                result = projects.scan()
                self.assertEqual(result['projects'], [])
                self.assertIsInstance(result['updated_at'], str)
                listeners.assert_not_called()
            config.write_text(json.dumps({'project_roots': [str(self.repo)]}))
            self.assertEqual(len(projects.scan()['projects']), 1)
            listeners.assert_called_once()

    def test_current_config_routing(self):
        # Missing routing, legacy preference, or ignored XDG must not silently
        # show an empty/wrong project list. Exercise the actual helper process.
        current = self.root / '.config/halcyon/config.json'
        custom = self.root / 'custom/halcyon/config.json'
        legacy = self.root / '.config/cosmic-desktop-cards/config.json'
        override = self.root / 'override.json'
        for path in (current, custom, override):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps({'project_roots': [str(self.repo)]}))
        legacy.parent.mkdir(parents=True)
        bins = self.root / 'bin'
        bins.mkdir()
        (bins / 'git').symlink_to('/usr/bin/git')
        for case, extra, stale_legacy in (
            ('fresh current home', {}, False),
            ('stale legacy', {}, True),
            ('custom XDG', {'XDG_CONFIG_HOME': str(self.root / 'custom')}, True),
            ('empty XDG', {'XDG_CONFIG_HOME': ''}, True),
            ('explicit override', {'DESKTOP_CARDS_CONFIG': str(override),
                                   'XDG_CONFIG_HOME': str(self.root / 'missing')}, True),
        ):
            with self.subTest(case=case):
                if stale_legacy:
                    legacy.write_text(json.dumps({'project_roots': []}))
                else:
                    legacy.unlink(missing_ok=True)
                env = dict(os.environ)
                for key in ('DESKTOP_CARDS_CONFIG', 'XDG_CONFIG_HOME'):
                    env.pop(key, None)
                env.update(PATH=str(bins), **extra)
                output = subprocess.check_output(
                    ['/usr/bin/python3', str(Path(projects.__file__).resolve())], env=env)
                rows = json.loads(output)['projects']
                self.assertEqual([row['path'] for row in rows], [str(self.repo)])
                self.assertFalse(rows[0]['error'])
        self.assertFalse(self.marker.exists())

    def configure_helpers(self):
        hooks = self.root / 'hooks'
        hooks.mkdir()
        for name in ['post-index-change', 'post-checkout', 'fsmonitor-watchman']:
            (hooks / name).symlink_to(self.helper)
        for key, value in [('core.fsmonitor', str(self.helper)), ('core.hooksPath', str(hooks)),
                           ('core.sshCommand', str(self.helper)), ('core.pager', str(self.helper)),
                           ('pager.status', str(self.helper)), ('diff.external', str(self.helper)),
                           ('diff.evil.textconv', str(self.helper))]:
            self.git('config', key, value)
        (self.repo / '.gitattributes').write_text('tracked filter=evil diff=evil\n')
        (self.repo / 'tracked').write_text('dirty working tree content\n')

    def test_clean_and_process_filters_are_never_executed(self):
        self.configure_helpers()
        for drivers in [('clean',), ('process',), ('clean', 'process')]:
            with self.subTest(drivers=drivers):
                for driver in drivers:
                    self.git('config', 'filter.evil.' + driver, str(self.helper))
                self.git('config', 'filter.evil.required', 'true')
                result = self.scan()[0]
                self.assertTrue(result['error'])
                self.assertEqual(result['branch'], 'Unknown')
                self.git('config', '--remove-section', 'filter.evil')

    def test_helpers_and_unconfigured_attributes_do_not_block_normal_status(self):
        self.configure_helpers()
        index = self.repo / '.git/index'
        before = (index.read_bytes(), index.stat().st_mtime_ns)
        result = self.scan()[0]
        self.assertFalse(result['error'])
        self.assertEqual(result['branch'], 'main')
        self.assertEqual(result['changed'], 1)
        self.assertEqual(result['untracked'], 1)
        self.assertEqual((index.read_bytes(), index.stat().st_mtime_ns), before)

    def test_config_change_after_snapshot_cannot_enable_a_filter(self):
        self.configure_helpers()
        real = projects.git_command
        def command(path, args, deadline=None):
            if 'status' in args:
                self.git('config', 'filter.evil.process', str(self.helper))
                self.git('config', 'filter.evil.clean', str(self.helper))
            return real(path, args, deadline=deadline)
        with patch.object(projects, 'git_command', command):
            result = self.scan()[0]
        self.assertFalse(result['error'])
        self.assertEqual(result['changed'], 1)

    def test_global_and_environment_config_cannot_enable_helpers(self):
        self.configure_helpers()
        (self.root / '.gitconfig').write_text('[filter "evil"]\n\tprocess = ' + str(self.helper) + '\n')
        with patch.dict(os.environ, {'GIT_CONFIG_COUNT': '1', 'GIT_CONFIG_KEY_0': 'filter.evil.process',
                                     'GIT_CONFIG_VALUE_0': str(self.helper), 'GIT_EXTERNAL_DIFF': str(self.helper)}):
            result = self.scan()[0]
        self.assertFalse(result['error'])
        self.assertEqual(result['changed'], 1)

    def test_included_worktree_filter_config_is_unavailable(self):
        checkout = self.root / 'linked'
        self.git('worktree', 'add', '-qb', 'linked', str(checkout))
        self.git('config', 'extensions.worktreeConfig', 'true')
        included = self.root / 'included'
        included.write_text('[filter "evil"]\n\tclean = ' + str(self.helper) + '\n')
        self.git('config', '--worktree', 'include.path', str(included))
        self.configure_helpers()
        rows = {row['path']: row for row in self.scan()}
        self.assertTrue(rows[str(self.repo)]['error'])
        self.assertFalse(rows[str(checkout)]['error'])

    def test_dirty_submodule_helpers_are_not_executed(self):
        submodule = self.repo / 'sub'
        submodule.mkdir()
        subprocess.run(['git', 'init', '-q', str(submodule)], check=True, capture_output=True)
        (submodule / 'tracked').write_text('original submodule content\n')
        subprocess.run(['git', '-C', str(submodule), 'add', 'tracked'], check=True, capture_output=True)
        subprocess.run(['git', '-C', str(submodule), '-c', 'user.name=Test',
                        '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false',
                        'commit', '-qm', 'submodule fixture'], check=True, capture_output=True)
        commit = subprocess.run(['git', '-C', str(submodule), 'rev-parse', 'HEAD'],
                                check=True, capture_output=True).stdout.decode().strip()
        self.git('update-index', '--add', '--cacheinfo', '160000,' + commit + ',sub')
        subprocess.run(['git', '-C', str(submodule), 'config', 'core.fsmonitor', str(self.helper)],
                       check=True, capture_output=True)
        subprocess.run(['git', '-C', str(submodule), 'config', 'filter.evil.process', str(self.helper)],
                       check=True, capture_output=True)
        (submodule / '.gitattributes').write_text('* filter=evil\n')
        (submodule / 'tracked').write_text('dirty submodule work\n')
        self.configure_helpers()
        result = self.scan()[0]
        self.assertFalse(result['error'])
        self.assertGreaterEqual(result['changed'], 1)

    def test_split_index_and_upstream_tracking_are_preserved(self):
        commit = self.git('rev-parse', 'HEAD').decode().strip()
        self.git('config', 'remote.origin.url', 'https://example.invalid/project')
        self.git('config', 'remote.origin.fetch', '+refs/heads/*:refs/remotes/origin/*')
        self.git('update-ref', 'refs/remotes/origin/main', commit)
        self.git('config', 'branch.main.remote', 'origin')
        self.git('config', 'branch.main.merge', 'refs/heads/main')
        self.git('update-index', '--split-index')
        (self.repo / 'tracked').write_text('dirty content\n')
        result = self.scan()[0]
        self.assertFalse(result['error'])
        self.assertTrue(result['upstream'])
        self.assertEqual((result['ahead'], result['behind']), (0, 0))
        self.assertEqual(result['changed'], 1)


if __name__ == '__main__':
    unittest.main()
