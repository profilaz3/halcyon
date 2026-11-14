"""The supervisors block while the host runs instead of waking every 20 ms."""
import os
import signal
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import panel_launcher
import selector


def host(root, seconds):
    script = Path(root) / 'host'
    script.write_text(f'#!/bin/sh\nexec sleep {seconds}\n')
    script.chmod(0o755)
    return script


def soon(seconds, action):
    timer = threading.Timer(seconds, action)
    timer.start()
    return timer


def terminate_once_handled():
    """SIGTERM to this process once a supervisor handles it, never before."""
    def send():
        deadline = time.monotonic() + 10
        while signal.getsignal(signal.SIGTERM) in (signal.SIG_DFL, None):
            if time.monotonic() > deadline: return
            time.sleep(.01)
        time.sleep(.2)
        os.kill(os.getpid(), signal.SIGTERM)
    thread = threading.Thread(target=send)
    thread.start()
    return thread


class LauncherWakeups(unittest.TestCase):
    def test_a_running_host_costs_a_few_wakes_not_fifty_a_second(self):
        with tempfile.TemporaryDirectory() as root:
            before = panel_launcher.Wakeup.waits
            started = time.monotonic()
            code, stopping, wedged = panel_launcher.run_host(host(root, 1))
            elapsed = time.monotonic() - started
            waits = panel_launcher.Wakeup.waits - before
        self.assertEqual((code, stopping, wedged), (0, False, False))
        self.assertGreaterEqual(elapsed, .9)
        # The 20 ms poll woke about 50 times in this second.
        self.assertLessEqual(waits, 3, waits)

    def test_a_signal_still_stops_the_host_at_once(self):
        with tempfile.TemporaryDirectory() as root:
            sender = terminate_once_handled()
            started = time.monotonic()
            code, stopping, _ = panel_launcher.run_host(host(root, 30))
            elapsed = time.monotonic() - started
            sender.join()
        self.assertTrue(stopping)
        self.assertEqual(code, -signal.SIGTERM)
        self.assertLess(elapsed, 5)
        # Handlers and the wakeup descriptor are back as they were.
        self.assertEqual(signal.getsignal(signal.SIGTERM), signal.SIG_DFL)
        self.assertEqual(signal.set_wakeup_fd(-1), -1)

    def test_an_event_set_by_another_thread_wakes_the_wait(self):
        with tempfile.TemporaryDirectory() as root:
            child = panel_launcher.OwnedHost(host(root, 30), (), None)
            wakeup = panel_launcher.Wakeup(child.pid)
            done = panel_launcher.WakingEvent(wakeup)
            try:
                timer = soon(.2, done.set)
                started = time.monotonic()
                wakeup.wait(lambda: done.wait(.02))
                self.assertTrue(done.is_set())
                self.assertLess(time.monotonic() - started, 5)
                timer.join()
            finally:
                wakeup.close()
                child.signal_group(signal.SIGKILL)
                child.finish()
                child.stdout.close()
            # A late set() after close is harmless.
            done.set()

    def test_without_pidfds_the_old_poll_remains(self):
        with tempfile.TemporaryDirectory() as root, \
                patch.object(panel_launcher.os, 'pidfd_open', side_effect=OSError('unsupported')):
            before = panel_launcher.Wakeup.waits
            code, _, _ = panel_launcher.run_host(host(root, .3))
            waits = panel_launcher.Wakeup.waits - before
        self.assertEqual(code, 0)
        self.assertGreater(waits, 5)


class SelectorWakeups(unittest.TestCase):
    def fallback(self, root, seconds):
        script = host(root, seconds)
        runtime = Path(root) / 'runtime'
        runtime.mkdir()
        real_popen = selector.subprocess.Popen

        def spawn(args, **kwargs):
            assert args == ['/usr/bin/cosmic-panel']
            return real_popen([str(script)], **kwargs)
        environment = {'XDG_RUNTIME_DIR': str(runtime), 'WAYLAND_DISPLAY': f'wakeups-{os.getpid()}'}
        with patch.dict(os.environ, environment), patch.object(selector.subprocess, 'Popen', spawn):
            return selector.fallback()

    def test_the_fallback_host_costs_a_few_wakes(self):
        with tempfile.TemporaryDirectory() as root:
            before = selector.Wakeup.waits
            code = self.fallback(root, 1)
            waits = selector.Wakeup.waits - before
        self.assertEqual(code, 0)
        self.assertLessEqual(waits, 3, waits)

    def test_a_signal_still_stops_the_fallback_host(self):
        with tempfile.TemporaryDirectory() as root:
            sender = terminate_once_handled()
            started = time.monotonic()
            self.fallback(root, 30)
            elapsed = time.monotonic() - started
            sender.join()
        self.assertLess(elapsed, 5)
        self.assertEqual(signal.getsignal(signal.SIGTERM), signal.SIG_DFL)


if __name__ == '__main__':
    unittest.main()
