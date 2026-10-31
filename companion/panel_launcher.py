#!/usr/bin/env python3
"""COSMIC session panel selector. One host at a time; stock on incompatibility."""
import collections
import errno
import fcntl
import hashlib
import json
import os
import re
from pathlib import Path
import signal
import socket
import subprocess
import struct
import sys
import threading
import time

STOCK = Path('/usr/bin/cosmic-panel')
PACKAGES = {
    'cosmic-panel': '1.11.0~1792526322~24.04~ca20ce3',
    'cosmic-comp': '0.1~1792508925~24.04~8cb1802',
    'cosmic-session': '1.11.0~1792525615~24.04~4b0a0a2',
}
# Whole combinations, never a cross-product of individually supported packages.
# Add a combination only after native validation with this companion revision.
SUPPORTED_SYSTEMS = (
    PACKAGES,
    {
        'cosmic-panel': '1.10.0~1791315703~24.04~3ce6ce1',
        'cosmic-comp': '0.1~1791979042~24.04~742b800',
        'cosmic-session': '1.10.0~1791314577~24.04~8093b59',
    },
)


# A package rebuild may change timestamps/distribution versions without changing
# native source. Preserve whole reviewed triples, not a per-package wildcard.
SOURCE_POLICY = 'reviewed-source-v1'

def source_revision(name, version):
    if not isinstance(version, str):
        return None
    match = re.fullmatch(r'(\d+(?:\.\d+)*)~\d+~[0-9.]+~([0-9a-f]{7,40})', version)
    if not match:
        return None
    release = tuple(int(n) for n in match[1].split('.'))
    if name != 'cosmic-comp' and release < (1, 10):
        return None
    return match[2]


def supported_system(versions):
    if not isinstance(versions, dict) or versions.keys() != PACKAGES.keys():
        return False
    if any(versions == system for system in SUPPORTED_SYSTEMS):
        return True
    sources = {name: source_revision(name, version) for name, version in versions.items()}
    return all(sources.values()) and any(
        sources == {name: source_revision(name, version) for name, version in system.items()}
        for system in SUPPORTED_SYSTEMS)


def state_directory(home=None):
    """Resolve the same persistent failure state for management and launch."""
    return Path(os.environ.get('XDG_STATE_HOME') or (home or Path.home()) / '.local/state') / 'desktop-cards-panel'


def digest(path):
    h=hashlib.sha256()
    with open(path,'rb') as f:
        for part in iter(lambda:f.read(1024*1024),b''):h.update(part)
    return h.hexdigest()

def installed_versions():
    """Keep partial diagnostics when one package is missing or cannot be read."""
    from runtime_identity import rpm_system, package_version
    if rpm_system():
        return {name: package_version(name) for name in PACKAGES}
    versions = {}
    for name in PACKAGES:
        try:
            value = subprocess.check_output(
                ['dpkg-query', '-W', '-f=${Version}', name], text=True,
                stderr=subprocess.DEVNULL, timeout=5).strip()
            versions[name] = value or None
        except (OSError, ValueError, subprocess.SubprocessError):
            versions[name] = None
    return versions

def compatible(bundle, versions=None, manifest_name='manifest.json'):
    try:
        manifest=json.loads((bundle/manifest_name).read_text())
        if isinstance(manifest, dict) and manifest.get('version') == 2:
            from verification import bounded
            def inspect():
                from bundle_contract import verify_bundle, classify_compatibility, VerificationContext
                from runtime_identity import installed_identity, running_identity, host_identity
                with VerificationContext() as context:
                    captured, catalog = verify_bundle(bundle, manifest_name, context=context)
                    from runtime_compatibility import enabled, Session
                    if enabled(catalog):
                        owner = host_identity(captured['binary_sha256'])
                        if owner is None: return False
                        session = Session()
                        try:
                            ready = session.inventory(owner)
                            context.check()
                            return ready
                        finally: session.close()
                    installed = installed_identity(catalog)
                    running = running_identity(catalog, host=host_identity(captured['binary_sha256']))
                    context.check()
                    return classify_compatibility(installed, running, {**captured,'verified':True}, catalog)['state'] == 'ready'
            return bounded(inspect,5)
        if not isinstance(manifest, dict):return False
        if manifest['upstream']!='ca20ce3185b3b4404f429d59d429c40b58a2ddef' or manifest['protocol']!=1:return False
        if digest(bundle/'cosmic-panel-islands')!=manifest['binary_sha256']:return False
        if versions is None:
            # New launches require v2 measured cohort admission. Legacy source
            # metadata remains readable for migration/explicit diagnostics only.
            return False
        return supported_system(versions) and (
            any(versions == row for row in SUPPORTED_SYSTEMS)
            or manifest.get('runtime_probe') == 1)
    except (OSError,ValueError,KeyError,TypeError,AttributeError,subprocess.SubprocessError):return False

def _prepare_launch(bundle, context=None):
    """Return a postlaunch guard only for verified prelaunch eligibility.

    Freeze the exact bundle, installed lock and permitted profiles for this one
    launch. Eligibility is not a running-ready measurement. No host is started
    here; the supervisor supplies its actual child to the returned guard.
    """
    installed_files = configuration = None
    handed_off = False
    try:
        # The manual installer copies this launcher without generation helpers.
        # Decline legacy/invalid bundles before importing optional modules. Never
        # restore unsafe version-only admission for that standalone shape.
        header = json.loads((bundle/'manifest.json').read_bytes())
        if header.get('format') != 'halcyon-islands-bundle' or header.get('version') != 2:
            return None
        from bundle_contract import verify_bundle, classify_compatibility, VerificationContext
        from runtime_identity import installed_identity, running_identity, host_process
        from panel_configuration import Snapshot, policy
        context = context or VerificationContext()
        manifest, catalog = verify_bundle(bundle, context=context)
        if catalog['version'] != 2:
            context.close(); return None
        manifest = {**manifest, 'verified': True}
        from runtime_compatibility import enabled, prepare
        if enabled(catalog):
            guard = prepare(manifest, catalog, context)
            handed_off = guard is not None
            return guard
        from runtime_target import installed_matches
        from runtime_identity import FileMeasurement
        installed_files = FileMeasurement()
        installed = installed_identity(catalog, measurement=installed_files)
        target = next((row for row in catalog['cohorts'] if installed_matches(installed,row)), None)
        if target is None:
            installed_files.close(); context.close(); return None
        configuration = Snapshot(policy(target['runtime'])) if target['runtime'].get('configuration') else None
        prelaunch = running_identity(catalog, phase='prelaunch', configuration=configuration)
        state = classify_compatibility(installed, prelaunch, manifest, catalog, phase='prelaunch')
        if state['state'] != 'eligible':
            if configuration: configuration.close()
            installed_files.close(); context.close(); return None
        eligible = set(prelaunch['profiles']).intersection(b['id'] for b in target['runtime_compatibility'])
        owner = None
        startup_complete = False

        def admitted(child):
            nonlocal owner
            try:
                if child.poll() is not None:
                    return False
                current = host_process(child.pid, manifest['binary_sha256'])
                if owner is None:
                    owner = current
                if current != owner:
                    return False
                context.check(); installed_files.check()
                running = running_identity(catalog, phase='running', host=owner, configuration=configuration)
                ready = classify_compatibility(installed, running, manifest, catalog)['state'] == 'ready'
                context.check(); installed_files.check()
                return ready and bool(eligible.intersection(running['profiles'])) and child.poll() is None
            except (OSError, ValueError, KeyError, TypeError, IndexError):
                return False
        def accepted():
            nonlocal startup_complete
            startup_complete = True
            if configuration: configuration.close()
        admitted.accepted = accepted
        def invalidated():
            try: context.check(); installed_files.check()
            except (OSError,ValueError): return True
            return configuration.watch.changed() if configuration else False
        admitted.invalidated = invalidated
        admitted.configuration = configuration
        admitted.changed = lambda: not startup_complete and configuration is not None and configuration.changed()
        def close():
            if configuration: configuration.close()
            installed_files.close(); context.close()
        admitted.close = close
        handed_off = True
        return admitted
    except (ImportError, OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.SubprocessError) as error:
        code = str(error)
        # Never print desktop commands, private paths or arbitrary exception text.
        if not re.fullmatch(r'[a-z_]{1,80}', code): code = 'startup_check_failed'
        sys.stderr.write(f'Islands startup check failed: {code}\n')
        return None
    finally:
        if not handed_off:
            if configuration: configuration.close()
            if installed_files: installed_files.close()
            if context: context.close()


def prepare_launch(bundle, deadline=None):
    """Five seconds covers capture, relevant identity and the ABI probe."""
    try: from verification import Admission
    except ImportError: return None
    deadline = time.monotonic()+5 if deadline is None else deadline
    def factory():
        guard = _prepare_launch(bundle)
        if guard is not None and not runtime_compatible(bundle, timeout=max(.001,deadline-time.monotonic())):
            guard.close(); return None
        return guard
    try: return Admission(factory, deadline)
    except (OSError, ValueError, TimeoutError): return None


def runtime_compatible(bundle, timeout=5):
    """Execute the checksummed binary with no inherited session descriptors.

    This loads its native ABI and checks advertised globals on a fresh named
    Wayland connection. No surface, applet, preference or notification stream.
    A timeout, loader failure, missing capability or malformed reply picks stock.
    Legacy exact-version bundles retain their existing supervised startup.
    """
    try:
        manifest = json.loads((bundle / 'manifest.json').read_text())
        if not isinstance(manifest, dict):
            return False
        if manifest.get('runtime_probe') != 1:
            versions = installed_versions()
            return any(versions == row for row in SUPPORTED_SYSTEMS)
        env = {k: v for k, v in os.environ.items()
               if k not in ('WAYLAND_SOCKET', 'PANEL_NOTIFICATIONS_FD',
                            'DAEMON_NOTIFICATIONS_FD', 'COSMIC_SESSION_SOCK')}
        from verification import probe_parent_death
        result = subprocess.run([str(bundle / 'cosmic-panel-islands'), '--islands-probe'],
                                env=env, capture_output=True, text=True, timeout=timeout,
                                close_fds=True, preexec_fn=probe_parent_death())
        report = json.loads(result.stdout)
        return (result.returncode == 0 and isinstance(report, dict)
                and report.get('probe') == 1 and report.get('ready') is True)
    except (OSError, ValueError, TypeError, subprocess.SubprocessError):
        return False


def session_identity(env=None):
    env = os.environ if env is None else env
    session = env.get('XDG_SESSION_ID')
    if not session:
        return None
    try:
        boot = Path('/proc/sys/kernel/random/boot_id').read_text().strip()
    except OSError:
        return None
    return boot + ':' + session

def display_identity(env):
    display=env.get('WAYLAND_DISPLAY','wayland-0')
    return display if display.startswith('/') else str(Path(env.get('XDG_RUNTIME_DIR',''))/display)

def compositor_gone():
    """A vanished compositor is session teardown, not a panel crash.

    Probe a separate named socket, never the inherited WAYLAND_SOCKET or
    notifications stream. Unknown/access-denied states retain crash handling.
    """
    display = os.environ.get('WAYLAND_DISPLAY')
    if not display:
        return False
    if not display.startswith('/'):
        runtime = os.environ.get('XDG_RUNTIME_DIR')
        if not runtime:
            return False
        display = str(Path(runtime) / display)
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as probe:
            probe.settimeout(0.2)
            probe.connect(display)
        return False
    except OSError as error:
        return error.errno in (errno.ENOENT, errno.ECONNREFUSED)


def other_host():
    wanted=display_identity(os.environ)
    for process in Path('/proc').iterdir():
        if not process.name.isdigit() or int(process.name)==os.getpid():continue
        try:
            if process.stat().st_uid!=os.getuid():continue
            if (process/'exe').resolve().name.removesuffix(' (deleted)') not in ('cosmic-panel','cosmic-panel-islands'):continue
            env=dict(item.split('=',1) for item in (process/'environ').read_bytes().decode().split('\0') if '=' in item)
            if display_identity(env)==wanted:return int(process.name)
        except (OSError,UnicodeError):continue
    return None

# A panel whose compositor connection failed can keep running, reporting the
# same protocol error in a loop with no panel or dock on screen. This many
# reports within WEDGED_SECONDS means the host is gone.
WEDGED_ERRORS = 200
WEDGED_SECONDS = 10
# Repeats of one output line passed to the journal before they are counted.
REPEATS_SHOWN = 20
# Waits before restarting a failed host, by consecutive failure. A host that
# ran HEALTHY_SECONDS starts the sequence again.
RESTART_DELAYS = (1, 2, 5, 10, 30, 60)
HEALTHY_SECONDS = 300
READY_SECONDS = 15


# Cleanup is separate from the unchanged startup/readiness deadlines.
GROUP_TERM_SECONDS = .3
GROUP_CLEANUP_SECONDS = 1.5


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
            start_new_session=True, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        self.pid = self.process.pid
        self.stdout = self.process.stdout
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
                self.stdout.close()
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


def watch_output(stream, child, out, wedged):
    """Pass the host's output on, collapsing floods, and stop a wedged host."""
    last, repeats, errors = None, 0, collections.deque(maxlen=WEDGED_ERRORS)
    for line in stream:
        if b'Protocol error ' in line:
            errors.append(time.monotonic())
            if (len(errors) == WEDGED_ERRORS and errors[-1] - errors[0] < WEDGED_SECONDS
                    and not wedged.is_set()):
                wedged.set()
                out.write(b'cosmic-panel stopped responding to the compositor; restarting it\n')
                try:
                    child.signal_group(signal.SIGKILL)
                except ProcessLookupError:
                    pass
        if line == last:
            repeats += 1
            if repeats > REPEATS_SHOWN:
                continue
        else:
            if repeats > REPEATS_SHOWN:
                out.write(b'(previous line repeated %d more times)\n' % (repeats - REPEATS_SHOWN))
            last, repeats = line, 0
        out.write(line)
        out.flush()
    if repeats > REPEATS_SHOWN:
        out.write(b'(previous line repeated %d more times)\n' % (repeats - REPEATS_SHOWN))
        out.flush()


def wait_ready(child, done, wedged, admission=None):
    """Require child-owned IPC AND its declared runtime before readiness."""
    deadline = time.monotonic() + READY_SECONDS
    runtime = os.environ.get('XDG_RUNTIME_DIR')
    from verification import Admission, Worker
    measurement = None
    while runtime and time.monotonic() < deadline and not done.is_set():
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.settimeout(min(.5, max(.01, deadline - time.monotonic())))
                client.connect(str(Path(runtime) / 'desktop-cards-islands-v1.sock'))
                pid, uid, _ = struct.unpack('3i', client.getsockopt(
                    socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize('3i')))
                if pid != child.pid or uid != os.getuid():
                    raise ValueError('wrong IPC owner')
                client.sendall(b'"Capabilities"\n')
                with client.makefile('rb') as stream:
                    line = stream.readline(65537)
                if len(line) > 65536 or not line.endswith(b'\n'):
                    raise ValueError('invalid IPC reply')
                cap = json.loads(line).get('Capabilities', {})
                if cap.get('protocol') == 1 and cap.get('together') is True and cap.get('separate') is True:
                    admitted = admission is None
                    if admission is not None:
                        # File/procfs IO cannot extend the readiness deadline.
                        # At most one read-only measurement is in flight. A late
                        # result has no side effects and is discarded on teardown.
                        if measurement is None:
                            if isinstance(admission, Admission):
                                admission.begin(child); measurement = admission
                            else:
                                measurement = Worker(lambda: admission(child) is True)
                        value, complete = measurement.poll()
                        if complete:
                            admitted = value is True
                            if isinstance(measurement, Worker): measurement.close()
                            measurement = None
                    if admitted and not done.is_set() and child.poll() is None and time.monotonic() < deadline:
                        # Recheck the configuration watch at the acceptance
                        # boundary; IPC polling must not admit a stale result.
                        if admission is not None and getattr(admission, 'invalidated', lambda: False)():
                            break
                        if admission is not None: getattr(admission, 'accepted', lambda: None)()
                        return True
        except (OSError, ValueError, AttributeError, TypeError, TimeoutError):
            if isinstance(measurement, Worker): measurement.close()
            measurement = None
        if done.wait(.1):
            if isinstance(measurement, Worker): measurement.close()
            return False
    if isinstance(measurement, Worker): measurement.close()
    if not done.is_set() and child.poll() is None:
        wedged.set()
        sys.stderr.write('Islands startup IPC/runtime readiness failed; stopping owned host\n')
        try:
            child.signal_group(signal.SIGKILL)
        except ProcessLookupError:
            pass
    return False


def run_host(binary, pass_fds=(), env=None, readiness=False, admission=None):
    """Keep supervision descriptors in this process, outside native applets.

    Returns the exit code, whether a signal asked us to stop, and whether the
    host was stopped because it wedged."""
    child = OwnedHost(binary, pass_fds, env)
    stopping = False
    wedged = threading.Event()
    done = threading.Event()
    observer_errors = []

    def observe(function, *args):
        try:
            function(*args)
        except BaseException as error:
            # A daemon thread cannot propagate to run_host by raising alone.
            # Wake its main wait and retain the failure through cleanup; never
            # silently turn an observer failure into stock/return75 success.
            observer_errors.append(error)
            done.set()

    watcher = threading.Thread(target=observe, args=(watch_output, child.stdout, child, sys.stderr.buffer, wedged), daemon=True)
    watcher.start()
    readiness_thread = None
    if readiness:
        readiness_thread = threading.Thread(target=observe, args=(wait_ready, child, done, wedged, admission), daemon=True)
        readiness_thread.start()

    def terminate(signum, frame):
        nonlocal stopping
        stopping = True
        try:
            child.signal_group(signum)
        except ProcessLookupError:
            pass

    previous = {s: signal.signal(s, terminate) for s in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
    try:
        while not done.is_set() and child.poll() is None and not stopping:
            done.wait(.02)
    finally:
        done.set()
        try:
            code = child.finish()
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)
            if readiness_thread is not None:
                readiness_thread.join(2)
            # Applets share the pipe; group exit releases their descriptors.
            watcher.join(2)
            if not watcher.is_alive(): child.stdout.close()
    if observer_errors:
        raise GroupCleanupError('host_observer_failed') from observer_errors[0]
    return code, stopping, wedged.is_set()


def pause(seconds):
    """Sleep between restarts; True when a signal asked us to stop instead."""
    stop = threading.Event()
    previous = {s: signal.signal(s, lambda signum, frame: stop.set()) for s in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
    try:
        stop.wait(seconds)
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
    return stop.is_set()


def retire_session_streams(pass_fds, env=None):
    """A completed host consumed these streams; the next host connects afresh."""
    environment = dict(os.environ if env is None else env)
    retired = set()
    for name in ('PANEL_NOTIFICATIONS_FD', 'WAYLAND_SOCKET'):
        value = environment.pop(name, None)
        try:
            descriptor = int(value)
        except (TypeError, ValueError):
            continue
        if descriptor in pass_fds:
            retired.add(descriptor)
    for descriptor in retired:
        try:
            os.close(descriptor)
        except OSError:
            pass
    return tuple(fd for fd in pass_fds if fd not in retired), environment


def stock_host(binary, pass_fds=(), limit=None, env=None, keep_running=False):
    """Run the stock host, restarting it here when it fails.

    cosmic-session's own restart delay doubles with every failure and never
    resets, reaching hours, so a failure is not handed back to it. A restarted
    host cannot reuse either inherited session stream. It opens a fresh
    WAYLAND_DISPLAY connection and runs without notifications until login."""
    failures, runs = 0, 0
    while True:
        started = time.monotonic()
        code, stopping, wedged = run_host(binary, pass_fds, env)
        runs += 1
        if stopping or compositor_gone():
            return 0
        if code == 0 and not wedged and not keep_running:
            return 0
        if limit is not None and runs >= limit:
            return code if code >= 0 else 128 - code
        if time.monotonic() - started >= HEALTHY_SECONDS:
            failures = 0
        delay = RESTART_DELAYS[min(failures, len(RESTART_DELAYS) - 1)]
        failures += 1
        sys.stderr.write('cosmic-panel exited (%s); restarting in %ds\n' % ('wedged' if wedged else code, delay))
        sys.stderr.flush()
        pass_fds, env = retire_session_streams(pass_fds, env)
        if pause(delay):
            return 0


def supervise(binary, fallback, pass_fds=(), failed=None, session_socket=False, admission=None):
    """Reap the selected host before starting stock or returning to the session."""
    started = True
    try:
        _, stopping, _ = run_host(binary, pass_fds, readiness=True, admission=admission)
    except OSError:
        started, stopping = False, False
    if stopping or (started and compositor_gone()):
        return 0
    persisted = False
    if failed:
        try:
            failed()
            persisted = True
        except Exception:
            pass
    if session_socket and started and persisted:
        # cosmic-session recreates the panel/notifications socket pair when
        # its supervised process exits. Reusing an authenticated stream in
        # a new panel would break notifications. The marker selects stock
        # on that restart, which receives the fresh socket from the session.
        return 75
    env = None
    if started:
        pass_fds, env = retire_session_streams(pass_fds)
    if not persisted:
        # Without a marker, keep the stock decision and lock in this process
        # until logout. A consumed notification stream cannot be reused.
        return stock_host(fallback, pass_fds, env=env, keep_running=True)
    return stock_host(fallback, pass_fds, env=env)

def main(bundle=None, pointer=None, deadline=None):
    data=Path(os.environ.get('XDG_DATA_HOME',Path.home()/'.local/share'))
    state=state_directory()
    bundle=bundle or data/'cosmic-desktop-cards/companion'
    selection=data/'cosmic-desktop-cards/companion/selection.json'
    try:
        selected=json.loads(selection.read_text())
        enabled=isinstance(selected, dict) and selected.get('enabled') is True
        if enabled and selected.get('defer_session') is not None:
            current=session_identity()
            enabled=current is not None and current != selected['defer_session']
    except (OSError,ValueError):enabled=False
    if enabled and pointer is not None:
        current=session_identity()
        enabled=current is not None and current != pointer.get('defer_session')
    # A manual launch must never duplicate the session's existing host.
    if other_host() is not None:return 0
    runtime=Path(os.environ['XDG_RUNTIME_DIR']);lockpath=runtime/'desktop-cards-panel.lock'
    fd=os.open(lockpath,os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
    try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
    except BlockingIOError:return 0
    # The supervisor owns the lock. Passing it into a host also lets native
    # applets inherit it, which can block the next host after a shutdown.
    inherited=[]
    for name in ('PANEL_NOTIFICATIONS_FD','WAYLAND_SOCKET'):
        try:
            value=int(os.environ[name]);os.fstat(value);inherited.append(value);os.set_inheritable(value,True)
        except (KeyError,ValueError,OSError):pass
    transient = None
    try: transient = (state/'fallback-session').read_text()
    except OSError: pass
    if not enabled or (state/'failed').exists() or (transient is not None and transient == session_identity()):
        return stock_host(STOCK, tuple(inherited))
    admission = prepare_launch(bundle) if deadline is None else prepare_launch(bundle, deadline=deadline)
    if admission is None:
        return stock_host(STOCK, tuple(inherited))
    def failed():
        state.mkdir(parents=True,exist_ok=True,mode=0o700)
        if getattr(admission, 'changed', lambda: False)():
            # A changing layout/output inventory is not a persistent crash.
            # Scope fallback to this boot/login so cosmic-session can recreate
            # notification descriptors without retrying Islands in a loop.
            session = session_identity()
            if session is None: raise ValueError('session_unknown')
            (state/'fallback-session').write_text(session)
        else:
            (state/'failed').write_text('The companion exited unexpectedly. Stock panel selected.\n')
    try:
        return supervise(bundle/'cosmic-panel-islands',STOCK,tuple(inherited),failed,'PANEL_NOTIFICATIONS_FD' in os.environ,admission=admission)
    finally:
        getattr(admission, 'close', lambda: None)()

if __name__=='__main__':sys.exit(main())
