"""Package ownership migration and ordinary APT upgrade, entirely in temp roots."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import version_policy as policy

MAIN = 'cosmic-ext-halcyon'
ISLANDS = MAIN + '-islands'


class Bundled(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='halcyon-bundle-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root/'Cargo.toml').write_text('[package]\nversion="0.5.0"\n[package.metadata.halcyon]\nbundled-islands=true\n')
        self.packages = self.root/'packages'
        self.packages.mkdir()

    def package(self, name, version, extra='', payload=None):
        root = self.root/f'{name}-{version}'
        (root/'DEBIAN').mkdir(parents=True, exist_ok=True)
        (root/'DEBIAN/control').write_text(f'Package: {name}\nVersion: {version}\nArchitecture: amd64\nMaintainer: Fixture <fixture@example.invalid>\nDescription: private fixture\n{extra}')
        for relative, value in (payload or {}).items():
            path = root/relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(value)
        if payload and 'etc/xdg/autostart/halcyon-islands-bootstrap.desktop' in payload:
            (root/'DEBIAN/conffiles').write_text('/etc/xdg/autostart/halcyon-islands-bootstrap.desktop\n')
        out = self.packages/f'{name}_{version}_amd64.deb'
        subprocess.run(['dpkg-deb','--root-owner-group','--build',str(root),str(out)],check=True,capture_output=True)
        return out

    def candidates(self):
        extra = f'Breaks: {policy.main_breaks(self.root)}\nReplaces: {policy.main_breaks(self.root)}\nX-Halcyon-Islands: bundled\n'
        payload = {'usr/bin/halcyon': 'main', 'usr/bin/halcyon-islands':'new helper',
                   'usr/lib/cosmic-ext-halcyon-islands/cosmic-panel-islands':'new host',
                   'etc/xdg/autostart/halcyon-islands-bootstrap.desktop':'bootstrap'}
        main = self.package(MAIN,'0.5.0',extra,payload)
        bridge = self.package(ISLANDS,'0.5.0',f'Depends: {MAIN} (>= 0.5.0)\nX-Halcyon-Islands: transitional\n')
        policy.verify_bundled(self.root, main, bridge)
        return main, bridge

    def test_payload_ownership_survives_upgrade_and_bridge_purge(self):
        oldmain = self.package(MAIN,'0.4.1',f'Breaks: {ISLANDS} (<< 0.4.1), {ISLANDS} (>> 0.4.1)\n',payload={'usr/bin/halcyon':'old main'})
        oldaddon = self.package(ISLANDS,'0.4.1',f'Depends: {MAIN} (>= 0.4.1)\n',payload={
            'usr/bin/halcyon-islands':'old helper',
            'usr/lib/cosmic-ext-halcyon-islands/cosmic-panel-islands':'old host',
            'etc/xdg/autostart/halcyon-islands-bootstrap.desktop':'bootstrap'})
        main, bridge = self.candidates()
        for reverse in (False, True):
            private = self.root/f'system-{reverse}'
            private.mkdir()
            base = ['dpkg',f'--root={private}',f'--log={private}/dpkg.log','--force-not-root','--force-confold','--auto-deconfigure']
            def run(*args):
                result = subprocess.run(base+list(args),capture_output=True,text=True,timeout=20)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            run('--install',str(oldmain),str(oldaddon))
            # A local change to the old conffile is retained across ownership transfer.
            bootstrap=private/'etc/xdg/autostart/halcyon-islands-bootstrap.desktop'
            bootstrap.write_text('administrator choice')
            run('--install', *map(str, (bridge, main) if reverse else (main, bridge)))
            run('--purge',ISLANDS)
            self.assertEqual((private/'usr/bin/halcyon-islands').read_text(),'new helper')
            self.assertEqual((private/'usr/lib/cosmic-ext-halcyon-islands/cosmic-panel-islands').read_text(),'new host')
            self.assertEqual(bootstrap.read_text(),'administrator choice')
            self.assertIn('/usr/bin/halcyon-islands',(private/'var/lib/dpkg/info/cosmic-ext-halcyon.list').read_text())

    def test_bridge_payload_or_scripts_and_wrong_owner_are_rejected(self):
        main, bridge = self.candidates()
        extra = f'Depends: {MAIN} (>= 0.5.0)\nX-Halcyon-Islands: transitional\n'
        dirty = self.package(ISLANDS,'0.5.0',extra,{'usr/bin/unrelated':'payload'})
        with self.assertRaisesRegex(ValueError, 'transition_must_be_empty'):
            policy.verify_bundled(self.root, main, dirty)
        (self.root/f'{ISLANDS}-0.5.0/usr/bin/unrelated').unlink()
        (self.root/f'{ISLANDS}-0.5.0/DEBIAN/postinst').write_text('#!/bin/sh\nexit 0\n')
        (self.root/f'{ISLANDS}-0.5.0/DEBIAN/postinst').chmod(0o755)
        scripted = self.package(ISLANDS,'0.5.0',extra)
        with self.assertRaisesRegex(ValueError, 'transition_has_control_actions'):
            policy.verify_bundled(self.root, main, scripted)
        wrong = self.package(MAIN,'0.5.0',f'Breaks: {policy.main_breaks(self.root)}\nReplaces: {policy.main_breaks(self.root)}\n')
        with self.assertRaisesRegex(ValueError, 'bundled_package_contract'):
            policy.verify_bundled(self.root, wrong)

    def test_ordinary_apt_upgrade_and_fresh_install_need_no_removal(self):
        main, bridge = self.candidates()
        repo = self.root/'repo'; repo.mkdir()
        for p in (main,bridge): shutil.copy(p,repo/p.name)
        scan = subprocess.run(['dpkg-scanpackages','.','/dev/null'],cwd=repo,capture_output=True,text=True,check=True)
        (repo/'Packages').write_text(scan.stdout)
        for legacy in [False,True]:
            root = self.root/str(legacy)
            for p in ['etc/apt/apt.conf.d','etc/apt/sources.list.d','var/lib/apt/lists/partial','var/lib/dpkg','var/cache/apt/archives/partial','var/log/apt']:
                (root/p).mkdir(parents=True,exist_ok=True)
            (root/'etc/apt/sources.list').write_text(f'deb [trusted=yes] file:{repo} ./\n')
            installed = [(MAIN,'0.4.1'),(ISLANDS,'0.4.1')] if legacy else []
            (root/'var/lib/dpkg/status').write_text(''.join(f'Package: {name}\nStatus: install ok installed\nVersion: {version}\nArchitecture: amd64\nDescription: fixture\n' + (f'Breaks: {ISLANDS} (<< 0.4.1), {ISLANDS} (>> 0.4.1)\n\n' if name == MAIN else f'Depends: {MAIN} (>= 0.4.1)\n\n') for name,version in installed))
            config=root/'apt.conf'
            config.write_text(f'Dir "{root}";\nDir::Etc "{root}/etc/apt";\nDir::State::status "{root}/var/lib/dpkg/status";\nAPT::Architecture "amd64";\nAPT::Sandbox::User "{os.getuid()}";\n#clear DPkg::Pre-Install-Pkgs;\n#clear DPkg::Post-Invoke;\n')
            env={**os.environ,'APT_CONFIG':str(config),'LC_ALL':'C'}
            update=subprocess.run(['apt-get','update'],env=env,capture_output=True,text=True,timeout=20)
            self.assertEqual(update.returncode,0,update.stderr)
            command=['apt-get','--simulate','--no-remove']+(['upgrade'] if legacy else ['install',MAIN])
            result=subprocess.run(command,env=env,capture_output=True,text=True,timeout=20)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertIn('Inst '+MAIN,result.stdout)
            self.assertEqual('Inst '+ISLANDS in result.stdout,legacy,result.stdout)
            self.assertNotIn('Remv ',result.stdout)
            self.assertNotIn('kept back',result.stdout)


if __name__=='__main__': unittest.main()
