#!/usr/bin/env python3
"""Final package fixtures; no installation or host preference changes."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'companion'))
from bundle_contract import canonical,digest,verify_bundle
from test_generation_contract import catalog_fixture, installed

HERE=Path(__file__).parent;REPO=HERE.parent.parent;ARTIFACTS=None
spec=importlib.util.spec_from_file_location('builder',HERE/'build-deb.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)

class IslandsPackage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp=tempfile.TemporaryDirectory();cls.addClassCleanup(cls.tmp.cleanup);cls.root=Path(cls.tmp.name)
        cls.binary=cls.root/'binary';cls.binary.write_text('#!/bin/sh\nexit 99\n')
        cls.manifest=cls.root/'input.json';cls.manifest.write_bytes(canonical(dict(protocol=1,upstream='ca20ce3185b3b4404f429d59d429c40b58a2ddef',runtime_probe=1,binary_sha256=digest(cls.binary))))
        os.environ['SOURCE_DATE_EPOCH']='1792627200'
        # Synthetic package tests use their own empty policy, never the actual
        # accepted release payload/catalog or a native-admission claim.
        cls.empty_catalog=cls.root/'empty-catalog.json'
        cls.empty_catalog.write_bytes(canonical(dict(format='halcyon-islands-catalog',version=2,cohorts=[])))
        cls.deb=builder.build(cls.binary,cls.manifest,'0.2.1~test.1',cls.root/'out',catalog=cls.empty_catalog)
        cls.extract=cls.root/'extract';subprocess.run(['dpkg-deb','-x',str(cls.deb),str(cls.extract)],check=True)
        cls.library=cls.extract/'usr/lib/cosmic-ext-halcyon-islands'
        if ARTIFACTS:
            ARTIFACTS.mkdir(parents=True,exist_ok=True);shutil.copy2(cls.deb,ARTIFACTS/cls.deb.name)
    def test_final_bundle_full_inventory_and_bootstrap(self):
        manifest,catalog=verify_bundle(self.library,'companion-manifest.json')
        self.assertEqual(catalog['cohorts'],[])
        self.assertEqual(manifest['package_version'],'0.2.1~test.1')
        desktop=self.extract/'etc/xdg/autostart/halcyon-islands-bootstrap.desktop'
        self.assertIn('/refresh.py',desktop.read_text());self.assertIn('TryExec=/usr/bin/halcyon-islands',desktop.read_text())
        self.assertEqual(subprocess.run(['desktop-file-validate',str(desktop)],capture_output=True).returncode,0)
        control=self.root/'control';subprocess.run(['dpkg-deb','-e',str(self.deb),str(control)],check=True)
        self.assertEqual(sorted(p.name for p in control.iterdir()),['conffiles','control'])
        from version_policy import companion_depends
        self.assertIn(companion_depends(REPO), (control/'control').read_text())
    def test_reproducible_and_no_version_replacement(self):
        before=digest(self.deb)
        builder.build(self.binary,self.manifest,'0.2.1~test.1',self.root/'out',catalog=self.empty_catalog)
        self.assertEqual(digest(self.deb),before)
        bad=self.root/'changed';bad.write_text('#!/bin/sh\nexit 98\n')
        record=json.loads(self.manifest.read_bytes());record['binary_sha256']=digest(bad)
        path=self.root/'changed.json';path.write_bytes(canonical(record))
        with self.assertRaisesRegex(ValueError,'immutable'):builder.build(bad,path,'0.2.1~test.1',self.root/'out',catalog=self.empty_catalog)
        self.assertEqual(digest(self.deb),before)
    def test_wrong_digest_protocol_source_and_catalog_rejected(self):
        record=json.loads(self.manifest.read_bytes())
        for key,value in [('binary_sha256','0'*64),('protocol',2),('upstream','unknown')]:
            path=self.root/(key+'.json');path.write_bytes(canonical({**record,key:value}))
            with self.assertRaises(ValueError):builder.build(self.binary,path,'0.2.1',self.root/key,catalog=self.empty_catalog)
        path=self.root/'wrong-catalog.json';path.write_bytes(canonical(catalog_fixture()))
        with self.assertRaisesRegex(ValueError,'different finalized payload'):builder.build(self.binary,self.manifest,'0.2.1',self.root/'catalog',catalog=path)
    def test_empty_catalog_blocks_enable_but_management_and_json_remain(self):
        home=self.root/'home';home.mkdir(exist_ok=True)
        cli=self.extract/'usr/bin/halcyon-islands'
        env={**os.environ,'XDG_SESSION_ID':'unit','HOME':str(home)}
        def run(*args):return subprocess.run([str(cli),'--root',str(home),*args],env=env,capture_output=True,text=True)
        status=run('status','--json');self.assertEqual(status.returncode,0,status.stderr)
        value=json.loads(status.stdout);self.assertEqual(value['installation'],'installed');self.assertFalse(value['enabled'])
        enabled=run('enable');self.assertEqual(enabled.returncode,1,enabled.stdout+enabled.stderr)
        self.assertIn('no_qualified_cohort',enabled.stdout)
        self.assertFalse((home/'.local/bin/cosmic-panel').exists())
        self.assertEqual(run('disable').returncode,0)
        diagnostics=run('diagnostics');self.assertEqual(diagnostics.returncode,0)
        self.assertNotIn(str(home),diagnostics.stdout)
    def test_final_package_cli_actual_020_refresh_and_disable(self):
        from test_refresh import DEFAULT_RELEASE, RELEASE_SHA
        released=Path(os.environ.get('HALCYON_RELEASED_020',str(DEFAULT_RELEASE)))
        if not released.is_file():self.skipTest('Actual public 0.2.0 package required')
        self.assertEqual(digest(released),RELEASE_SHA)
        with tempfile.TemporaryDirectory(dir=self.root) as tmp:
            root=Path(tmp);home=root/'home';home.mkdir()
            old=root/'old';subprocess.run(['dpkg-deb','-x',str(released),str(old)],check=True)
            legacy=old/'usr/lib/cosmic-ext-halcyon-islands'
            env={**os.environ,'XDG_SESSION_ID':'old-login','HOME':str(home)}
            subprocess.run([sys.executable,'-B',str(legacy/'install.py'),'--root',str(home),
                '--binary',str(legacy/'cosmic-panel-islands'),'--manifest',str(legacy/'companion-manifest.json'),
                '--enable-next-login','--record-owner','cosmic-ext-halcyon-islands'],env=env,check=True,capture_output=True)
            initial,_=verify_bundle(self.library,'companion-manifest.json')
            catalog=catalog_fixture(initial['payload_sha256']);catalog_path=root/'catalog.json';catalog_path.write_bytes(canonical(catalog))
            # Synthetic admission is confined to this extracted package and home;
            # it exercises real CLI/archiver/restore, never claims native success.
            candidate=builder.build(self.binary,self.manifest,'0.2.1',root/'out',catalog=catalog_path)
            new=root/'new';subprocess.run(['dpkg-deb','-x',str(candidate),str(new)],check=True)
            (home/'installed-identity.json').write_bytes(canonical(installed(catalog['cohorts'][0])))
            cli=new/'usr/bin/halcyon-islands'
            def call(command):
                return subprocess.run([str(cli),'--root',str(home),command],env=env,capture_output=True,text=True)
            refreshed=call('refresh');self.assertEqual(refreshed.returncode,0,refreshed.stderr)
            self.assertEqual(json.loads(refreshed.stdout)['state'],'staged')
            self.assertEqual(json.loads(call('refresh').stdout)['state'],'unchanged')
            state=call('diagnostics');self.assertEqual(state.returncode,0,state.stderr)
            self.assertTrue(json.loads(state.stdout)['enabled']);self.assertNotIn(str(home),state.stdout)
            disabled=call('disable');self.assertEqual(disabled.returncode,0,disabled.stderr)
            self.assertFalse((home/'.local/bin/cosmic-panel').exists())
            self.assertFalse((home/'.local/share/cosmic-desktop-cards/companion/cosmic-panel-islands').exists())
            self.assertEqual(json.loads(call('refresh').stdout)['state'],'disabled')

    def test_native_elf_dependencies_and_notices(self):
        binary=Path('/usr/bin/true');record=self.root/'native.json'
        record.write_bytes(canonical(dict(protocol=1,upstream='ca20ce3185b3b4404f429d59d429c40b58a2ddef',binary_sha256=digest(binary))))
        with self.assertRaisesRegex(ValueError,'licenses'):builder.build(binary,record,'0.2.1',self.root/'native',catalog=self.empty_catalog)
        licenses=self.root/'notices';licenses.write_text('Native dependency test notices\n')
        package=builder.build(binary,record,'0.2.1',self.root/'native',licenses,catalog=self.empty_catalog)
        deps=subprocess.check_output(['dpkg-deb','-f',str(package),'Depends'],text=True)
        self.assertIn('libc6 (>=',deps)
        self.assertNotIn('cosmic-comp (',deps)
    def test_bootstrap_stays_quiet_once_its_package_is_removed(self):
        # dpkg keeps a conffile after remove (until purge), so the autostart
        # entry must only fire while the files it runs are still installed.
        # systemd's autostart generator honours TryExec; cosmic-session's own
        # launcher ignores it, so the command checks for its script too.
        import shlex
        entry=dict(line.split('=',1) for line in (self.extract/'etc/xdg/autostart/halcyon-islands-bootstrap.desktop').read_text().splitlines() if '=' in line)
        target=entry['TryExec'];words=shlex.split(entry['Exec'])
        self.assertEqual(words[:2],['/bin/sh','-c'])
        script=shlex.split(words[2])[-1]
        self.assertTrue(words[2].startswith(f'test -f {script} || exit 0;'),words[2])
        private=self.root/'removal';private.mkdir()
        base=['dpkg',f'--root={private}',f'--log={private}/dpkg.log','--force-not-root','--force-depends','--force-script-chrootless']
        def run(*args):
            result=subprocess.run(base+list(args),capture_output=True,text=True,timeout=60)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        (private/'var/lib/dpkg/updates').mkdir(parents=True);(private/'var/lib/dpkg/info').mkdir()
        for name in ('status','available'):(private/'var/lib/dpkg'/name).touch()
        run('--install',str(self.deb))
        shipped=(private/'var/lib/dpkg/info'/f'{builder.PACKAGE}.list').read_text().splitlines()
        self.assertIn(target,shipped,'TryExec must name a file this package ships')
        self.assertIn(script,shipped)
        self.assertTrue(os.access(private/target.lstrip('/'),os.X_OK))
        run('--remove',builder.PACKAGE)
        self.assertTrue((private/'etc/xdg/autostart/halcyon-islands-bootstrap.desktop').exists())
        self.assertFalse((private/script.lstrip('/')).exists())
        self.assertFalse((private/target.lstrip('/')).exists(),'a removed package must not run at login')
    def test_lintian(self):
        result=subprocess.run(['lintian','--allow-root','--fail-on','error',str(self.deb)],capture_output=True,text=True)
        if ARTIFACTS:(ARTIFACTS/'lintian.txt').write_text(result.stdout+result.stderr)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--artifacts',type=Path);args,remaining=parser.parse_known_args();ARTIFACTS=args.artifacts
    unittest.main(argv=[sys.argv[0],*remaining],verbosity=2)
