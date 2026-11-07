"""Execute the real v2 CLI in an isolated home, with only host attestations replaced."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from bundle_contract import canonical
from test_generation_contract import bundle_fixture


class EnablePath(unittest.TestCase):
    def test_production_cli_missing_reversed_and_normal_path(self):
        cli=Path(__file__).resolve().parents[1]/'packaging/islands/halcyon-islands'
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);library=root/'library';manifest,catalog=bundle_fixture(library)
            harness=root/'cli-harness.py'
            harness.write_text('''import os,sys,runpy,subprocess
from pathlib import Path
library=Path(sys.argv[2]);sys.path.insert(0,str(library))
import refresh,runtime_identity
# No production bypass: these substitutions exist only in this isolated test.
refresh.LIBRARY=library
if os.environ.get('TEST_RETIRED_SELECTOR'):
    refresh.SHIPPED_SELECTORS=refresh.SHIPPED_SELECTORS|{os.environ['TEST_RETIRED_SELECTOR']}
real_verify=refresh.verify_bundle
refresh.verify_bundle=lambda path,*args,**kw: real_verify(path,*args,**{**kw,'owner':os.getuid()})
cohort=sys.argv[3]
runtime_identity.installed_identity=lambda _: {'format':'halcyon-installed-runtime','version':1,'scope':'islands-runtime','verified':True,'targets':[cohort]}
real_run=subprocess.run
subprocess.run=lambda args,**kw: subprocess.CompletedProcess(args,0,os.environ.get('TEST_PACKAGE_STATUS','install ok installed')+'\\n0.2.1','') if args[0]=='/usr/bin/dpkg-query' else real_run(args,**kw)
entry=runpy.run_path(sys.argv[1]);entry['main'].__globals__['LIBRARY']=library
sys.argv=[sys.argv[1],'enable']
sys.exit(entry['main']())
''')
            for case in ('missing','reversed','normal','held','partial_held','custom_failed','unused_default_failed','orphan','retired_orphan','foreign'):
                with self.subTest(case=case):
                    home=root/case;home.mkdir();local=str(home/'.local/bin')
                    path={'missing':'/usr/bin','reversed':'/usr/bin:'+local}.get(case,local+':/usr/bin')
                    env={**os.environ,'HOME':str(home),'XDG_DATA_HOME':str(home/'.local/share'),'XDG_CONFIG_HOME':str(home/'.config'),'XDG_STATE_HOME':str(home/'.local/state'),'XDG_CURRENT_DESKTOP':'COSMIC','XDG_SESSION_ID':'isolated-test','PATH':path}
                    if case in ('held','partial_held'):
                        env['TEST_PACKAGE_STATUS']='hold ok installed' if case=='held' else 'hold ok unpacked'
                    if case in ('custom_failed','unused_default_failed'):
                        state=home/'custom-state';env['XDG_STATE_HOME']=str(state)
                        failed=(state if case=='custom_failed' else home/'.local/state')/'desktop-cards-panel/failed'
                        failed.parent.mkdir(parents=True);failed.touch()
                    selector=home/'.local/bin/cosmic-panel'
                    if case in ('orphan','retired_orphan','foreign'):
                        # A reinstall that dropped the journal but left a selector behind.
                        selector.parent.mkdir(parents=True)
                        selector.write_bytes({'orphan':(library/'panel_launcher.py').read_bytes(),
                                              'retired_orphan':b'retired',
                                              'foreign':b'#!/bin/sh\nexec /usr/bin/cosmic-panel\n'}[case])
                        selector.chmod(0o755)
                        if case=='retired_orphan':
                            env['TEST_RETIRED_SELECTOR']=hashlib.sha256(b'retired').hexdigest()
                    run=subprocess.run([sys.executable,str(harness),str(cli),str(library),catalog['cohorts'][0]['runtime_target']['id']],env=env,text=True,capture_output=True,timeout=20)
                    if case in ('normal','held','unused_default_failed','orphan','retired_orphan'):
                        self.assertEqual(run.returncode,0,run.stderr)
                        self.assertEqual(json.loads(run.stdout)['state'],'staged')
                        self.assertEqual((home/'.local/bin/cosmic-panel').read_bytes(),(library/'selector.py').read_bytes())
                    elif case=='foreign':
                        self.assertNotEqual(run.returncode,0)
                        self.assertEqual(json.loads(run.stdout)['reason'],'unowned_selector')
                        self.assertEqual(selector.read_bytes(),b'#!/bin/sh\nexec /usr/bin/cosmic-panel\n')
                    elif case=='partial_held':
                        self.assertNotEqual(run.returncode,0)
                        self.assertEqual(json.loads(run.stdout)['reason'],'package_absent')
                        self.assertFalse((home/'.local/bin/cosmic-panel').exists())
                    elif case=='custom_failed':
                        self.assertNotEqual(run.returncode,0)
                        self.assertIn('previous Islands failure',run.stderr)
                        self.assertFalse((home/'.local/bin/cosmic-panel').exists())
                        self.assertTrue(failed.exists())
                    else:
                        self.assertNotEqual(run.returncode,0)
                        self.assertIn('PATH must put ~/.local/bin before /usr/bin',run.stderr)
                        self.assertEqual(list(home.iterdir()),[], 'precondition creates no lock/journal or shell configuration')
                    self.assertFalse((home/'.profile').exists())
                    self.assertFalse((home/'.bashrc').exists())

if __name__=='__main__':unittest.main()
