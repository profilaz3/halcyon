import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('rpm_defaults',Path(__file__).with_name('rpm_defaults.py'))
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)


class Enrollment(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name, data in [('source.repo',b'[halcyon]\nenabled=1\n'),('archive-key.asc',b'fixture public key')]:
            file = m.path(self.root,m.BASE+'/'+name); file.parent.mkdir(parents=True,exist_ok=True);file.write_bytes(data)
    def source(self): return m.path(self.root,m.SOURCE)
    def state(self): return json.loads(m.path(self.root,m.STATE).read_text())
    def test_clean_install_is_idempotent_and_offline(self):
        m.configure(self.root); before = self.source().read_bytes(); m.configure(self.root)
        self.assertEqual(self.source().read_bytes(),before)
        self.assertEqual(self.state()['phase'],'complete')
    def test_disabled_deleted_or_renamed_source_stays_that_way(self):
        for edit in ('disabled','deleted','renamed'):
            with self.subTest(edit=edit):
                m.path(self.root,m.STATE).unlink(missing_ok=True)
                for file in self.source().parent.glob('*'): file.unlink()
                m.path(self.root,m.KEY).unlink(missing_ok=True)
                m.configure(self.root)
                if edit == 'disabled': self.source().write_text('[halcyon]\nenabled=0\n')
                elif edit == 'renamed': self.source().rename(self.source().with_name('custom.repo'))
                else: self.source().unlink()
                m.configure(self.root)
                if edit == 'disabled': self.assertIn('enabled=0',self.source().read_text())
                else: self.assertFalse(self.source().exists())
    def test_pending_rename_is_preserved_on_retry(self):
        original = m.receipt
        def interrupted(root,state):
            if state['phase']=='complete': raise OSError('fixture power loss')
            original(root,state)
        with patch.object(m,'receipt',interrupted):
            with self.assertRaises(OSError): m.configure(self.root)
        self.source().rename(self.source().with_name('admin.repo'))
        m.configure(self.root)
        self.assertFalse(self.source().exists()); self.assertEqual(self.state()['phase'],'preserved')
    def test_remove_reinstall_preserves_choice_and_shared_key(self):
        m.configure(self.root)
        self.source().with_name('admin.repo').write_bytes(self.source().read_bytes())
        m.remove(self.root); m.configure(self.root)
        self.assertFalse(self.source().exists()); self.assertTrue(m.path(self.root,m.KEY).exists())
        self.assertTrue(self.source().with_name('admin.repo').exists())
    def test_foreign_configuration_is_not_adopted(self):
        self.source().parent.mkdir(parents=True)
        self.source().with_name('other.repo').write_text('[custom]\nbaseurl=https://example.invalid/halcyon\n')
        m.configure(self.root)
        self.assertEqual(self.state()['phase'],'preserved'); self.assertFalse(self.source().exists())
    def test_interrupted_publish_never_leaves_an_empty_destination(self):
        with patch.object(m.os,'link',side_effect=OSError('fixture interruption')):
            with self.assertRaises(OSError): m.configure(self.root)
        self.assertFalse(self.source().exists()); self.assertFalse(m.path(self.root,m.KEY).exists())
        m.configure(self.root); self.assertTrue(self.source().read_bytes())

    def test_key_rotation_is_recoverable_and_preserves_source_edits(self):
        m.configure(self.root)
        m.path(self.root,m.BASE+'/archive-key.asc').write_bytes(b'old and new public keys')
        original=m.receipt
        def fail_completion(root,state):
            if state['phase']=='complete':raise OSError('receipt interruption')
            original(root,state)
        with patch.object(m,'receipt',fail_completion):
            with self.assertRaises(OSError):m.configure(self.root)
        self.assertEqual(self.state()['phase'],'key-update')
        m.configure(self.root);self.assertEqual(self.state()['phase'],'complete')
        self.assertEqual(m.path(self.root,m.KEY).read_bytes(),b'old and new public keys')
        self.source().write_text('[halcyon]\nenabled=0\n')
        m.path(self.root,m.BASE+'/archive-key.asc').write_bytes(b'another key')
        m.configure(self.root)
        self.assertEqual(m.path(self.root,m.KEY).read_bytes(),b'old and new public keys')


if __name__ == '__main__': unittest.main()
