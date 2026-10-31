import copy
from pathlib import Path
import unittest
import os
import tempfile
from unittest.mock import patch
import runtime_compatibility as r


class RpmPolicy(unittest.TestCase):
    def test_unrelated_multilib_and_retained_kernels_do_not_hide_cosmic(self):
        import runtime_identity as identity
        from types import SimpleNamespace
        rows = 'kernel\t0:6.17-1\tx86_64\nkernel\t0:6.18-1\tx86_64\n'
        rows += 'openssl-libs\t1:3.5-1\ti686\nopenssl-libs\t1:3.5-1\tx86_64\n'
        rows += 'cosmic-panel\t0:1.10.0-2.fc43\tx86_64\n'
        with patch.object(identity, 'rpm_system', return_value=True), patch.object(identity, 'rpm_query', return_value=SimpleNamespace(returncode=0, stdout=rows)):
            measured = identity.versions()
        self.assertNotIn('kernel', measured)
        self.assertNotIn('kernel:x86_64', measured)
        self.assertNotIn('openssl-libs', measured)
        self.assertEqual(measured['openssl-libs:x86_64'], ('1:3.5-1', 'x86_64'))
        self.assertEqual(measured['cosmic-panel'], ('0:1.10.0-2.fc43', 'x86_64'))

    def test_database_watch_ignores_reader_metadata_but_rejects_writes_and_aba(self):
        from panel_configuration import Watch
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'rpmdb.sqlite-wal';path.write_bytes(b'fixture')
            watch=Watch()
            try:
                watch.add_content(path)
                os.chmod(path,0o644)
                with path.open('ab'):pass
                self.assertFalse(watch.changed())
                path.write_bytes(b'changed');self.assertTrue(watch.changed())
            finally:watch.close()
            watch=Watch()
            try:
                watch.add_content(path);saved=path.with_suffix('.saved')
                path.rename(saved);saved.rename(path);self.assertTrue(watch.changed())
            finally:watch.close()
    def test_fedora_policy_binds_separate_native_bytes(self):
        original=copy.deepcopy(r.POLICY)
        catalog={'startup_policy':r.FEDORA_POLICY}
        manifest={'upstream':r.CONFIGURATION_SOURCE['upstream'],'protocol':1,'runtime_probe':1,
                  'binary_sha256':r.FEDORA_POLICY['binary_sha256']}
        r.validate_bundle(manifest,catalog)
        for field,value in [('binary_sha256',r.POLICY['binary_sha256']),('runtime_probe',0)]:
            with self.assertRaises(ValueError):r.validate_bundle({**manifest,field:value},catalog)
        with self.assertRaises(ValueError):r.enabled({'startup_policy':{**r.FEDORA_POLICY,'families':['1.99']}})
        self.assertEqual(r.POLICY,original)

    def test_platform_contract_rejects_other_fedora_releases(self):
        with patch('platform.machine',return_value='x86_64'):
            for version in ('42','44'):
                with patch.object(Path,'read_text',return_value='ID=fedora\nVERSION_ID='+version+'\n'):
                    with self.assertRaises(ValueError):r.validate_platform({'startup_policy':r.FEDORA_POLICY})
            with patch.object(Path,'read_text',return_value='ID=fedora\nVERSION_ID=43\n'):
                r.validate_platform({'startup_policy':r.FEDORA_POLICY})
        with patch.object(r,'rpm_system',return_value=True):
            with self.assertRaises(ValueError):r.validate_platform({'startup_policy':r.POLICY})
            versions={name:'0:1.10.0-2.fc43' for name in ('cosmic-panel',*r.CORE)}
            self.assertEqual(r.family(versions)['policy'],r.FEDORA_POLICY['id'])
            versions['cosmic-panel']='0:1.11.0-1.fc43'
            with self.assertRaises(ValueError):r.family(versions)


if __name__=='__main__':unittest.main()
