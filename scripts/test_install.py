import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('install', Path(__file__).with_name('install.py'))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)

APP_ID = 'io.github.profilaz3.Halcyon'


class InstallTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.home = self.root / 'home'
        self.binary = self.root / 'halcyon'
        self.build(b'release one')
        self.calls = []
        # systemctl answers: which `is-enabled` / `is-active` checks succeed.
        self.ok = set()

    def build(self, contents):
        self.binary.write_bytes(contents)
        self.binary.chmod(0o755)

    def runner(self, args, check=False):
        self.calls.append(args)
        query = tuple(args[2:4]) if len(args) > 3 else ()
        code = 0 if any(query == (verb, name) or (verb in args and name in args) for verb, name in self.ok) else 1
        if args[2:3] not in (['is-enabled'], ['is-active']):
            code = 0
        return subprocess.CompletedProcess(args, code, '', '')

    def install(self, **options):
        return installer.install(self.binary, self.home, run=self.runner, **options)

    def test_first_install_places_the_package_layout_under_local(self):
        result = self.install()
        self.assertTrue(result['first'])
        local = self.home / '.local'
        self.assertEqual((local / 'bin/halcyon').read_bytes(), b'release one')
        for path in (local / 'bin/halcyon', local / 'bin/halcyon-ctl',
                     local / 'lib/cosmic-ext-halcyon/calendar-sync.py',
                     local / 'lib/cosmic-ext-halcyon/project-status.py'):
            self.assertEqual(path.stat().st_mode & 0o777, 0o755, path)
        wallpapers = sorted(p.name for p in (local / 'share/backgrounds/halcyon').iterdir())
        self.assertIn('halcyon-night.jpg', wallpapers)
        self.assertEqual(len(wallpapers), 17)
        self.assertTrue({"halcyon-rain.jpg", "halcyon-snow.jpg", "halcyon-fog.jpg"} <= set(wallpapers))
        activity = local / f'share/applications/{APP_ID}.Activity.desktop'
        self.assertIn('--activity-island', activity.read_text())
        ridge = local / 'share/backgrounds/halcyon/halcyon-ridge.png'
        self.assertEqual(ridge.read_bytes(), (installer.SOURCE / 'assets/wallpapers/halcyon-ridge.png').read_bytes())
        for size in (48, 64, 128, 256, 512):
            self.assertTrue((local / f'share/icons/hicolor/{size}x{size}/apps/{APP_ID}.png').is_file())
        launcher = (local / f'share/applications/{APP_ID}.desktop').read_text()
        self.assertIn(f'Exec={local}/bin/halcyon --settings', launcher)
        unit = (self.home / '.config/systemd/user/halcyon.service').read_text()
        self.assertIn(f'ExecStart={local}/bin/halcyon', unit)
        # An upgrade restart (exit 75) starts the widgets again.
        self.assertIn('\nRestart=on-failure\n', unit)
        self.assertIn('\nRestartForceExitStatus=75\n', unit)
        self.assertTrue((local / 'share/cosmic-ext-halcyon/google-calendar-setup.html').is_file())
        # Nothing is enabled or started on a fresh install.
        self.assertFalse(any('enable' in call or 'restart' in call for call in self.calls))

    def test_an_install_from_before_the_rename_is_taken_over(self):
        units = self.home / '.config/systemd/user'
        units.mkdir(parents=True)
        (units / 'cosmic-desktop-cards.service').write_text('[Service]\n')
        launchers = self.home / '.local/share/applications'
        launchers.mkdir(parents=True)
        (launchers / 'io.github.workstation.DesktopCards.desktop').write_text('[Desktop Entry]\n')
        bin_dir = self.home / '.local/bin'
        bin_dir.mkdir(parents=True)
        (bin_dir / 'desktop-cards').write_text('old controller')
        self.ok = {('is-enabled', 'cosmic-desktop-cards.service'), ('is-active', 'cosmic-desktop-cards.service')}
        result = self.install()
        self.assertTrue(result['took_over'])
        self.assertIn(['systemctl', '--user', 'disable', '--now', 'cosmic-desktop-cards.service'], self.calls)
        self.assertIn(['systemctl', '--user', 'enable', 'halcyon.service'], self.calls)
        self.assertIn(['systemctl', '--user', 'restart', 'halcyon.service'], self.calls)
        self.assertFalse((units / 'cosmic-desktop-cards.service').exists())
        self.assertFalse((launchers / 'io.github.workstation.DesktopCards.desktop').exists())
        # Old shortcuts that run `desktop-cards` keep working.
        shim = bin_dir / 'desktop-cards'
        self.assertTrue(shim.is_symlink())
        self.assertEqual(shim.resolve(), (bin_dir / 'halcyon-ctl').resolve())

    def test_an_upgrade_keeps_the_previous_program(self):
        self.install()
        self.build(b'release two')
        result = self.install()
        self.assertFalse(result['first'])
        self.assertEqual((result['backup'] / 'halcyon').read_bytes(), b'release one')
        self.assertEqual((self.home / '.local/bin/halcyon').read_bytes(), b'release two')

    def test_only_the_newest_two_backups_are_kept(self):
        backups = self.home / '.local/share/halcyon/backups'
        backups.mkdir(parents=True)
        # Earlier upgrades, and a folder that isn't an install backup.
        for stamp in ('20240101T000000Z', '20250101T000000Z', '20260101T000000Z'):
            (backups / f'{stamp}-install').mkdir()
            (backups / f'{stamp}-install' / 'halcyon').write_bytes(b'old')
        (backups / 'notes').mkdir()
        self.install()
        self.build(b'release two')
        result = self.install()
        self.assertEqual(sorted(path.name for path in backups.iterdir()),
                         ['20260101T000000Z-install', result['backup'].name, 'notes'])
        self.assertEqual((result['backup'] / 'halcyon').read_bytes(), b'release one')
        # An install that replaces nothing keeps them as they are.
        self.install()
        self.assertEqual(len(list(backups.iterdir())), 3)

    def test_preferences_and_notes_are_never_touched(self):
        config = self.home / '.config/halcyon/config.json'
        config.parent.mkdir(parents=True)
        config.write_text('{"theme":"teal"}')
        self.install()
        self.assertEqual(config.read_text(), '{"theme":"teal"}')


if __name__ == '__main__':
    unittest.main()
