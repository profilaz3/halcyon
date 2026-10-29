"""Actual AF_UNIX wire IO and isolated libwayland object-map regressions.

Synthetic outputs, never native COSMIC acceptance. No WAYLAND_SOCKET consumption.
"""
from contextlib import contextmanager
from pathlib import Path
import select
import shutil
import socket
import struct
import subprocess
import tempfile
import threading
import time
import unittest

from panel_configuration import output_names


def string(value):
    b = value.encode() + b'\0'
    return struct.pack('I', len(b)) + b + b'\0' * (-len(b) % 4)


@contextmanager
def wire_server(count=1, version=4, fault=None):
    """Strict client-ID high-water allocation (including callbacks), real wire."""
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / 'display'
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(path)); listener.listen(1); listener.settimeout(2)
            ids, errors = [], []
            def serve():
                try:
                    with listener.accept()[0] as c:
                        c.settimeout(2)
                        def read(n):
                            b = b''
                            while len(b) < n:
                                part = c.recv(n - len(b))
                                if not part: raise EOFError
                                b += part
                            return b
                        def send(obj, op, b):
                            c.sendall(struct.pack('II', obj, (len(b)+8)<<16 | op)+b)
                        def protocol_error(payload):
                            send(1, 0, payload)
                            # Keep the read side open for the already queued
                            # sync request. Closing both sides races sendall and
                            # tests a broken pipe rather than error decoding.
                            c.shutdown(socket.SHUT_WR)
                            while c.recv(4096):
                                pass
                        bound = []
                        syncs = 0
                        for _ in range(40):
                            obj, header = struct.unpack('II', read(8))
                            size, op = header >> 16, header & 65535
                            assert 8 <= size <= 256 and size % 4 == 0
                            b = read(size-8)
                            new_id = struct.unpack_from('I', b, len(b)-4)[0]
                            # Pinned ObjectMap.insert_at rejects gaps/occupied IDs.
                            if new_id != len(ids)+2:
                                send(1, 0, struct.pack('II', 1, 0)+string(f'Invalid new_id: {new_id}.'))
                                return
                            ids.append(new_id)
                            if obj == 1 and op == 1:
                                assert new_id == 2 and len(b) == 4
                                if fault == 'error':
                                    protocol_error(struct.pack('II', 2, 0)+string('Invalid new_id: 10.')); return
                                if fault == 'c_error':
                                    protocol_error(struct.pack('II', 1, 1)+string('invalid arguments for wl_registry#2.bind')); return
                                if fault == 'private_error':
                                    protocol_error(struct.pack('II', 2, 3)+string('/home/private/token\n'+'x'*1024)); return
                                if fault == 'malformed_error':
                                    protocol_error(struct.pack('III', 1, 0, 100)); return
                                for n in range(count):
                                    send(2, 0, struct.pack('I', n+20)+string('wl_output')+struct.pack('I', version))
                            elif obj == 2 and op == 0:
                                assert len(b) == 28 and b[8:18] == b'wl_output\0'
                                assert struct.unpack_from('I', b, 20)[0] == 4
                                bound.append(new_id)
                            elif obj == 1 and op == 0:
                                assert len(b) == 4
                                syncs += 1
                                if syncs == 2:
                                    assert len(bound) == count
                                    if fault == 'stall':
                                        # Wait for deadline disconnect, not a sleeping/leaked worker.
                                        self_closed = c.recv(1); assert self_closed == b''; return
                                    if fault == 'remove': send(2, 1, struct.pack('I', 20))
                                    for n, oid in enumerate(bound):
                                        if fault != 'missing':
                                            name = 'TEST-0' if fault == 'duplicate' else f'TEST-{n}'
                                            send(oid, 4, string(name))
                                            if fault == 'rename': send(oid, 4, string('CHANGED'))
                                    if fault == 'bad_callback': send(new_id, 0, b''); return
                                send(new_id, 0, struct.pack('I', 0))
                                send(1, 1, struct.pack('I', new_id))
                                if syncs == 2: return
                            else: raise AssertionError((obj, op))
                except (EOFError, BrokenPipeError, ConnectionResetError): pass
                except Exception as e: errors.append(repr(e))
            t = threading.Thread(target=serve); t.start()
            try: yield {'WAYLAND_DISPLAY': str(path)}, ids
            finally:
                t.join(3)
                assert not t.is_alive(), 'wire server leaked'
                assert not errors, errors


class WireOutputs(unittest.TestCase):
    def test_shared_allocator_one_two_and_max_outputs(self):
        for count in (1, 2, 32):
            with self.subTest(count=count), wire_server(count) as (env, ids):
                self.assertEqual(output_names(env), sorted(f'TEST-{n}' for n in range(count)))
                self.assertEqual(ids, list(range(2, count+5)))

    def test_error_object_code_and_bounded_safe_message(self):
        with wire_server(fault='error') as (env, _):
            with self.assertRaisesRegex(ValueError, r'wayland_error object=2 code=0 message=Invalid new_id: 10\.'):
                output_names(env)
        with wire_server(fault='c_error') as (env, _):
            with self.assertRaisesRegex(ValueError, r'object=1 code=1 message=invalid arguments for wl_registry#2.bind'):
                output_names(env)
        with wire_server(fault='private_error') as (env, _):
            with self.assertRaises(ValueError) as caught: output_names(env)
            message = str(caught.exception)
            self.assertIn('object=2 code=3', message)
            self.assertIn('redacted', message)
            self.assertNotIn('private', message)
            self.assertLess(len(message), 200)
        with wire_server(fault='malformed_error') as (env, _):
            with self.assertRaisesRegex(ValueError, 'wayland_string'): output_names(env)

    def test_refusals_preserve_output_truth(self):
        for count, version, fault, reason in (
            (0,4,None,'output_names'), (33,4,None,'output_scope'), (1,3,None,'output_scope'),
            (1,4,'remove','outputs_changed'), (1,4,'rename','name_changed'),
            (2,4,'duplicate','output_names'), (1,4,'missing','output_names'),
            (1,4,'bad_callback','wayland_callback'),
        ):
            with self.subTest(fault=fault,count=count,version=version), wire_server(count,version,fault) as (env,_):
                with self.assertRaisesRegex(ValueError, reason): output_names(env)

    def test_deadline_and_inherited_socket_unconsumed(self):
        a, b = socket.socketpair()
        self.addCleanup(a.close); self.addCleanup(b.close)
        a.sendall(b'untouched')
        with wire_server(fault='stall') as (env,_):
            env['WAYLAND_SOCKET'] = str(b.fileno())
            start = time.monotonic()
            with self.assertRaises((OSError, ValueError)): output_names(env)
            self.assertLess(time.monotonic()-start, 1.5)
        b.settimeout(.1)
        self.assertEqual(b.recv(9), b'untouched')
        with wire_server() as (env,_):
            env['WAYLAND_SOCKET'] = str(b.fileno())
            self.assertEqual(output_names(env), ['TEST-0'])
        a.sendall(b'ok'); self.assertEqual(b.recv(2), b'ok')


class LibwaylandOutputs(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which('cc') or not shutil.which('pkg-config'):
            raise unittest.SkipTest('isolated C server requires cc and libwayland-server development files')
        flags = subprocess.run(['pkg-config','--cflags','--libs','wayland-server'], capture_output=True, text=True)
        if flags.returncode: raise unittest.SkipTest('libwayland-server development files unavailable')
        cls.tmp = tempfile.TemporaryDirectory(); cls.addClassCleanup(cls.tmp.cleanup)
        cls.binary = Path(cls.tmp.name)/'output-server'
        subprocess.run(['cc','-Wall','-Wextra','-Werror', str(Path(__file__).parent/'fixtures/wayland-output-server.c'),
                        '-o',str(cls.binary), *flags.stdout.split()], check=True, capture_output=True, timeout=10)

    def test_actual_server_registry_bind_sync_and_disconnect(self):
        for count in (1,2,32):
            with self.subTest(count=count), tempfile.TemporaryDirectory() as tmp, socket.socket(socket.AF_UNIX) as listener:
                path=Path(tmp)/'display'; listener.bind(str(path)); listener.listen(8)
                p=subprocess.Popen([str(self.binary),str(listener.fileno()),str(count)], pass_fds=(listener.fileno(),),
                                   stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                try:
                    self.assertTrue(select.select([p.stdout],[],[],2)[0])
                    self.assertEqual(p.stdout.readline(),b'ready\n')
                    # Multiple separate connections exercise disconnect/object cleanup.
                    for _ in range(2):
                        self.assertEqual(output_names({'WAYLAND_DISPLAY':str(path)}), sorted(f'TEST-{n}' for n in range(1,count+1)))
                finally:
                    p.terminate()
                    out,err=p.communicate(timeout=2)
                self.assertEqual(err,b'')


if __name__ == '__main__': unittest.main()

