"""Disposable real-procfs policy tests, NOT physical/native qualification."""
import copy
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

import runtime_compatibility as c
from runtime_identity import _stat
import test_panel_configuration as config_fixture

VERSIONS={'cosmic-panel':'1.11.2~new-build','cosmic-session':'1.11.0~other-build',
          'cosmic-comp':'0.1~new-source','cosmic-settings-daemon':'1.11.0~rebuild'}

class Policy(unittest.TestCase):
    def test_catalog_schema_matches_current_startup_policy(self):
        schema=json.loads((Path(__file__).parent/'compatibility/schema.json').read_text())
        self.assertEqual(schema['properties']['startup_policy']['const'], c.POLICY)

    def test_release_catalog_keeps_authoritative_policy_and_preserves_history(self):
        from bundle_contract import release_catalog, read_catalog, canonical
        source=read_catalog(Path(__file__).parent/'compatibility/catalog.json')
        before=canonical(source)
        projected=release_catalog(source)
        self.assertEqual(projected['startup_policy'], c.POLICY)
        self.assertEqual(projected['cohorts'], [])
        self.assertLess(len(canonical(projected)), 2048)
        self.assertEqual(canonical(source), before)
        self.assertTrue(source['cohorts'])
        legacy=json.loads(before);legacy.pop('startup_policy')
        self.assertEqual(release_catalog(legacy), legacy)

    def test_release_catalog_refuses_unreviewed_policy_before_projection(self):
        from bundle_contract import release_catalog
        catalog=dict(format='halcyon-islands-catalog',version=2,cohorts=[],startup_policy=copy.deepcopy(c.POLICY))
        # Use the real catalog format to exercise policy validation itself.
        from bundle_contract import CATALOG
        catalog['format']=CATALOG
        catalog['startup_policy']['families'].append('1.12')
        with self.assertRaisesRegex(ValueError,'unsupported_startup_policy'):
            release_catalog(catalog)

    def test_release_families_not_vm_device_or_package_attestation(self):
        for gpu in ('AMD','Intel','NVIDIA','virtio'):
            for release in ('1.10','1.11'):
                versions={**VERSIONS,'cosmic-panel':release+'.9~future-rebuild', 'cosmic-session':release+'.0~other-rebuild'}
                # These unrelated variables are never consulted by the policy.
                with patch('runtime_identity.drm_device_snapshot',side_effect=AssertionError('no GPU attestation')), patch('runtime_identity.installed_identity',side_effect=AssertionError('no archive audit')), patch('platform.release',return_value=gpu+'-different-kernel'):
                    self.assertEqual(c.family(versions)['family'],release)
                    self.assertIsNone(c.state(c.family(versions))['cohort'])
                    self.assertNotEqual(c.state(c.family(versions))['qualification'],'native-session')
        for value in ('1.9.0','1.12.0','2.0.0','garbage','1.110.0'):
            with self.assertRaises(ValueError):c.family({**VERSIONS,'cosmic-panel':value})
        with self.assertRaises(ValueError):c.family({**VERSIONS,'cosmic-session':'1.10.0'})
    def test_unknown_source_and_interface_never_inherit_family_permission(self):
        m=dict(upstream=c.CONFIGURATION_SOURCE['upstream'],binary_sha256=c.POLICY['binary_sha256'],runtime_probe=1,protocol=1)
        catalog=dict(startup_policy=copy.deepcopy(c.POLICY))
        c.validate_bundle(m,catalog)
        for key,value in [('upstream','f'*40),('binary_sha256','0'*64),('runtime_probe',0),('protocol',2)]:
            with self.assertRaises(ValueError):c.validate_bundle({**m,key:value},catalog)
        catalog['startup_policy']['families'].append('2.0')
        with self.assertRaises(ValueError):c.enabled(catalog)
    def test_real_package_query_is_one_bounded_call(self):
        output=''.join(n+'\tinstall ok installed\t'+v+'\n' for n,v in VERSIONS.items())
        with patch.object(c.subprocess,'run',return_value=subprocess.CompletedProcess([],0,output,'')) as run:
            self.assertEqual(c.family()['family'],'1.11');self.assertEqual(run.call_count,1)
            self.assertEqual(run.call_args.kwargs['timeout'],1)

    def test_held_packages_remain_installed_without_accepting_partial_states(self):
        from bundle_contract import installed_package_status
        for status in ('install ok installed', 'hold ok installed'):
            output=''.join(n+'\t'+status+'\t'+v+'\n' for n,v in VERSIONS.items())
            with self.subTest(status=status), patch.object(c.subprocess,'run',return_value=subprocess.CompletedProcess([],0,output,'')):
                self.assertEqual(c.family()['family'],'1.11')
        for status in ('hold ok unpacked', 'hold reinstreq installed', 'deinstall ok config-files', 'ok installed', None):
            with self.subTest(status=status):
                self.assertFalse(installed_package_status(status))
                if status is not None:
                    output=''.join(n+'\t'+status+'\t'+v+'\n' for n,v in VERSIONS.items())
                    with patch.object(c.subprocess,'run',return_value=subprocess.CompletedProcess([],0,output,'')):
                        with self.assertRaisesRegex(ValueError,'family_packages_incomplete'):c.family()

class Processes(config_fixture.Configuration):
    # Reuse the established config fixture, but only collect our new lifecycle
    # cases (the original suite is run once separately).
    def setUp(self):
        super().setUp()
        self.env['XDG_SESSION_ID']='halcyon-policy-fixture-'+str(os.getpid())
        self.bin=self.root/'usr/bin';self.bin.mkdir(parents=True)
        self.actors=[];self.sessions=[];self.groups=[]
        self.addCleanup(self.cleanup_actors)
        for name in (*c.CORE,'cosmic-applets'):
            shutil.copy2('/usr/bin/sleep',self.bin/name)
        (self.bin/'cosmic-applet-audio').symlink_to('cosmic-applets')
        self.env['PATH']=str(self.bin)
        self.write(self.data/'applications/com.system76.CosmicAppletAudio.desktop','[Desktop Entry]\nExec=cosmic-applet-audio\n')
        self.setbar('Panel',['com.system76.CosmicAppletAudio'])
        for name in c.CORE:self.actors.append(subprocess.Popen([str(self.bin/name),'60'],env=self.env))
    def cleanup_actors(self):
        for s in self.sessions:s.close()
        for g in self.groups:
            try:os.killpg(g.pid,signal.SIGKILL)
            except ProcessLookupError:pass
            g.wait(timeout=2)
        for p in self.actors:
            p.kill();p.wait(timeout=2)
    def observation(self):
        s=c.Session(VERSIONS,self.env,self.root);self.sessions.append(s);return s
    def host(self,count=1,foreign=False):
        ready=self.root/('ready'+str(len(self.groups)))
        ready.unlink(missing_ok=True)
        script='import subprocess,time,pathlib; children=[subprocess.Popen([EXE,"60"]) for _ in range(COUNT)];pathlib.Path(READY).write_text("ok");time.sleep(60)'
        script=script.replace('EXE',repr(str(self.bin/'cosmic-applets'))).replace('COUNT',str(count)).replace('READY',repr(str(ready)))
        p=subprocess.Popen([os.sys.executable,'-B','-c',script],env=self.env,start_new_session=True);self.groups.append(p)
        deadline=time.monotonic()+2
        while not ready.exists() and time.monotonic()<deadline:time.sleep(.01)
        self.assertTrue(ready.exists())
        return dict(pid=p.pid,**_stat(Path('/proc')/str(p.pid)))
    def test_real_core_then_owned_child_succeeds_zero_and_multiple_layouts(self):
        for count in (0,1,2):
            self.setbar('Panel',['com.system76.CosmicAppletAudio'] if count else [])
            self.setbar('Dock',['com.system76.CosmicAppletAudio'])
            self.write(self.user/'com.system76.CosmicPanel/v1/entries',json.dumps(['Panel','Dock'] if count==2 else ['Panel']))
            s=self.observation();self.assertTrue(s.inventory())
            host=self.host(count);self.assertTrue(s.inventory(host))
            p=self.groups.pop();os.killpg(p.pid,signal.SIGKILL);p.wait(timeout=2);s.close()
    def test_optional_children_do_not_determine_panel_readiness(self):
        s=self.observation();self.assertTrue(s.inventory())
        host=self.host(0);self.assertTrue(s.inventory(host))
        self.assertEqual(s.applet_issues(host)[0]['reason'],'applet_not_observed')
        foreign=self.host(1)
        self.assertTrue(s.inventory(host));self.assertTrue(s.inventory())
        self.assertEqual(s.applet_issues(host)[0]['reason'],'applet_not_observed')
        # Replacing an applet is observed, but a fresh status still measures the
        # panel separately from that old optional applet process.
        old=self.bin/'cosmic-applets';old.rename(self.bin/'old');shutil.copy2('/usr/bin/sleep',old)
        with self.assertRaises(ValueError):s.check()
        s2=self.observation();self.assertTrue(s2.inventory(foreign))
        self.assertEqual(s2.applet_issues(foreign)[0]['reason'],'applet_not_observed')
    def test_extra_optional_children_do_not_bypass_executable_or_config_aba(self):
        s=self.observation();host=self.host(2);self.assertTrue(s.inventory(host))
        p=self.bin/'cosmic-comp';raw=p.read_bytes();p.rename(self.bin/'previous');p.write_bytes(raw);p.chmod(0o755)
        with self.assertRaises(ValueError):s.check()
        s2=self.observation();path=self.center();raw=path.read_bytes();path.write_bytes(b'Some([])');path.write_bytes(raw)
        with self.assertRaises(ValueError):s2.check()
    def test_supported_aliases_need_not_be_multiplex(self):
        (self.bin/'cosmic-applet-audio').unlink();shutil.copy2('/usr/bin/sleep',self.bin/'cosmic-applet-audio')
        s=self.observation();self.assertEqual(s.configuration.counts,{'com.system76.CosmicAppletAudio':1});self.assertTrue(s.inventory())
    def test_package_metadata_changed_during_family_query_refuses(self):
        status=self.root/'var/lib/dpkg/status';status.parent.mkdir(parents=True);status.write_bytes(b'old')
        original=c.family
        def changed(versions, policy=None):
            result=original(versions, policy=policy)
            status.write_bytes(b'new');status.write_bytes(b'old')
            return result
        with patch.object(c,'family',side_effect=changed):
            with self.assertRaisesRegex(ValueError,'installed_executable_changed'):self.observation()
    def test_required_core_missing_and_unrelated_zombie(self):
        zombie=subprocess.Popen(['/bin/true'],env=self.env);time.sleep(.02)
        try:self.assertTrue(self.observation().inventory())
        finally:zombie.wait()
        actor=self.actors.pop();actor.kill();actor.wait(timeout=2)
        self.assertFalse(self.observation().inventory())

# Avoid duplicating inherited tests; fixture setup is shared, coverage isn't.
for name in dir(config_fixture.Configuration):
    if name.startswith('test_') and name not in Processes.__dict__:
        setattr(Processes,name,None)


class Wired(unittest.TestCase):
    def setUp(self):
        from test_generation_contract import bundle_fixture, FIXTURE_BINARY_SHA
        from bundle_contract import canonical,digest
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
        self.home=self.root/'home';self.home.mkdir();self.library=self.root/'library'
        # Synthetic policy binary solely inside this test's patched constant.
        self.scope=patch.dict(c.POLICY,binary_sha256=FIXTURE_BINARY_SHA);self.scope.start();self.addCleanup(self.scope.stop)
        self.manifest,self.catalog=bundle_fixture(self.library,catalog=False)
        self.catalog['startup_policy']=copy.deepcopy(c.POLICY)
        (self.library/'catalog.json').write_bytes(canonical(self.catalog))
        self.manifest['files']['catalog.json']['sha256']=digest(self.library/'catalog.json')
        (self.library/'companion-manifest.json').write_bytes(canonical(self.manifest))
        (self.library/'manifest.json').write_bytes(canonical(self.manifest))
        (self.library/'manifest.json').chmod(0o644)
        (self.home/'installed-identity.json').write_bytes(canonical(dict(package_versions=VERSIONS)))
    def test_staging_status_disable_no_vm_scan_no_false_ready_or_logout_loop(self):
        import refresh,install,install_status
        with patch('runtime_identity.installed_identity',side_effect=AssertionError('no full inventory')):
            with refresh.locked(self.home):
                result=refresh.stage_locked(self.home,self.library,session='first',fixture=True,enable=True)
            self.assertEqual(result['state'],'staged')
            with patch('panel_launcher.session_identity',return_value='second'):
                for host,label in [(False,'Using standard panel'),(True,'Ready')]:
                    result=install_status.detect(self.home,self.library,True,{},installed={**c.family(VERSIONS), 'applet_issues':[dict(applet='org.example.Missing',reason='desktop_entry_missing')]},host=host)
                    self.assertEqual(result['status'],label,result)
                    self.assertEqual(result['diagnostics']['applet_issues'],[dict(applet='org.example.Missing',reason='desktop_entry_missing')])
                    self.assertTrue(result['compatible']);self.assertIsNone(result['qualification_cohort'])
                self.assertEqual(result['reason'],'family_ready_hardware_not_qualified')
            with patch('panel_launcher.session_identity',return_value='first'):
                result=install_status.detect(self.home,self.library,True,{},installed=c.family(VERSIONS),host=False)
                self.assertEqual(result['status'],'Log out to activate') # truly deferred, once
            with refresh.locked(self.home):
                for path,_ in refresh.records(self.home):install.restore(path)
            self.assertFalse((self.home/refresh.SELECTOR).exists())
    def test_final_status_check_failure_cannot_publish_ready(self):
        import refresh, install_status
        with refresh.locked(self.home):
            refresh.stage_locked(self.home,self.library,session='first',fixture=True,enable=True)
        class Observation:
            def inventory(self, owner): return True
            def applet_issues(self, owner): return [dict(applet='optional', reason='desktop_entry_missing')]
            def check(self): pass
            def close(self): pass
        initial = dict(enabled=True)
        with patch('panel_launcher.session_identity',return_value='second'), patch.object(c,'family',return_value=c.family(VERSIONS)), patch('runtime_identity.host_identity',return_value={'pid':123}), patch.object(c,'Session',return_value=Observation()):
            result = install_status.inspect(self.home,self.library,True,{},initial)
            self.assertEqual(result['status'],'Ready')
            self.assertTrue(result['healthy'])
            self.assertEqual(result['applet_issues'][0]['applet'],'optional')
            with patch.object(Observation,'check',side_effect=ValueError('configuration_changed')):
                result = install_status.inspect(self.home,self.library,True,{},initial)
            self.assertEqual(result['status'],'Using standard panel')
            self.assertFalse(result['healthy'])
            self.assertFalse(result['running'])
            self.assertEqual(result['reason'],'configuration_changed')
    def test_actual_prepare_dispatch_integrity_and_probe_refusal(self):
        import panel_launcher as launcher
        class Observation:
            configuration=None
            def inventory(self,*args):return True
            def check(self):pass
            def close(self):pass
        with patch.object(c,'Session',return_value=Observation()),patch('runtime_identity.installed_identity',side_effect=AssertionError('no VM scan')):
            guard=launcher._prepare_launch(self.library)
            self.assertIsNotNone(guard);guard.close()
            with patch.object(launcher,'runtime_compatible',return_value=False):
                self.assertIsNone(launcher.prepare_launch(self.library))
        (self.library/'runtime_compatibility.py').write_text('tampered')
        self.assertIsNone(launcher._prepare_launch(self.library))
    def test_actual_020_migrates_under_family_policy_and_disable_restores(self):
        import refresh,install
        from test_refresh import Actual020
        self.stage=lambda: refresh.stage_installed_update(self.home,self.library,session='second',fixture=True)
        Actual020.enable(self)
        self.assertEqual(len(refresh.records(self.home)),2)
        with refresh.locked(self.home):
            for path,_ in refresh.records(self.home):install.restore(path)
        self.assertFalse((self.home/refresh.SELECTOR).exists())
        self.assertFalse((self.home/'.local/share/cosmic-desktop-cards/companion/cosmic-panel-islands').exists())
    def test_historical_v2_inventory_remains_readable_but_cannot_claim_new_policy(self):
        from bundle_contract import verify_bundle,canonical,digest,identity
        m=copy.deepcopy(self.manifest)
        del m['files']['journal.py']
        (self.library/'journal.py').unlink()
        del m['files']['runtime_compatibility.py']
        (self.library/'runtime_compatibility.py').unlink()
        m['payload_sha256']=identity({k:v for k,v in m['files'].items() if k!='catalog.json'})
        # A mixed old helper inventory with the new policy fails closed.
        (self.library/'manifest.json').write_bytes(canonical(m))
        with self.assertRaisesRegex(ValueError,'helper_missing'):verify_bundle(self.library)
        old=copy.deepcopy(self.catalog);del old['startup_policy']
        (self.library/'catalog.json').write_bytes(canonical(old))
        m['files']['catalog.json']['sha256']=digest(self.library/'catalog.json')
        (self.library/'manifest.json').write_bytes(canonical(m))
        verified,cat=verify_bundle(self.library)
        self.assertFalse(c.enabled(cat));self.assertEqual(verified['payload_sha256'],m['payload_sha256'])
    def test_previous_generation_without_journal_remains_hash_verified(self):
        from bundle_contract import verify_bundle,canonical,identity
        m=copy.deepcopy(self.manifest)
        del m['files']['journal.py']
        (self.library/'journal.py').unlink()
        m['payload_sha256']=identity({k:v for k,v in m['files'].items() if k!='catalog.json'})
        (self.library/'manifest.json').write_bytes(canonical(m))
        verified,_=verify_bundle(self.library)
        self.assertEqual(verified['payload_sha256'],m['payload_sha256'])
        (self.library/'refresh.py').write_bytes(b'tampered old helper')
        with self.assertRaisesRegex(ValueError,'component_mismatch'):
            verify_bundle(self.library)
    def test_unsupported_family_is_not_staged(self):
        import refresh
        (self.home/'installed-identity.json').write_text(json.dumps(dict(package_versions={**VERSIONS,'cosmic-panel':'2.0'})))
        with refresh.locked(self.home):
            result=refresh.stage_locked(self.home,self.library,session='first',fixture=True,enable=True)
        self.assertEqual(result['reason'],'unsupported_cosmic_family')
        self.assertFalse((self.home/refresh.SELECTOR).exists())

if __name__=='__main__':unittest.main()
