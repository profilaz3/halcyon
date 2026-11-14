#!/usr/bin/python3
"""Stable v1 selector: resolve one generation; preserve session descriptors."""
import hashlib
import fcntl
import signal
import stat
import subprocess
import json
import os
from pathlib import Path
import runpy
import select
import sys
import time
import threading

sys.dont_write_bytecode = True


# Trusted bootstrap copy of the launcher ownership guard: fallback must not
# import any helper from the generation whose verification just failed.
# Cleanup is separate from the unchanged startup/readiness deadlines.
GROUP_TERM_SECONDS = .3
GROUP_CLEANUP_SECONDS = 1.5


class Wakeup:
    """Blocks until the host's leader exits, a signal arrives or wake() runs.

    The supervisor slept in 20 ms steps to notice these, about 50 wakes a
    second for the whole session. A pidfd turns readable when the leader
    exits (it stays unreaped, as poll() needs); signal.set_wakeup_fd writes
    a byte for each signal; wake() writes one from any thread. Without
    pidfds (Python before 3.9, Linux before 5.3) wait() keeps the old poll.
    """
    waits = 0  # Every wait() call, for tests.

    def __init__(self, pid):
        self.lock = threading.Lock()
        self.closed = False
        self.pidfd = self.read = self.write = None
        self.previous = None
        try:
            self.pidfd = os.pidfd_open(pid)
            self.read, self.write = os.pipe2(os.O_NONBLOCK | os.O_CLOEXEC)
        except (AttributeError, OSError):
            self.close()

    def arm(self):
        """Route signals to the pipe; main thread only, as signal handlers."""
        if self.write is not None and self.previous is None:
            self.previous = signal.set_wakeup_fd(self.write, warn_on_full_buffer=False)

    def wake(self):
        with self.lock:
            if self.closed or self.write is None: return
            try: os.write(self.write, b'\0')
            except BlockingIOError: pass  # Already pending.

    def wait(self, poll):
        Wakeup.waits += 1
        if self.pidfd is None:
            poll()
            return
        select.select([self.pidfd, self.read], [], [])
        try:
            while os.read(self.read, 512): pass
        except BlockingIOError: pass

    def close(self):
        with self.lock:
            if self.closed: return
            self.closed = True
            if self.previous is not None:
                signal.set_wakeup_fd(self.previous)
            for fd in (self.pidfd, self.read, self.write):
                if fd is not None: os.close(fd)


class GroupCleanupError(RuntimeError):
    """Do not launch stock or report a completed handoff with live owned actors."""


class OwnedHost:
    """One new session/group, anchored by its unreaped direct-child leader.

    poll() uses WNOWAIT: readiness/output threads can observe exit but cannot
    release the leader PID for reuse. Only finish() reaps, after group cleanup.
    Outsiders cannot join this new session's process group. No name-based kills,
    adoption of unrelated applets, persistent process tracking or subreaper role.
    """
    def __init__(self, binary, pass_fds, env):
        self.process = subprocess.Popen([str(binary)], pass_fds=pass_fds,
            start_new_session=True, env=env)
        self.pid = self.process.pid
        self.lock = threading.RLock()
        self.closed = False
        self.code = None
        self.stat_fd = None
        try:
            # Keep the original proc object open so FD exhaustion cannot prevent
            # guarded termination. The waitable child still pins PID/PGID/SID.
            self.stat_fd = os.open(f'/proc/{self.pid}/stat', os.O_RDONLY | os.O_CLOEXEC | os.O_NONBLOCK)
            self.identity = self._stat(self.pid, fd=self.stat_fd)[1:]
            if self.identity[:2] != (self.pid, self.pid):
                raise GroupCleanupError('host_session_not_owned')
        except BaseException as e:
            # Popen completed setsid/exec and no observer has received this
            # child yet. Its unreaped identity still pins the new group even
            # when procfs failed; stop it, but do not report successful cleanup.
            try:
                os.waitid(os.P_PID, self.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                os.killpg(self.pid, signal.SIGKILL)
            except OSError: pass
            try: self.process.wait(timeout=GROUP_CLEANUP_SECONDS)
            except subprocess.TimeoutExpired: pass
            finally:
                if self.stat_fd is not None: os.close(self.stat_fd)
            raise GroupCleanupError('host_ownership_setup_failed') from e

    @staticmethod
    def _stat(pid, *, fd=None, threads=False):
        if fd is None:
            owned_fd = os.open(f'/proc/{pid}/stat', os.O_RDONLY | os.O_CLOEXEC | os.O_NONBLOCK)
            try: data = os.read(owned_fd, 8193)
            finally: os.close(owned_fd)
        else:
            data = os.pread(fd, 8193, 0)
        if len(data) > 8192: raise GroupCleanupError('host_stat_limit')
        fields = data.rsplit(b')', 1)[1].split()
        # State, process group, session, birth ticks; comm may contain ')'.
        result = fields[0], int(fields[2]), int(fields[3]), int(fields[19])
        if threads:
            count = int(fields[17])
            if count < 0: raise GroupCleanupError('host_thread_count_invalid')
            return (*result, count)
        return result

    def poll(self):
        with self.lock:
            if self.closed: return self.code
            try:
                status = os.waitid(os.P_PID, self.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
            except OSError as e:
                raise GroupCleanupError('host_leader_ownership_lost') from e
            if status is None: return None
            return status.si_status if status.si_code == os.CLD_EXITED else -status.si_status

    def _check(self):
        self.poll()  # Must still be our waitable child, alive or exited.
        try: same = self._stat(self.pid, fd=self.stat_fd)[1:] == self.identity
        except (OSError, ValueError, IndexError) as e:
            raise GroupCleanupError('host_leader_identity_unavailable') from e
        if not same: raise GroupCleanupError('host_leader_identity_changed')

    def signal_group(self, signum):
        with self.lock:
            if self.closed: return
            self._check()
            # The unreaped leader reserves its PID/PGID/SID through this call.
            try: os.killpg(self.pid, signum)
            except ProcessLookupError: pass
            except OSError as e:
                raise GroupCleanupError('host_group_signal_failed') from e

    def live_members(self, deadline):
        self._check()
        live = []
        with os.scandir('/proc') as entries:
            for index, entry in enumerate(entries):
                if index >= 65536 or time.monotonic() >= deadline:
                    raise GroupCleanupError('host_group_scan_limit')
                if not entry.name.isdigit(): continue
                try:
                    # Public stat only: never unrelated executable/env/maps IO.
                    state, group, session, start, threads = self._stat(int(entry.name), threads=True)
                except (FileNotFoundError, ProcessLookupError): continue
                except (OSError, ValueError, IndexError) as e:
                    raise GroupCleanupError('host_group_scan_incomplete') from e
                if group == self.pid:
                    if session != self.pid:
                        raise GroupCleanupError('host_group_session_changed')
                    # A main thread may pthread_exit while workers stay alive.
                    # Top-level Z alone is not group completion. The bounded
                    # stat count conservatively keeps such actors in cleanup.
                    if state not in (b'Z', b'X') or threads > 1:
                        live.append((int(entry.name), start))
        return live

    def finish(self):
        deadline = time.monotonic() + GROUP_CLEANUP_SECONDS
        term_until = time.monotonic() + GROUP_TERM_SECONDS
        try:
            self.signal_group(signal.SIGTERM)
            while True:
                if not self.live_members(deadline) and self.poll() is not None:
                    # /proc enumeration is not atomic: a last fork could be
                    # omitted as its parent exits. Stop the still-pinned group
                    # before the final census, even if the first looked empty.
                    self.signal_group(signal.SIGKILL)
                    if not self.live_members(deadline): break
                if time.monotonic() >= term_until:
                    self.signal_group(signal.SIGKILL)
                if time.monotonic() >= deadline:
                    raise GroupCleanupError('host_group_cleanup_timeout')
                time.sleep(.01)
        except (GroupCleanupError, OSError) as error:
            # A census failure must not leave cooperative cleanup as the only
            # attempt. Escalate only while the original anchor still validates;
            # propagate the failure, never report stock handoff as completed.
            self.signal_group(signal.SIGKILL)
            # Even when census IO is unavailable (e.g. EMFILE), give the exact
            # child bounded time to exit/reap. This does not convert an unknown
            # descendant census into successful handoff.
            while self.poll() is None and time.monotonic() < deadline:
                time.sleep(.01)
            raise GroupCleanupError(str(error) if isinstance(error, GroupCleanupError) else 'host_group_scan_failed') from error
        finally:
            with self.lock:
                # Retire signal authority before reaping; late watcher calls
                # cannot target a subsequently reused PID/group.
                try: self.code = self.poll()
                finally:
                    self.closed = True
                    os.close(self.stat_fd)
                    self.stat_fd = None
                if self.code is not None:
                    self.process.wait()
        return self.code



def display_identity(env):
    # Keep the launcher's socket semantics without trusting generation helpers.
    display = env.get('WAYLAND_DISPLAY', 'wayland-0')
    return display if display.startswith('/') else str(Path(env.get('XDG_RUNTIME_DIR', '')) / display)


def fallback():
    """Fail closed without importing a damaged generation or duplicating a host."""
    runtime = os.environ.get('XDG_RUNTIME_DIR')
    if not runtime:
        return 1
    display = display_identity(os.environ)
    for process in Path('/proc').iterdir():
        if not process.name.isdigit() or int(process.name) == os.getpid():
            continue
        try:
            if process.stat().st_uid != os.getuid():
                continue
            if (process/'exe').resolve().name.removesuffix(' (deleted)') not in ('cosmic-panel','cosmic-panel-islands'):
                continue
            env = dict(item.split('=',1) for item in (process/'environ').read_bytes().decode().split('\0') if '=' in item)
            if display_identity(env) == display:
                return 0
        except (OSError, UnicodeError):
            continue
    fd = os.open(Path(runtime)/'desktop-cards-panel.lock', os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW, 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            return 0
        inherited=[]
        for name in ('PANEL_NOTIFICATIONS_FD','WAYLAND_SOCKET'):
            try:
                value=int(os.environ[name]);os.fstat(value);inherited.append(value)
            except (KeyError,ValueError,OSError):
                pass
        child = OwnedHost('/usr/bin/cosmic-panel', inherited, None)
        wakeup = Wakeup(child.pid)
        stopping = False
        def stop(signum, _):
            nonlocal stopping
            stopping = True
        previous = {sig: signal.signal(sig, stop)
                    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
        try:
            wakeup.arm()
            while not stopping and child.poll() is None:
                wakeup.wait(lambda: time.sleep(.02))
        finally:
            try:
                # Keep both the leader's identity and the panel lock until all
                # owned descendants have exited, escalating after a short grace.
                code = child.finish()
            finally:
                wakeup.close()
                for sig, handler in previous.items():
                    signal.signal(sig, handler)
        return code
    finally:
        os.close(fd)


def captured(path, limit):
    fd=os.open(path,os.O_RDONLY|os.O_NONBLOCK|os.O_NOFOLLOW)
    with os.fdopen(fd,'rb') as stream:
        before=os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size>limit: raise ValueError('unbounded_bootstrap_record')
        data=stream.read(before.st_size+1)
        after=os.fstat(stream.fileno())
        if len(data)!=before.st_size or (before.st_ino,before.st_size,before.st_mtime_ns,before.st_ctime_ns)!=(after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns):
            raise ValueError('bootstrap_changed')
        return data


def main():
    home = Path.home()
    base = home / '.local/share/cosmic-desktop-cards/companion'
    deadline = time.monotonic()+5
    def expired(*_): raise TimeoutError('prelaunch_deadline')
    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, 5)
    try:
        # Verify the generation before importing any user-bundle helper. The
        # manifest's content address comes from the atomically selected record.
        pointer_path = base / 'generation.json'
        if any(p.is_symlink() for p in (pointer_path, *pointer_path.parents)):
            raise ValueError('symlink')
        pointer = json.loads(captured(pointer_path,16*1024*1024))
        generation = pointer['generation']
        if len(generation) != 64 or any(c not in '0123456789abcdef' for c in generation):
            raise ValueError('generation')
        bundle = base / 'generations' / generation
        manifest_path = bundle / 'manifest.json'
        if any(p.is_symlink() for p in (manifest_path, *manifest_path.parents)):
            raise ValueError('symlink')
        raw = captured(manifest_path,16*1024*1024)
        if hashlib.sha256(raw).hexdigest() != generation:
            raise ValueError('manifest')
        manifest = json.loads(raw)
        for name, entry in manifest['files'].items():
            if Path(name).name != name:
                raise ValueError('path')
            path = bundle / name
            if path.is_symlink() or path.stat().st_uid != os.getuid() or path.stat().st_mode & 0o777 != entry['mode']:
                raise ValueError('owner_or_mode')
            if name in ('catalog.json','cosmic-panel-islands'):
                # Data only: the verified helper below bounds, hashes and
                # validates the same opened bytes before refresh or launch.
                # Do not pre-hash an unbounded catalog before that reader.
                continue
            if hashlib.sha256(captured(path,16*1024*1024)).hexdigest() != entry['sha256']:
                raise ValueError('digest')
        sys.path.insert(0, str(bundle))
        # The read-only launch worker captures/validates the catalog once.
        # Staging is the independent package-owned bootstrap's responsibility;
        # never copy an update or wait for a transaction on the panel path.
        import panel_launcher
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
        return panel_launcher.main(bundle=bundle, pointer=pointer, deadline=deadline)
    except (OSError, ValueError, KeyError, TypeError, AttributeError, ImportError):
        # No shell, no descriptor probe/consumption and no unsafe helper import.
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
        return fallback()
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


if __name__ == '__main__':
    sys.exit(main())
