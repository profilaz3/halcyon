import fcntl
from concurrent.futures import ThreadPoolExecutor
from contextlib import redirect_stdout
import importlib.machinery
import importlib.util
import io
import json
import re
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

loader = importlib.machinery.SourceFileLoader('controller', str(Path(__file__).with_name('desktop-cards')))
spec = importlib.util.spec_from_loader(loader.name, loader)
controller = importlib.util.module_from_spec(spec)
loader.exec_module(controller)

class ControllerTests(unittest.TestCase):
    def test_geocoding_reads_and_results_are_bounded(self):
        with self.assertRaisesRegex(RuntimeError, 'too large'):
            controller.city_results(io.BytesIO(b' ' * (64 * 1024 + 1)))
        valid = dict(name='Zagreb',latitude=45.8,longitude=16)
        rows = [{**valid, 'name':'x'*257}, {**valid,'latitude':91}, {**valid,'longitude':True}, *([valid]*20)]
        self.assertEqual(controller.city_results(io.BytesIO(json.dumps(dict(results=rows)).encode())), [valid]*8)
        with patch.object(controller.urllib.request, 'urlopen') as fetch:
            with self.assertRaisesRegex(RuntimeError, 'too long'):
                controller.set_weather('x'*257)
            fetch.assert_not_called()

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.config = Path(self.directory.name) / 'config.json'
        self.patch = patch.object(controller, 'CONFIG_FILE', self.config)
        self.patch.start()
        self.addCleanup(self.patch.stop)
        folder = patch.object(controller, 'CONFIG', Path(self.directory.name))
        folder.start()
        self.addCleanup(folder.stop)
        for attribute, name in [('BACKGROUND', 'background.ron'), ('CUSTOM', 'custom.ron'),
                                ('WALLPAPERS', 'wallpapers')]:
            isolated = patch.object(controller, attribute, Path(self.directory.name) / name)
            isolated.start()
            self.addCleanup(isolated.stop)
        # Never reach the installed helper: every subprocess is recorded.
        helper = patch.object(controller.subprocess, 'run',
                              return_value=subprocess.CompletedProcess([], 0, '', ''))
        self.run = helper.start()
        self.addCleanup(helper.stop)
        self.helper = str(controller.USER_DIR / '.local/bin/cosmic-desktop-cards')

    def links_calls(self):
        return [call.args[0] for call in self.run.call_args_list
                if call.args and call.args[0][1:2] == ['--apply-palette-links']]

    def assert_one_link_call(self, name, *flags):
        self.assertEqual(self.links_calls(), [[self.helper, '--apply-palette-links', name, *flags]])
        self.assertEqual(len(self.run.call_args_list), 1, 'the helper is the only command')

    def test_settings_launches_the_native_window_on_the_requested_page(self):
        with patch.object(controller.subprocess, 'run') as run:
            controller.settings('calendar')
            run.assert_called_once_with(
                [str(controller.USER_DIR / '.local/bin/cosmic-desktop-cards'), '--settings', 'calendar'],
                check=True)

    def test_settings_ownership_is_only_in_the_native_entry_point(self):
        # Even a held legacy controller lock must not intercept native routing.
        with self.config.with_name('settings.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            controller.settings('weather')
        self.run.assert_called_once_with([str(controller.BINARY), '--settings', 'weather'], check=True)
        self.assertFalse(self.config.with_name('settings-command.json').exists())

    def test_cli_settings_route_and_weather_shortcut_use_graphical_pages(self):
        with patch.object(controller.sys, 'argv', ['desktop-cards', 'settings', '--page', 'projects']):
            with patch.object(controller, 'settings') as settings:
                controller.main()
                settings.assert_called_once_with('projects')
        with patch.object(controller, 'settings') as settings:
            controller.weather_picker()
            settings.assert_called_once_with('weather')

    def test_every_native_settings_page_is_reachable_from_the_controller(self):
        source = (Path(__file__).parents[1] / 'src/settings.rs').read_text()
        keys = re.search(r'fn key\(self\) -> &\'static str \{(.*?)\n    \}', source, re.S).group(1)
        native = set(re.findall(r'=> "([a-z-]+)"', keys))
        self.assertIn('desktop-motion', native)
        self.assertEqual(native - set(controller.SETTINGS_PAGES), set())

    def test_desktop_subpage_deep_links_and_old_desktop_page_all_route(self):
        for page in ('desktop-dock', 'desktop-appearance', 'desktop-wallpaper', 'desktop-icons',
                     'desktop-profiles', 'desktop-windows', 'desktop-motion',
                     'desktop-workspaces', 'desktop-window-rules', 'desktop-shortcuts',
                     'desktop-automation', 'desktop',
                     'appearance'):
            with patch.object(controller.sys, 'argv', ['desktop-cards', 'settings', '--page', page]):
                with patch.object(controller, 'settings') as settings:
                    controller.main()
                    settings.assert_called_once_with(page)

    def test_tool_pages_toggles_and_focus_commands_reach_the_desktop(self):
        for page in ('music', 'notes', 'focus'):
            with patch.object(controller.sys, 'argv', ['desktop-cards', 'settings', '--page', page]):
                with patch.object(controller, 'settings') as settings:
                    controller.main()
                    settings.assert_called_once_with(page)
        with patch.dict(controller.os.environ, {'XDG_RUNTIME_DIR': self.directory.name}):
            command = Path(self.directory.name) / 'halcyon/command.json'
            with patch.object(controller.sys, 'argv', ['desktop-cards', 'toggle', 'notes']):
                controller.main()
            self.assertEqual(json.loads(command.read_text()), {'action': 'toggle', 'widget': 'notes'})
            with patch.object(controller.sys, 'argv', ['desktop-cards', 'focus', 'toggle']):
                controller.main()
            self.assertEqual(json.loads(command.read_text()), {'action': 'focus', 'do': 'toggle'})
            with patch.object(controller.sys, 'argv', ['desktop-cards', 'focus', 'explode']):
                with self.assertRaises(SystemExit), redirect_stdout(io.StringIO()), \
                        patch.object(controller.sys, 'stderr', io.StringIO()):
                    controller.main()
            for args, sent in [(['timer', '5'], {'action': 'timer', 'minutes': 5.0}),
                               (['timer', '2.5'], {'action': 'timer', 'minutes': 2.5}),
                               (['timer', 'cancel'], {'action': 'timer', 'do': 'cancel'}),
                               (['timer', 'pause'], {'action': 'timer', 'do': 'pause'})]:
                with patch.object(controller.sys, 'argv', ['desktop-cards', *args]):
                    controller.main()
                self.assertEqual(json.loads(command.read_text()), sent)
            for bad in ('0', '-3', '1441', 'soon'):
                with patch.object(controller.sys, 'argv', ['desktop-cards', 'timer', bad]):
                    with self.assertRaises(SystemExit), redirect_stdout(io.StringIO()), \
                            patch.object(controller.sys, 'stderr', io.StringIO()):
                        controller.main()
        self.assertFalse(self.config.exists(), 'tool commands never edit preferences')

    def test_live_palette_and_color_requests_delegate_without_python_writes(self):
        before = b'{"theme":"lavender","future_setting":42}\n'
        self.config.write_bytes(before)
        controller.CUSTOM.write_bytes(b'["/previous.png",]\n')
        for action, args in [(lambda: controller.apply_theme('rose'), ['--select-palette', 'rose']),
                             (lambda: controller.set_cosmic_colors(True), ['--set-cosmic-colors', 'on']),
                             (lambda: controller.set_cosmic_colors(False), ['--set-cosmic-colors', 'off'])]:
            self.run.reset_mock()
            with redirect_stdout(io.StringIO()):
                action()
            self.run.assert_called_once_with([self.helper, *args], text=True, capture_output=True)
            self.assertEqual(self.config.read_bytes(), before)
            self.assertEqual(controller.CUSTOM.read_bytes(), b'["/previous.png",]\n')

    def test_failed_capture_leaves_python_preferences_and_collection_untouched(self):
        for present in (False, True):
            self.config.unlink(missing_ok=True)
            before = b'{"theme":"teal"}\n'
            if present:
                self.config.write_bytes(before)
            controller.CUSTOM.write_bytes(b'future collection bytes\xff')
            self.run.return_value = subprocess.CompletedProcess([], 1, '', 'Could not preserve the desktop')
            with self.assertRaisesRegex(RuntimeError, 'Could not preserve'):
                controller.apply_theme('rose')
            self.assertEqual(self.config.read_bytes() if present else self.config.exists(), before if present else False)
            self.assertEqual(controller.CUSTOM.read_bytes(), b'future collection bytes\xff')

    def test_invalid_palette_never_reaches_helper(self):
        with self.assertRaisesRegex(RuntimeError, 'Unknown'):
            controller.apply_theme('unknown')
        self.run.assert_not_called()
        self.assertFalse(self.config.exists())

    def test_capture_helper_sees_original_preferences_and_no_parent_lock(self):
        original = '{"theme":"teal","appearance":{"match_wallpaper":false,"match_cosmic_colors":true}}\n'
        self.config.write_text(original)
        def inspect(args, **kwargs):
            self.assertEqual(self.config.read_text(), original, 'capture must precede app writes')
            with open(str(self.config) + '.lock', 'a') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return subprocess.CompletedProcess(args, 0, '', '')
        self.run.side_effect = inspect
        with redirect_stdout(io.StringIO()):
            controller.apply_theme('rose')

    def test_preview_palette_only_keeps_wallpaper_and_unrelated_preferences(self):
        data = {'theme': 'lavender', 'appearance': {'match_wallpaper': False, 'opacity': 80,
                                                  'future_tint': 'warm'},
                'clock': {'show_date': False}, 'future_setting': 42}
        self.config.write_text(json.dumps(data))
        controller.BACKGROUND.write_text('(source: Color(Single((0.1, 0.2, 0.3))))\n')
        controller.CUSTOM.write_text('["/previous.png",]\n')
        before = {path: path.read_bytes() for path in (controller.BACKGROUND, controller.CUSTOM)}
        with patch.dict(controller.os.environ, {'DESKTOP_CARDS_CONFIG': str(self.config)}), patch.object(controller, 'bundled_wallpaper') as wallpaper, redirect_stdout(io.StringIO()):
            controller.apply_theme('teal')
            wallpaper.assert_not_called()
        self.run.assert_not_called()
        data['theme'] = 'teal'
        self.assertEqual(json.loads(self.config.read_text()), data)
        self.assertEqual({path: path.read_bytes() for path in before}, before)

    def test_preview_palette_and_cosmic_preferences_never_change_system_settings(self):
        self.config.write_text('{"theme":"lavender","appearance":{"match_cosmic_colors":true}}')
        with patch.dict(controller.os.environ, {'DESKTOP_CARDS_CONFIG': str(self.config)}):
            with patch.object(controller, 'bundled_wallpaper') as wallpaper, patch.object(controller, 'apply_cosmic_colors') as sync:
                with redirect_stdout(io.StringIO()):
                    controller.apply_theme('teal')
                    controller.set_cosmic_colors(True)
                wallpaper.assert_not_called()
                sync.assert_not_called()
        self.run.assert_not_called()
        self.assertEqual(json.loads(self.config.read_text())['theme'], 'teal')

    def test_failed_atomic_write_keeps_previous_file_and_cleans_temporary_file(self):
        self.config.write_text('previous')
        with patch.object(controller.os, 'replace', side_effect=OSError('cannot replace')):
            with self.assertRaises(OSError):
                controller.atomic(self.config, 'new')
        self.assertEqual(self.config.read_text(), 'previous')
        self.assertEqual(list(self.config.parent.iterdir()), [self.config])

    def test_restore_uses_store_helper(self):
        saved = controller.CONFIG / 'original-background.ron'
        with patch.object(controller, 'service') as service:
            with self.assertRaisesRegex(RuntimeError, 'No previous wallpaper backup'):
                controller.restore()
            self.run.assert_not_called()
            saved.write_text('(output: "all", source: Path("/previous.png"))\n')
            with redirect_stdout(io.StringIO()):
                controller.restore()
            self.run.assert_called_once_with([self.helper, '--restore-background', str(saved)],
                                             text=True, capture_output=True)
            service.assert_called_once_with('stop')
            self.assertFalse(controller.BACKGROUND.exists(), 'the helper writes the background')
            service.reset_mock()
            self.run.return_value = subprocess.CompletedProcess([], 1, '', 'not a valid background')
            with self.assertRaisesRegex(RuntimeError, 'not a valid background'):
                controller.restore()
            service.assert_not_called()

    def test_restore_wallpaper_leaves_widgets_startup_and_preferences_unchanged(self):
        saved = controller.CONFIG / 'original-background.ron'
        saved.write_text('(output: "all", source: Path("/previous.png"))\n')
        preferences = '{"enabled_widgets":["calendar","clock"],"theme":"teal"}\n'
        self.config.write_text(preferences)
        with patch.object(controller, 'service') as service, redirect_stdout(io.StringIO()):
            with patch.object(controller.sys, 'argv', ['desktop-cards', 'restore-wallpaper']):
                controller.main()
            # The sole subprocess restores the background through the safe store.
            # In particular, no systemctl disable/stop command changes startup or visibility.
            self.run.assert_called_once_with([self.helper, '--restore-background', str(saved)],
                                             text=True, capture_output=True)
            service.assert_not_called()
            self.assertEqual(self.config.read_text(), preferences)
            self.assertFalse(controller.BACKGROUND.exists(), 'the helper writes the background')

    def test_restore_wallpaper_failures_leave_widgets_and_startup_unchanged(self):
        saved = controller.CONFIG / 'original-background.ron'
        with patch.object(controller, 'service') as service:
            with self.assertRaisesRegex(RuntimeError, 'No previous wallpaper backup'):
                controller.restore_wallpaper()
            self.run.assert_not_called()
            saved.write_text('(output: "all", source: Path("/previous.png"))\n')
            self.run.return_value = subprocess.CompletedProcess([], 1, '', 'wallpaper changed elsewhere')
            with self.assertRaisesRegex(RuntimeError, 'wallpaper changed elsewhere'):
                controller.restore_wallpaper()
            self.run.assert_called_once_with([self.helper, '--restore-background', str(saved)],
                                             text=True, capture_output=True)
            service.assert_not_called()

    def test_dt10_restore_says_startup_is_off_and_how_to_turn_it_back_on(self):
        saved = controller.CONFIG / 'original-background.ron'
        saved.write_text('(output: "all", source: Path("/previous.png"))\n')
        output = io.StringIO()
        with patch.object(controller, 'service') as service, redirect_stdout(output):
            with patch.object(controller.sys, 'argv', ['halcyon-ctl', 'restore']):
                controller.main()
        service.assert_called_once_with('stop')
        self.assertIn('won\u2019t start at sign-in until you run: halcyon-ctl start', output.getvalue())

    def test_dt10_every_command_is_described(self):
        output = io.StringIO()
        with patch.object(controller.sys, 'argv', ['halcyon-ctl', '--help']), redirect_stdout(output):
            with self.assertRaises(SystemExit):
                controller.main()
        text = output.getvalue()
        for name in ('start', 'stop', 'restart', 'status', 'settings', 'drawer', 'panel', 'toggle',
                     'focus', 'timer', 'theme', 'cosmic-colors', 'weather', 'weather-picker', 'restore',
                     'restore-wallpaper', 'wallpapers-install'):
            line = next((line for line in text.splitlines() if line.strip().startswith(name + ' ')
                         or line.strip() == name), None)
            self.assertIsNotNone(line, name)
        self.assertIn('including at sign-in', text)

    def test_ax04_drawer_reaches_the_running_widgets(self):
        with patch.object(controller, 'desktop_command') as command:
            with patch.object(controller.sys, 'argv', ['halcyon-ctl', 'drawer']):
                controller.main()
        command.assert_called_once_with({'action': 'drawer'})

    def test_wallpapers_install_checks_the_installed_wallpapers_offline(self):
        for name in controller.THEMES:
            controller.wallpaper_file(name).parent.mkdir(parents=True, exist_ok=True)
            controller.wallpaper_file(name).write_bytes(b'wallpaper')
        output = io.StringIO()
        with patch.object(controller.sys, 'argv', ['halcyon-ctl', 'wallpapers-install']):
            with patch.object(controller.urllib.request, 'urlopen') as network, redirect_stdout(output):
                controller.main()
                network.assert_not_called()
        self.run.assert_not_called()
        self.assertIn('installed', output.getvalue())

    def test_concurrent_config_writers_preserve_every_update(self):
        barrier = threading.Barrier(8)
        def writer(index):
            barrier.wait()
            def change(data):
                count = data.get('counter', 0)
                time.sleep(0.002)
                data['counter'] = count + 1
                data[f'writer_{index}'] = True
            controller.edit_config(change)
        with ThreadPoolExecutor(max_workers=8) as workers:
            list(workers.map(writer, range(8)))
        data = json.loads(self.config.read_text())
        self.assertEqual(data['counter'], 8)
        self.assertTrue(all(data[f'writer_{i}'] for i in range(8)))
        self.assertTrue(Path(str(self.config) + '.lock').exists())
