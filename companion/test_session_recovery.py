"""Display-bound session discovery, using a real Unix peer and synthetic procfs."""
import os
from pathlib import Path
import socket
import struct
import tempfile
import unittest
from unittest.mock import patch

from runtime_identity import recover_session_environment


class SessionRecovery(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.runtime = self.root/'runtime'
        self.runtime.mkdir()
        self.address = self.runtime/'wayland-test'
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(server.close)
        server.bind(str(self.address))
        server.listen(32)
        self.proc = self.root/'proc'
        self.process = self.proc/str(os.getpid())
        self.process.mkdir(parents=True)
        fields = ['S', '1', '2', '3', *(['0']*15), '123']
        (self.process/'stat').write_text(f'{os.getpid()} (cosmic-comp) '+ ' '.join(fields))
        self.compositor = self.root/'cosmic-comp'
        self.compositor.write_bytes(b'synthetic executable identity')
        (self.process/'exe').symlink_to(self.compositor)
        self.values = {
            'XDG_SESSION_ID': 'c42', 'XDG_CURRENT_DESKTOP': 'COSMIC',
            'XDG_SESSION_TYPE': 'wayland', 'XDG_RUNTIME_DIR': str(self.runtime),
        }
        self.write_environment(self.values)
        self.env = {'XDG_RUNTIME_DIR': str(self.runtime), 'WAYLAND_DISPLAY': self.address.name}

    def write_environment(self, values):
        (self.process/'environ').write_bytes(b'\0'.join(f'{k}={v}'.encode() for k, v in values.items())+b'\0')

    def recover(self, env=None):
        return recover_session_environment(self.env if env is None else env,
                                          proc=self.proc, compositor=self.compositor)

    def test_missing_id_comes_from_the_selected_compositor_without_mutating_caller(self):
        self.assertEqual(self.recover(), {'XDG_SESSION_ID': 'c42'})
        self.assertNotIn('XDG_SESSION_ID', self.env)
        self.assertEqual(self.recover({**self.env, 'WAYLAND_DISPLAY': str(self.address)}),
                         {'XDG_SESSION_ID': 'c42'})

    def test_existing_session_is_never_replaced(self):
        with patch('socket.socket', side_effect=AssertionError('must not probe another session')):
            self.assertEqual(self.recover({**self.env, 'XDG_SESSION_ID': 'existing'}), {})

    def test_display_and_runtime_are_required(self):
        for missing in self.env:
            with self.subTest(missing=missing):
                self.assertEqual(self.recover({k: v for k, v in self.env.items() if k != missing}), {})
        self.assertEqual(self.recover({**self.env, 'WAYLAND_DISPLAY': 'missing'}), {})
        regular = self.runtime/'not-a-socket'
        regular.touch()
        self.assertEqual(self.recover({**self.env, 'WAYLAND_DISPLAY': regular.name}), {})

    def test_unrelated_executable_is_rejected(self):
        other = self.root/'other-compositor'
        other.touch()
        (self.process/'exe').unlink()
        (self.process/'exe').symlink_to(other)
        self.assertEqual(self.recover(), {})

    def test_other_peer_user_is_rejected(self):
        with patch('socket.socket') as connection:
            connection.return_value.__enter__.return_value.getsockopt.return_value = struct.pack(
                '3i', os.getpid(), os.getuid()+1, os.getgid())
            self.assertEqual(self.recover(), {})

    def test_unrelated_or_incomplete_session_metadata_is_rejected(self):
        changes = [
            {'XDG_SESSION_ID': ''}, {'XDG_SESSION_ID': 'invalid/session'},
            {'XDG_SESSION_ID': 'x'*129}, {'XDG_CURRENT_DESKTOP': 'GNOME'},
            {'XDG_SESSION_TYPE': 'x11'}, {'XDG_RUNTIME_DIR': str(self.root)},
        ]
        for change in changes:
            with self.subTest(change=change):
                self.write_environment({**self.values, **change})
                self.assertEqual(self.recover(), {})

    def test_changed_process_and_oversized_or_unreadable_environment_are_rejected(self):
        with patch('runtime_identity._stat', side_effect=[{'start': '1'}, {'start': '2'}]):
            self.assertEqual(self.recover(), {})
        (self.process/'environ').write_bytes(b'x'*(1024*1024+1))
        self.assertEqual(self.recover(), {})
        (self.process/'environ').unlink()
        self.assertEqual(self.recover(), {})


if __name__ == '__main__':
    unittest.main()
