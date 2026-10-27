"""Actual launcher + real ELF/procfs lifecycle. NOT native login qualification."""
import copy
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest

from bundle_contract import CONFIGURATION_SOURCE, canonical, cohort_id, digest, identity, runtime_id, valid_catalog
import install
import refresh
import runtime_identity
from test_generation_contract import installed, bind_target, bundle_fixture, catalog_fixture

HERE = Path(__file__).resolve().parent

RUNNER = r'''
import ctypes,json,os,signal,subprocess,sys,threading,time
from pathlib import Path
sys.path.insert(0,sys.argv[1])
import panel_launcher as launcher
import runtime_identity as runtime
import panel_configuration
# Synthetic compositor output inventory only; real config/desktop IO and inotify.
panel_configuration.output_names=lambda env=None:['fixture-output']
import install_status
from bundle_contract import classify_compatibility
cfg=json.loads(Path(sys.argv[2]).read_bytes())
def publish(path, value):
    path=Path(path);pending=path.with_name(path.name+'.tmp')
    pending.write_text(json.dumps(value));pending.replace(path)
# Test-only subreaper: collect synthetic orphan grandchildren after killpg.
ctypes.CDLL(None).prctl(36,1,0,0,0)
real_installed=runtime.installed_identity
def fixture_installed(catalog, *, measurement=None):
    if measurement is not None: measurement.root=Path(cfg['installed_root']).resolve()
    return real_installed(catalog,Path(cfg['installed_root']),cfg['versions'],cfg['system'],measurement=measurement)
runtime.installed_identity=fixture_installed
launcher.STOCK=Path(cfg['stock']);launcher.READY_SECONDS=1.2
if cfg['mode']=='slow-measurement':
    measured_runtime=runtime.running_identity
    def slow_runtime(*args,**kwargs):
        if kwargs.get('phase')=='running' and kwargs.get('configuration') is not None:time.sleep(10)
        return measured_runtime(*args,**kwargs)
    runtime.running_identity=slow_runtime
real_wait=launcher.wait_ready
foreign=[]
def observed_wait(child,done,wedged,admission=None):
    if cfg['mode']=='foreign':
        # An unrelated same-session actor cannot substitute for the host's child.
        foreign.append(subprocess.Popen([cfg['applet'],'90']))
    owner=runtime.host_identity(cfg['binary_sha256']);deadline=time.monotonic()+1
    while owner is None and time.monotonic()<deadline:
        time.sleep(.005);owner=runtime.host_identity(cfg['binary_sha256'])
    if cfg['mode']=='core-exit':os.kill(cfg['core_pid'],signal.SIGTERM)
    before=runtime.running_identity(cfg['catalog'],host=owner)
    publish(cfg['observations']+'-early', {'ipc_owner_verified':owner is not None,'runtime':before,'monotonic_ns':time.monotonic_ns()})
    if cfg['mode'] in ('config-change','config-aba'):
        config=Path(cfg['config_center']);old=config.read_bytes()
        config.write_text('Some([])')
        if cfg['mode']=='config-aba':config.write_bytes(old)
    if cfg['mode'] in ('sidecar-change','sidecar-aba'):
        path=Path(cfg['sidecar']);old=path.read_bytes()
        path.write_bytes(old.replace(b'enabled:true',b'enabled:false'))
        if cfg['mode']=='sidecar-aba':path.write_bytes(old)
    Path(os.environ['FIXTURE_ALLOW_SPAWN']).touch()
    ready=real_wait(child,done,wedged,admission)
    if ready:
        owner=runtime.host_identity(cfg['binary_sha256'])
        measured=runtime.running_identity(cfg['catalog'],host=owner)
        state=install_status.detect(Path(cfg['home']),Path(cfg['library']),True,cfg['versions'],
            installed=runtime.installed_identity(cfg['catalog']),running=measured,host=owner is not None)
        publish(cfg['observations'], {'ready':ready,'runtime':measured,'status':state,'pid':child.pid,'monotonic_ns':time.monotonic_ns()})
    else:
        publish(cfg['observations'], {'ready':False,'monotonic_ns':time.monotonic_ns()})
    return ready
launcher.wait_ready=observed_wait
try:
    result=launcher.main(bundle=Path(cfg['bundle']),pointer=cfg['pointer'])
    publish(cfg['exit'], {'result':result,'foreign_survived':[p.poll() is None for p in foreign]})
finally:
    for child in foreign:
        if child.poll() is None:child.terminate()
        child.wait()
    deadline=time.monotonic()+2
    while time.monotonic()<deadline:
        try:
            pid,_=os.waitpid(-1,os.WNOHANG)
            if pid==0:time.sleep(.01)
        except ChildProcessError:break
sys.exit(result)
'''


class LaunchLifecycle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.build=tempfile.TemporaryDirectory()
        cls.binary=Path(cls.build.name)/'host'
        subprocess.run(['cc','-O2','-Wall','-Wextra','-Werror',str(HERE/'fixtures/lifecycle_host.c'),'-o',str(cls.binary)],check=True)
        cls.binary.chmod(0o755)

    @classmethod
    def tearDownClass(cls):
        cls.build.cleanup()

    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name);self.home=self.root/'home';self.home.mkdir()
        self.installed_root=self.root/'installed';(self.installed_root/'usr/bin').mkdir(parents=True)
        self.runtime=self.root/'runtime';self.runtime.mkdir()
        self.events=self.root/'events';self.events.touch()
        self.env={**os.environ,'HOME':str(self.home),'XDG_DATA_HOME':str(self.home/'.local/share'),
            'XDG_STATE_HOME':str(self.home/'.local/state'),'XDG_CONFIG_HOME':str(self.home/'.config'),
            'XDG_RUNTIME_DIR':str(self.runtime),'XDG_SESSION_ID':'lifecycle-'+self.root.name,
            'WAYLAND_DISPLAY':'fixture-compositor','FIXTURE_EVENTS':str(self.events),'FIXTURE_ALLOW_SPAWN':str(self.root/'allow-spawn')}
        # No inherited real session descriptors can reach the synthetic test.
        for name in ('WAYLAND_SOCKET','PANEL_NOTIFICATIONS_FD','COSMIC_SESSION_SOCK'):
            self.env.pop(name,None)
        self.display=socket.socket(socket.AF_UNIX);self.display.bind(str(self.runtime/'fixture-compositor'));self.display.listen(16)
        self.addCleanup(self.display.close)
        self.children=[];self.runners=[];self.addCleanup(self.cleanup)
        self.catalog=catalog_fixture();self.row=self.catalog['cohorts'][0]
        self.row['runtime']['processes']={}
        for name in ('cosmic-comp','cosmic-session','cosmic-settings-daemon','cosmic-notifications','cosmic-applet-audio'):
            path=self.installed_root/'usr/bin'/name;shutil.copy2('/usr/bin/sleep',path)
            child=self.start_actor(path)
            observed=child.fixture_identity
            self.row['runtime']['processes'][name]={**{k:observed[k] for k in ('executable','sha256','mapped')},
                'count':'configured' if name=='cosmic-applet-audio' else 1,'lifecycle':'panel-child' if name=='cosmic-applet-audio' else 'session'}
            self.row['packages'][name]=dict(version='fixture-1',sha256='b'*64,files={'/usr/bin/'+name:digest(path)})
            if name=='cosmic-applet-audio':child.terminate();child.wait()
        # The sample actor used to record maps is gone before any launcher call.
        self.applet=self.installed_root/'usr/bin/cosmic-applet-audio'
        self.stock=self.root/'stock';shutil.copy2(self.binary,self.stock)
        stock_installed=self.installed_root/'usr/bin/cosmic-panel';shutil.copy2(self.binary,stock_installed)
        self.row['packages']['cosmic-panel']=dict(version='fixture-1',sha256='b'*64,files={'/usr/bin/cosmic-panel':digest(stock_installed)})
        self.other=self.root/'other/cosmic-applet-audio';self.other.parent.mkdir();self.other.write_bytes(self.applet.read_bytes()+b'unknown code');self.other.chmod(0o755)
        self.replacement=self.root/'replacement';shutil.copy2(self.other,self.replacement)
        self.library=self.root/'library'
        self.manifest,_=bundle_fixture(self.library,'0.2.1~lifecycle')
        shutil.copy2(self.binary,self.library/'cosmic-panel-islands')
        self.row['runtime']['configuration']=dict(native_source=CONFIGURATION_SOURCE,resolver='cosmic-panel-ca20ce3-islands-v2',max_outputs=1,
            applets={'com.test.Audio':dict(process='cosmic-applet-audio',args=['90'])})
        self.env['XDG_DATA_DIRS']=str(self.root/'system-data')
        self.config_center=self.home/'.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_center'
        self.configure(1)
        desktop=self.home/'.local/share/applications/com.test.Audio.desktop';desktop.parent.mkdir(parents=True,exist_ok=True)
        desktop.write_text('[Desktop Entry]\nName=Audio\nExec='+str(self.applet)+' 90\n')
        self.finalize()

    def configure(self,count):
        root=self.home/'.config/cosmic';entries=['Panel']+(['Dock'] if count==2 else [])
        container=root/'com.system76.CosmicPanel/v1';container.mkdir(parents=True,exist_ok=True)
        (container/'entries').write_text(json.dumps(entries))
        for name in entries:
            bar=root/('com.system76.CosmicPanel.'+name)/'v1';bar.mkdir(parents=True,exist_ok=True)
            (bar/'name').write_text(json.dumps(name));(bar/'output').write_text('All')
            (bar/'plugins_wings').write_text('None')
            (bar/'plugins_center').write_text('Some('+json.dumps(['com.test.Audio'] if count else [])+')')

    def tearDown(self):
        evidence=os.environ.get('HALCYON_LIFECYCLE_EVIDENCE')
        if evidence:
            destination=Path(evidence)/self._testMethodName
            destination.mkdir(parents=True,exist_ok=False)
            for name in ('events','observations','observations-early','runner.log','exit','runner.json'):
                if (self.root/name).exists():shutil.copy2(self.root/name,destination/name)
            for path in self.root.glob('layout-*'):shutil.copytree(path,destination/path.name)
            (destination/'NOT-NATIVE.json').write_bytes(canonical(dict(kind='synthetic real-procfs and actual launcher regression',native_qualification=False)))

    def cleanup(self):
        for runner in self.runners:
            if runner.poll() is None:runner.terminate()
            try:runner.wait(timeout=4)
            except subprocess.TimeoutExpired:runner.kill();runner.wait()
        for child in self.children:
            if child.poll() is None:child.terminate()
            child.wait()

    def start_actor(self,path):
        child=subprocess.Popen([str(path),'90'],env=self.env);self.children.append(child)
        deadline=time.monotonic()+2
        while time.monotonic()<deadline:
            try:
                process=Path('/proc')/str(child.pid)
                # Exec alone is too early: the dynamic loader may still alter
                # mappings. Wait for the sleep actor to block, then retain the
                # bounded, internally consistent measurement already obtained.
                if (os.readlink(process/'exe')==str(path)
                        and (process/'stat').read_text().rpartition(') ')[2].split()[0]=='S'):
                    child.fixture_identity=runtime_identity._process(process)
                    return child
            except (OSError,ValueError):pass
            time.sleep(.002)
        self.fail('fixture actor failed to exec')

    def finalize(self):
        self.row['processes']={n:p['executable'] for n,p in self.row['runtime']['processes'].items()}
        self.row['runtime']['host_mapped']={p:s for v in self.row['runtime']['processes'].values() for p,s in v['mapped'].items() if p.startswith(('/usr/lib/','/lib/','/lib64/'))}
        bind_target(self.row,self.installed_root)
        self.row['runtime_compatibility']=[dict(id=runtime_id(self.row['runtime']),evidence_id=self.row['evidence']['id'],reason='Synthetic lifecycle fixture only, not native evidence')]
        files=self.manifest['files'];files['cosmic-panel-islands']['sha256']=digest(self.library/'cosmic-panel-islands')
        self.manifest['binary_sha256']=files['cosmic-panel-islands']['sha256']
        self.manifest['payload_sha256']=identity({k:v for k,v in files.items() if k!='catalog.json'})
        self.row['payload_sha256']=self.manifest['payload_sha256'];bind_target(self.row,self.installed_root);valid_catalog(self.catalog)
        (self.library/'catalog.json').write_bytes(canonical(self.catalog));files['catalog.json']['sha256']=digest(self.library/'catalog.json')
        (self.library/'companion-manifest.json').write_bytes(canonical(self.manifest));(self.library/'companion-manifest.json').chmod(0o644)
        self.versions={n:p['version'] for n,p in self.row['packages'].items()}
        self.system={k:self.row[k] for k in ('distribution','suite','architecture')}

    def launch(self,mode='success'):
        (self.home/'installed-identity.json').write_bytes(canonical(installed(self.row)))
        with refresh.locked(self.home):
            staged=refresh.stage_locked(self.home,self.library,fixture=True,enable=True,session='previous-login')
        self.assertIn(staged['state'],('staged','unchanged'));self.assertEqual(staged['runtime_readiness'],'not_measured')
        bundle,_,_,pointer=refresh.selected_generation(self.home)
        cfg=dict(mode=mode,sidecar=str(self.home/'.config/cosmic/io.github.desktopcards.DockIslands/v1/Panel'),config_center=str(self.config_center),core_pid=self.children[0].pid,installed_root=str(self.installed_root),versions=self.versions,system=self.system,
            stock=str(self.stock),catalog=self.catalog,home=str(self.home),library=str(self.library),bundle=str(bundle),pointer=pointer,
            binary_sha256=self.manifest['binary_sha256'],applet=str(self.applet),observations=str(self.root/'observations'),exit=str(self.root/'exit'))
        config=self.root/'runner.json';config.write_bytes(canonical(cfg));runner=self.root/'runner.py';runner.write_text(RUNNER)
        self.log=(self.root/'runner.log').open('wb');self.addCleanup(self.log.close)
        env={**self.env,'FIXTURE_MODE':mode,'FIXTURE_APPLET':str(self.applet),'FIXTURE_OTHER_APPLET':str(self.other),'FIXTURE_REPLACEMENT':str(self.replacement)}
        child=subprocess.Popen([sys.executable,str(runner),str(HERE),str(config)],env=env,stdout=self.log,stderr=subprocess.STDOUT)
        self.runners.append(child);return child

    def await_file(self,name,child):
        deadline=time.monotonic()+6
        while time.monotonic()<deadline:
            path=self.root/name
            if path.exists():return json.loads(path.read_bytes())
            if child.poll() is not None:break
            time.sleep(.01)
        self.fail((self.root/'runner.log').read_text())

    def events_named(self,name):
        return [int(line.split()[1]) for line in self.events.read_text().splitlines() if line.split()[0]==name]

    def assert_reaped(self):
        for pid in self.events_named('host')+self.events_named('applet'):
            self.assertFalse((Path('/proc')/str(pid)).exists(),f'fixture PID {pid} not reaped')

    def test_core_only_login_spawns_owned_applet_then_ready_and_clean_teardown(self):
        self.successful_login()

    def successful_login(self,mode='success',count=1):
        from unittest.mock import patch
        with patch.dict(os.environ,self.env), patch('panel_configuration.output_names',return_value=['fixture-output']):
            pre=runtime_identity.running_identity(self.catalog,session=self.env['XDG_SESSION_ID'],phase='prelaunch')
        self.assertTrue(pre['verified']);self.assertEqual(pre['phase'],'prelaunch')
        child=self.launch(mode);observed=self.await_file('observations',child)
        self.assertTrue(observed['ready'],(self.root/'runner.log').read_text())
        early=json.loads((self.root/'observations-early').read_bytes())
        self.assertTrue(early['ipc_owner_verified']);self.assertEqual(early['runtime']['verified'],count==0)
        self.assertEqual(observed['status']['status'],'Ready')
        self.assertEqual(observed['runtime']['phase'],'running')
        self.assertEqual(len(self.events_named('host')),1);self.assertEqual(len(self.events_named('applet')),count)
        self.assertEqual(len(self.events_named('probe')),1)
        for line in self.events.read_text().splitlines():
            if line.startswith('applet '):
                self.assertLess(early['monotonic_ns'],int(line.split()[2]))
                self.assertLess(int(line.split()[2]),observed['monotonic_ns'])
        # An arbitrary prelaunch result cannot become running readiness in status.
        import install_status
        from unittest.mock import patch
        with patch.dict(os.environ,self.env):
            status=install_status.detect(self.home,self.library,True,self.versions,installed=installed(self.row),running=pre,host=True)
        self.assertNotEqual(status['status'],'Ready');self.assertEqual(status['running_profiles'],[])
        child.terminate();self.assertEqual(child.wait(timeout=5),0)
        self.assertEqual(self.events_named('stock'),[])
        self.assertFalse((self.home/'.local/state/desktop-cards-panel/failed').exists())
        self.assert_reaped()
        # Disable keeps the existing guarded restore chain.
        with refresh.locked(self.home):
            for path,_ in refresh.records(self.home):install.restore(path)
        self.assertFalse((self.home/refresh.SELECTOR).exists())

    def test_missing_required_child_times_out_before_ready(self):
        self.failure('missing')

    def failure(self,mode):
        child=self.launch(mode);self.assertEqual(child.wait(timeout=6),0,(self.root/'runner.log').read_text())
        self.assertFalse(json.loads((self.root/'observations').read_bytes())['ready'])
        self.assertEqual(len(self.events_named('host')),1);self.assertEqual(len(self.events_named('stock')),1)
        self.assertTrue((self.home/'.local/state/desktop-cards-panel/failed').exists())
        self.assert_reaped()
        if mode=='foreign':self.assertEqual(json.loads((self.root/'exit').read_bytes())['foreign_survived'],[True])

    def test_declared_two_children_can_become_ready(self):
        self.configure(2)
        self.successful_login('excess',2)

    def test_unconfigured_zero_children_is_ready_without_repacking(self):
        self.configure(0);self.successful_login('missing',0)

    def test_same_artifact_zero_one_two_layouts(self):
        original=(self.library/'companion-manifest.json').read_bytes()
        for count,mode in ((0,'missing'),(1,'success'),(2,'excess')):
            self.configure(count)
            self.successful_login(mode,count)
            self.assertEqual((self.library/'companion-manifest.json').read_bytes(),original)
            for name in ('events','observations','observations-early','exit'):
                path=self.root/name
                if path.exists():
                    capture=self.root/('layout-'+str(count));capture.mkdir(exist_ok=True)
                    shutil.copy2(path,capture/name)
                    path.unlink()
            self.events.touch()

    def test_zero_configuration_still_blocks_old_existing_actor(self):
        self.configure(0);foreign=self.start_actor(self.applet);self.blocked();self.assertIsNone(foreign.poll())

    def test_config_change_does_not_persist_crash_latch(self):
        self.config_race('config-change')

    def test_config_aba_session_fallback_and_next_login_retry(self):
        self.config_race('config-aba')
        # Same session selects stock without a second host or notification reuse.
        child=self.launch();self.assertEqual(child.wait(timeout=6),0)
        self.assertEqual(len(self.events_named('host')),1);self.assertEqual(len(self.events_named('stock')),2)
        # New login is eligible without clearing an enduring crash marker.
        for child in self.children[:4]:child.terminate();child.wait()
        self.env['XDG_SESSION_ID']+='-next-login'
        for name in ('cosmic-comp','cosmic-session','cosmic-settings-daemon','cosmic-notifications'):
            self.start_actor(self.installed_root/'usr/bin'/name)
        for name in ('observations','observations-early','exit','allow-spawn'):
            path=self.root/name
            if path.exists():path.unlink()
        self.events.write_text('')
        self.successful_login()

    def config_race(self,mode):
        child=self.launch(mode);self.assertEqual(child.wait(timeout=6),0,(self.root/'runner.log').read_text())
        self.assertFalse(json.loads((self.root/'observations').read_bytes())['ready'])
        self.assertEqual(len(self.events_named('stock')),1);self.assert_reaped()
        state=self.home/'.local/state/desktop-cards-panel'
        self.assertFalse((state/'failed').exists());self.assertTrue((state/'fallback-session').is_file())
        from panel_launcher import session_identity
        self.assertEqual((state/'fallback-session').read_text(),session_identity(self.env))

    def test_crash_after_ready_and_layout_edit_still_persists_crash_latch(self):
        child=self.launch();observed=self.await_file('observations',child);self.assertTrue(observed['ready'])
        self.config_center.write_text('Some([])')
        os.kill(observed['pid'],signal.SIGKILL)
        self.assertEqual(child.wait(timeout=6),0)
        self.assertTrue((self.home/'.local/state/desktop-cards-panel/failed').exists())
        self.assertEqual(len(self.events_named('stock')),1);self.assert_reaped()

    def configure_named_fallback(self, enabled=True, fixture=None):
        from test_islands_configuration import FIXTURES, settings
        self.config_center.with_name('output').write_text('Name("DP-9")')
        sidecar=self.home/'.config/cosmic/io.github.desktopcards.DockIslands/v1/Panel'
        sidecar.parent.mkdir(parents=True,exist_ok=True)
        sidecar.write_text((FIXTURES/fixture).read_text() if fixture else settings(enabled))

    def test_enabled_islands_absent_name_admits_actual_fallback_child(self):
        self.configure_named_fallback();self.successful_login()

    def test_disabled_islands_absent_name_remains_zero_native_children(self):
        self.configure_named_fallback(False);self.successful_login('missing',0)

    def test_enabled_fallback_still_requires_configured_child(self):
        self.configure_named_fallback();self.failure('missing')

    def test_enabled_fallback_child_present_before_readiness_is_ready(self):
        from unittest.mock import patch
        self.configure_named_fallback()
        # Reproduce the independent review's schedule with the same real loop.
        runner=RUNNER.replace('ready=real_wait(child,done,wedged,admission)',
                              'time.sleep(.5)\n    ready=real_wait(child,done,wedged,admission)')
        with patch(__name__+'.RUNNER',runner):self.successful_login()

    def test_saved_output_pretty_settings_require_actual_child(self):
        self.configure_named_fallback(fixture='outputs-enabled-pretty.ron');self.successful_login()

    def test_saved_output_pretty_missing_child_fails(self):
        self.configure_named_fallback(fixture='outputs-enabled-pretty.ron');self.failure('missing')

    def test_native_invalid_list_absent_name_is_disabled(self):
        self.configure_named_fallback(fixture='invalid-order-list.ron');self.successful_login('missing',0)

    def test_enabled_all_keeps_native_child_requirement(self):
        self.configure_named_fallback();self.config_center.with_name('output').write_text('All')
        self.successful_login()

    def test_disabled_active_keeps_native_child_requirement(self):
        self.configure_named_fallback(False);self.config_center.with_name('output').write_text('Active')
        self.successful_login()

    def test_disabled_present_name_keeps_native_child_requirement(self):
        self.configure_named_fallback(False)
        self.config_center.with_name('output').write_text('Name("fixture-output")')
        self.successful_login()

    def test_enabled_fallback_rejects_foreign_child(self):
        self.configure_named_fallback();self.failure('foreign')

    def test_enabled_fallback_rejects_excess_children(self):
        self.configure_named_fallback();self.failure('excess')

    def test_sidecar_change_during_fallback_startup_uses_session_latch(self):
        self.configure_named_fallback();self.config_race('sidecar-change')

    def test_sidecar_aba_during_fallback_startup_uses_session_latch(self):
        self.configure_named_fallback();self.config_race('sidecar-aba')

    def test_required_core_exit_after_launch_is_not_ignored(self):self.failure('core-exit')

    def test_slow_runtime_measurement_cannot_extend_readiness_deadline(self):
        started=time.monotonic();self.failure('slow-measurement')
        self.assertLess(time.monotonic()-started,4)

    def test_mismatched_child(self):self.failure('mismatch')
    def test_package_replaced_between_prelaunch_and_spawn(self):self.failure('replace')
    def test_wrong_session_child(self):self.failure('wrong-session')
    def test_excess_child_count(self):self.failure('excess')
    def test_matching_unowned_applet_cannot_supply_readiness(self):self.failure('foreign')

    def blocked(self):
        child=self.launch();self.assertEqual(child.wait(timeout=5),0,(self.root/'runner.log').read_text())
        self.assertEqual(self.events_named('host'),[]);self.assertEqual(self.events_named('probe'),[])
        self.assertEqual(len(self.events_named('stock')),1)
        self.assertFalse((self.home/'.local/state/desktop-cards-panel/failed').exists())

    def test_leftover_matching_applet_blocks_before_starting_any_host(self):
        foreign=self.start_actor(self.applet);self.blocked();self.assertIsNone(foreign.poll())

    def test_old_running_applet_with_new_installed_bytes_blocks_prelaunch(self):
        foreign=self.start_actor(self.applet)
        os.replace(self.replacement,self.applet)
        self.row['runtime']['processes']['cosmic-applet-audio']['sha256']=digest(self.applet)
        self.row['runtime']['processes']['cosmic-applet-audio']['mapped'][str(self.applet)]=digest(self.applet)
        self.row['packages']['cosmic-applet-audio']['files']['/usr/bin/cosmic-applet-audio']=digest(self.applet)
        self.finalize();self.blocked();self.assertIsNone(foreign.poll())

    def test_installed_applet_bytes_mismatch_blocks_prelaunch(self):
        os.replace(self.replacement,self.applet);self.blocked()

    def test_missing_required_core_is_not_prelaunch_eligibility(self):
        self.children[0].terminate();self.children[0].wait();self.blocked()

    def test_schema_rejects_panel_cycle_and_core_only_equivalence(self):
        bad=copy.deepcopy(self.catalog);row=bad['cohorts'][0]
        row['runtime']['processes']['cosmic-panel-islands']=row['runtime']['processes'].pop('cosmic-applet-audio')
        with self.assertRaisesRegex(ValueError,'host_is_not_a_dependency'):valid_catalog(bad)
        bad=copy.deepcopy(self.catalog);other=copy.deepcopy(bad['cohorts'][0])
        other['runtime']['processes'].pop('cosmic-applet-audio');other['runtime']['configuration']['applets']={};other['processes'].pop('cosmic-applet-audio')
        other['id']=cohort_id(other);other['runtime_compatibility'][0]['id']=runtime_id(other['runtime']);bad['cohorts'].append(other)
        bad['cohorts'][0]['runtime_compatibility'].append(other['runtime_compatibility'][0])
        bind_target(other,self.installed_root);bind_target(bad['cohorts'][0],self.installed_root)
        with self.assertRaisesRegex(ValueError,'equivalence_lifecycle'):valid_catalog(bad)

if __name__=='__main__':unittest.main()
