"""Generation/restore fault tests; actual shipped 0.2.0 installer when provided."""
import hashlib
import importlib.util
import json
import multiprocessing
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).parent))
import install
import refresh
from bundle_contract import canonical, digest
from test_generation_contract import installed, bind_target, bundle_fixture

RELEASE_SHA='1ec258de00228f02d37fa6201bd4ac4050344594cb452ad6d05ada459ea81c5e'
DEFAULT_RELEASE=Path(__file__).parents[1]/'.task-3-evidence/cosmic-ext-halcyon-islands_0.2.0_amd64.deb'


def worker(home, library, queue):
    queue.put(refresh.stage_installed_update(Path(home),Path(library),session='second',fixture=True))


class Refresh(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name);self.home=self.root/'home';self.home.mkdir()
        self.library=self.root/'library';self.manifest,self.catalog=bundle_fixture(self.library)
        (self.home/'installed-identity.json').write_bytes(canonical(installed(self.catalog['cohorts'][0])))
    def enable(self):
        with refresh.locked(self.home):
            result=refresh.stage_locked(self.home,self.library,session='first',fixture=True,enable=True)
        self.assertEqual(result['state'],'staged',result)
        return result
    def stage(self,**kwargs):
        return refresh.stage_installed_update(self.home,self.library,session='second',fixture=True,**kwargs)
    def upgrade(self,version='0.2.2'):
        self.manifest,self.catalog=bundle_fixture(self.library,version)
    def disable(self):
        with refresh.locked(self.home):
            for path,_ in refresh.records(self.home):install.restore(path)
    def test_new_and_disabled_users_never_automatically_enable(self):
        self.assertEqual(self.stage()['state'],'disabled')
        self.assertFalse((self.home/refresh.SELECTOR).exists())
        self.enable();self.disable();self.assertEqual(self.stage()['state'],'disabled')
    def test_generations_idempotency_disable_and_layout(self):
        config=self.home/'.config/cosmic/layout';config.parent.mkdir(parents=True);config.write_bytes(b'private user layout')
        first=self.enable();self.assertEqual(self.stage()['state'],'unchanged')
        self.upgrade();second=self.stage();self.assertEqual(second['state'],'staged')
        self.assertNotEqual(first['generation'],second['generation'])
        self.assertEqual(len(refresh.records(self.home)),2)
        self.disable();self.assertFalse((self.home/refresh.SELECTOR).exists());self.assertFalse((self.home/refresh.POINTER).exists())
        self.assertEqual(config.read_bytes(),b'private user layout')
    def bundled(self, version='0.5.0'):
        self.upgrade(version)
        path = self.library/'companion-manifest.json'
        manifest = json.loads(path.read_bytes())
        manifest['package_owner'] = 'cosmic-ext-halcyon'
        path.write_bytes(canonical(manifest))

    def test_bundled_upgrade_preserves_enabled_layout_and_legacy_restore_chain(self):
        self.enable()
        layout = self.home/'.config/cosmic/layout'
        layout.parent.mkdir(parents=True)
        layout.write_bytes(b'custom panel and dock')
        self.bundled()
        self.assertEqual(self.stage()['state'], 'staged')
        self.assertEqual({r['owner'] for _, r in refresh.records(self.home)},
                         {'cosmic-ext-halcyon', 'cosmic-ext-halcyon-islands'})
        refresh.verify_owned(self.home, refresh.records(self.home))
        self.disable()
        self.assertFalse((self.home/refresh.SELECTOR).exists())
        self.assertEqual(layout.read_bytes(), b'custom panel and dock')
        self.assertEqual(self.stage()['state'], 'disabled')

    def test_bundled_fresh_disabled_and_external_choices_are_preserved(self):
        self.bundled()
        self.assertEqual(self.stage()['state'], 'disabled')
        self.assertFalse((self.home/refresh.SELECTOR).exists())
        self.enable()
        self.bundled('0.5.1')
        selector = self.home/refresh.SELECTOR
        selector.write_bytes(b'custom selector')
        self.assertEqual(self.stage()['state'], 'external_change')
        self.assertEqual(selector.read_bytes(), b'custom selector')

    def test_package_replacement_after_snapshot_preserves_selector_and_restore_chain(self):
        self.enable()
        old_pointer = (self.home/refresh.POINTER).read_bytes()
        self.upgrade()
        intended = (self.library/'selector.py').read_bytes()
        def replace(point):
            if point == 'generation':
                (self.library/'selector.py').write_bytes(b'unrelated concurrent package replacement')
        self.assertEqual(self.stage(boundary=replace)['state'], 'staged')
        target, _, _, _ = refresh.selected_generation(self.home)
        self.assertEqual((self.home/refresh.SELECTOR).read_bytes(), intended)
        self.assertEqual((target/'selector.py').read_bytes(), intended)
        refresh.verify_owned(self.home, refresh.records(self.home))
        install.restore(refresh.records(self.home)[0][0])
        self.assertEqual((self.home/refresh.POINTER).read_bytes(), old_pointer)
        refresh.verify_owned(self.home, refresh.records(self.home))
        self.disable()
        self.assertFalse((self.home/refresh.SELECTOR).exists())
        self.assertFalse((self.home/refresh.POINTER).exists())

    def test_external_edit_preserved(self):
        self.enable();self.upgrade();selector=self.home/refresh.SELECTOR;selector.write_bytes(b'user edit')
        self.assertEqual(self.stage()['state'],'external_change');self.assertEqual(selector.read_bytes(),b'user edit')
        with self.assertRaises(ValueError):self.disable()
    def test_crash_latch_preserved_and_downgrade_refused(self):
        self.enable();failed=self.home/'.local/state/desktop-cards-panel/failed';failed.parent.mkdir(parents=True);failed.write_bytes(b'crash')
        self.upgrade('0.2.0');self.assertEqual(self.stage()['reason'],'non_monotonic_version')
        self.upgrade();self.assertEqual(self.stage()['state'],'staged');self.assertEqual(failed.read_bytes(),b'crash')
        self.disable();self.assertEqual(failed.read_bytes(),b'crash')
    def test_tampered_package_missing_package_and_unknown_cohort(self):
        self.enable();self.upgrade();(self.library/'panel_launcher.py').write_bytes(b'bad')
        self.assertEqual(self.stage()['state'],'blocked')
        bundle_fixture(self.library,'0.2.2');(self.home/'installed-identity.json').write_text('{}')
        self.assertEqual(self.stage()['reason'],'installed_runtime_unqualified')
        self.assertEqual(refresh.stage_installed_update(self.home,self.root/'missing',session='x',fixture=True)['state'],'blocked')
        self.disable()
    def test_interruption_before_each_promotion_boundary(self):
        self.enable();self.upgrade();original=(self.home/refresh.POINTER).read_bytes()
        for boundary in ('copied','generation','before_selection','selected'):
            with self.subTest(boundary=boundary):
                # Vary package version so each iteration really stages.
                self.upgrade('0.2.2+'+str(('copied','generation','before_selection','selected').index(boundary)))
                def fail(point):
                    if point==boundary:raise OSError('power failure')
                self.assertEqual(self.stage(boundary=fail)['state'],'blocked')
                refresh.selected_generation(self.home) # always a whole verified generation
                if boundary!='selected':self.assertEqual((self.home/refresh.POINTER).read_bytes(),original)
                self.assertIn(self.stage()['state'],('staged','unchanged'))
                original=(self.home/refresh.POINTER).read_bytes()
        self.disable()
    def test_hard_exit_between_pointer_and_selector_replays_journal(self):
        self.enable();self.upgrade();previous=(self.home/refresh.POINTER).read_bytes()
        program='''import sys,os
from pathlib import Path
sys.path.insert(0,sys.argv[1])
import refresh,install
old=install.place
def die(path,data,mode):
 old(path,data,mode)
 if path.name=='generation.json':os._exit(91)
install.place=die
refresh.stage_installed_update(Path(sys.argv[2]),Path(sys.argv[3]),session='second',fixture=True)
'''
        result=subprocess.run([sys.executable,'-c',program,str(Path(__file__).parent),str(self.home),str(self.library)])
        self.assertEqual(result.returncode,91)
        self.assertFalse(refresh.records(self.home)[0][1]['complete'])
        self.assertEqual(self.stage()['state'],'staged')
        self.disable();self.assertFalse((self.home/refresh.SELECTOR).exists())
    def test_concurrent_staging_and_independent_users(self):
        self.enable();self.upgrade();queue=multiprocessing.Queue()
        children=[multiprocessing.Process(target=worker,args=(str(self.home),str(self.library),queue)) for _ in range(2)]
        for child in children:child.start()
        for child in children:child.join(10);self.assertFalse(child.is_alive())
        results=[queue.get(timeout=2)['state'] for _ in children]
        self.assertIn('staged',results);self.assertEqual(len(refresh.records(self.home)),2)
        other=self.root/'other';other.mkdir()
        self.assertEqual(refresh.stage_installed_update(other,self.library,session='x',fixture=True)['state'],'disabled')
        self.disable()
    def test_nondefault_data_home_blocks_production_before_any_write(self):
        outside=self.root/'outside'
        with patch.dict(os.environ,{'XDG_DATA_HOME':str(outside)}):
            result=refresh.stage_installed_update(self.home,self.library,session='x')
        self.assertEqual(result['reason'],'unsupported_data_location')
        self.assertFalse(outside.exists())
        self.assertFalse((self.home/refresh.SELECTOR).exists())

    def test_symlink_escape_and_changed_selected_generation(self):
        self.enable();path,_,_,_=refresh.selected_generation(self.home)
        (path/'panel_launcher.py').write_bytes(b'changed')
        self.upgrade();self.assertEqual(self.stage()['state'],'blocked')
        self.disable() # generation damage never destroys ownership rollback


class Actual020(Refresh):
    """Inherits lifecycle assertions, starting with the public package installer."""
    def enable(self):
        package=Path(os.environ.get('HALCYON_RELEASED_020',str(DEFAULT_RELEASE)))
        if not package.is_file():self.skipTest('Actual 0.2.0 package required; set HALCYON_RELEASED_020 (see docs/COMPATIBILITY.md)')
        self.assertEqual(digest(package),RELEASE_SHA)
        extracted=self.root/'released'
        subprocess.run(['dpkg-deb','-x',str(package),str(extracted)],check=True)
        library=extracted/'usr/lib/cosmic-ext-halcyon-islands'
        # Execute the actual shipped installer, which copies its actual shipped
        # launcher and actual released ELF. No new implementation approximates it.
        env={**os.environ,'XDG_SESSION_ID':'first','HOME':str(self.home)}
        result=subprocess.run([sys.executable,'-B',str(library/'install.py'),'--root',str(self.home),
            '--binary',str(library/'cosmic-panel-islands'),'--manifest',str(library/'companion-manifest.json'),
            '--enable-next-login','--record-owner',refresh.PACKAGE],env=env,capture_output=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual((self.home/refresh.SELECTOR).read_bytes(),(library/'panel_launcher.py').read_bytes())
        legacy=self.home/'.local/share/cosmic-desktop-cards/companion/cosmic-panel-islands'
        self.assertEqual(digest(legacy),'f4dca9b2d6fff12c37657b4dfc8c8fb7de669fb17bb409153011df156e54dc31')
        migration=self.stage();self.assertEqual(migration['state'],'staged',migration)
        self.assertEqual(len(refresh.records(self.home)),2)
        return migration
    # Record-count assertions account for the immutable original install layer.
    def test_generations_idempotency_disable_and_layout(self):
        self.enable();self.upgrade();self.assertEqual(self.stage()['state'],'staged')
        self.upgrade('0.2.3');self.assertEqual(self.stage()['state'],'staged')
        self.assertEqual(len(refresh.records(self.home)),4)
        self.disable();self.assertFalse((self.home/refresh.SELECTOR).exists())
        self.assertFalse((self.home/'.local/share/cosmic-desktop-cards/companion/cosmic-panel-islands').exists())
    def test_concurrent_staging_and_independent_users(self):
        self.enable();self.upgrade()
        with refresh.locked(self.home):self.assertEqual(self.stage()['reason'],'busy')
        self.assertEqual(self.stage()['state'],'staged');self.disable()

if __name__=='__main__':unittest.main()
