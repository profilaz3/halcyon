"""Supported-family startup, deliberately NOT host attestation or qualification.

Trust the system's installed public executables, pin their objects for this
operation, then require the real ABI probe and host-owned IPC readiness. Historical
exact native evidence stays separate. No GPU, kernel, library or resource scan.
"""
import os
from pathlib import Path
import re
import stat
import subprocess

from bundle_contract import CONFIGURATION_SOURCE, installed_package_status
from panel_configuration import Snapshot, Watch
from runtime_identity import (_proc_bytes, _stat, _owned_by_host, host_process, LOADER_ENV,
                              rpm_system, package_version, watch_package_database)

POLICY = dict(version=4, id='cosmic-1.10-1.11-startup-v4', families=['1.10','1.11'],
    assurance='supported-family-probed-startup-not-hardware-qualification',
    native_source={**CONFIGURATION_SOURCE, 'patch_sha256':'6207734618f4815676fd7e2c0beccfcf094f21170fa1b941dfd9946edf17053f',
                   'islands_sha256':'4428bc01b14ec266af1fd82735b0370a773b82c350ef6ebbedd9c2cec088be6c',
                   'penumbra_patch_sha256':'c65009b2b68854de092ea3f9b728b5ecb5a189490f816b60575916d85f70e9ae'},
    binary_sha256='9133db20d917d5125f9c18e220a4be4424cc8537063c3f202703157d2ba1a9e9',
    protocol=1, runtime_probe=1, max_outputs=32)
# Separate native Fedora bytes. The existing Debian policy and historical
# release evidence remain immutable; this policy grants probed startup only.
FEDORA_POLICY = {**POLICY, 'id':'fedora43-cosmic-1.10-startup-v1', 'families':['1.10'],
    'platform':{'id':'fedora','version':'43','architecture':'x86_64'},
    'binary_sha256':'17ab8522091680f81d2595e3a9c1f26a8a6e043c589b51165ecf203b2e559b8c'}
FEDORA43_POLICY = {**FEDORA_POLICY, 'id':'fedora43-cosmic-1.10-1.11-startup-v2',
    'families':['1.10','1.11']}
FEDORA44_POLICY = {**POLICY, 'id':'fedora44-cosmic-1.11-startup-v1', 'families':['1.11'],
    'platform':{'id':'fedora','version':'44','architecture':'x86_64'},
    'binary_sha256':'070e0684c9cdce957d3033012d8b9fdd1d899d98db68122d4698c42f1b11ac5d'}
POLICIES = (POLICY, FEDORA_POLICY, FEDORA43_POLICY, FEDORA44_POLICY)
CORE = ('cosmic-comp', 'cosmic-session', 'cosmic-settings-daemon')


def enabled(catalog):
    value = catalog.get('startup_policy')
    if value is None: return False
    if value not in POLICIES: raise ValueError('unsupported_startup_policy')
    return True


def validate_bundle(manifest, catalog):
    if not enabled(catalog): raise ValueError('startup_policy_missing')
    if (manifest.get('upstream') != CONFIGURATION_SOURCE['upstream']
            or manifest.get('binary_sha256') != catalog['startup_policy']['binary_sha256']
            or manifest.get('runtime_probe') != 1 or manifest.get('protocol') != 1):
        raise ValueError('startup_native_source_or_interface')


def os_release():
    values = {}
    for line in Path('/etc/os-release').read_text()[:65536].splitlines():
        key, separator, value = line.partition('=')
        if separator: values[key] = value.strip('"')
    return values


def current_policy():
    if not rpm_system(): return POLICY
    release = os_release()
    policies = {'43': FEDORA43_POLICY, '44': FEDORA44_POLICY}
    if release.get('ID') != 'fedora' or release.get('VERSION_ID') not in policies:
        raise ValueError('unsupported_package_platform')
    return policies[release['VERSION_ID']]


def validate_platform(catalog):
    """Platform eligibility is checked at use, not while inspecting a package."""
    if not enabled(catalog): raise ValueError('startup_policy_missing')
    policy = catalog['startup_policy']
    target = policy.get('platform')
    if target:
        import platform
        values = os_release()
        if (values.get('ID') != target['id'] or values.get('VERSION_ID') != target['version']
                or platform.machine() != target['architecture']):
            raise ValueError('unsupported_package_platform')
    elif rpm_system():
        raise ValueError('unsupported_package_platform')


def family(versions=None, policy=None):
    policy = current_policy() if policy is None else policy
    if policy not in POLICIES: raise ValueError('unsupported_startup_policy')
    if versions is None:
        names = ('cosmic-panel','cosmic-session','cosmic-comp','cosmic-settings-daemon')
        if rpm_system():
            versions = {name: package_version(name) for name in names}
            if not all(versions.values()): raise ValueError('family_packages_missing')
        else:
            result = subprocess.run(['/usr/bin/dpkg-query','-W',
                '-f=${Package}\t${Status}\t${Version}\n',*names],
                capture_output=True, text=True, timeout=1)
            if result.returncode or len(result.stdout)>8192: raise ValueError('family_packages_missing')
            versions = {}
            for line in result.stdout.splitlines():
                name, status, version = line.split('\t')
                if not installed_package_status(status) or name in versions: raise ValueError('family_packages_incomplete')
                versions[name] = version
    def release(name):
        value=versions.get(name,'')
        if not re.fullmatch(r'[A-Za-z0-9.+:~_-]{1,128}',value): raise ValueError('family_version_invalid')
        match=re.match(r'^(?:[0-9]+:)?(1\.(?:10|11))(?:\.\d+)?(?:[~+\-].*)?$',value)
        return match[1] if match else None
    selected=release('cosmic-panel')
    if selected not in policy['families'] or release('cosmic-session') != selected:
        raise ValueError('unsupported_cosmic_family')
    if not versions.get('cosmic-comp') or not versions.get('cosmic-settings-daemon'):
        raise ValueError('family_packages_missing')
    return dict(family=selected, package_versions=versions, policy=policy['id'])


def state(observation=None, running=False):
    return dict(state='ready' if running else 'eligible',
        reason='family_ready_hardware_not_qualified' if running else 'family_startup_available',
        cohort=None, qualification='not_measured_for_this_payload_or_hardware',
        policy=(observation or {}).get('policy',POLICY['id']), family=(observation or {}).get('family'))


class Session:
    """One launch/status observation; no acceptance cache or write authority."""
    def __init__(self, versions=None, env=None, root=Path('/'), proc=Path('/proc'), policy=None):
        self.env=dict(os.environ if env is None else env); self.proc=proc
        self.watch=Watch(); self.files={}; self.configuration=None; self.core=None
        try:
            # Watch before querying: a package transaction/ABA during the
            # family read must not bless a later executable generation.
            watch_package_database(self.watch,root)
            self.observation=family(versions, policy=policy)
            if not self.env.get('XDG_SESSION_ID') or any(self.env.get(k) for k in LOADER_ENV):
                raise ValueError('session_or_loader_environment')
            def pin(path, core=False):
                self.watch.add(path, entry=True)
                resolved=path.resolve(strict=True)
                if core and resolved not in {root/'usr/bin'/n for n in CORE}:
                    raise ValueError('executable_outside_supported_contract')
                self.watch.add(resolved)
                fd=os.open(resolved,os.O_RDONLY|os.O_NONBLOCK|os.O_CLOEXEC)
                info=os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or not info.st_mode&0o111:
                    os.close(fd); raise ValueError('executable_invalid')
                if resolved in self.files: os.close(fd)
                else: self.files[resolved]=(fd,self.stamp(info))
                return str(resolved)
            self.processes={n:pin(root/'usr/bin'/n, core=True) for n in CORE}
            self.configuration=Snapshot(env=self.env)
            for applet in self.configuration.applets.values():
                if 'resolved' in applet:
                    try: pin(Path(applet['executable']))
                    except (OSError, ValueError):
                        applet.clear(); applet['issue']='applet_executable_missing'
            self.check()
        except BaseException:
            self.close(); raise

    @staticmethod
    def stamp(info): return (info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns,info.st_ctime_ns)
    def check(self):
        if self.watch.changed(): raise ValueError('installed_executable_changed')
        for path,(fd,stamp) in self.files.items():
            if self.stamp(os.fstat(fd)) != stamp or self.stamp(path.stat()) != stamp:
                raise ValueError('installed_executable_changed')
        if self.configuration and self.configuration.changed(): raise ValueError('configuration_changed')

    def inventory(self, host=None):
        self.check(); found={n:[] for n in self.processes}; session=self.env['XDG_SESSION_ID'].encode()
        with os.scandir(self.proc) as entries:
            count=0
            for entry in entries:
                if not entry.name.isdigit(): continue
                count+=1
                if count>65536: raise ValueError('process_inventory_limit')
                p=self.proc/entry.name; possible=False
                try:
                    if p.stat().st_uid != os.getuid(): continue
                    comm=_proc_bytes(p/'comm',4096).decode().strip()
                    possible=any(comm==n or comm==n[:15] for n in CORE)
                    # A dead single-thread zombie is terminal; a Z leader with
                    # live workers is not. Missing required counts still refuse.
                    fields=_proc_bytes(p/'stat').rsplit(b') ',1)[-1].split()
                    if len(fields)>17 and fields[0] in (b'Z',b'X') and int(fields[17])==1: continue
                    name=Path(os.readlink(p/'exe').removesuffix(' (deleted)')).name
                    if name not in found and not possible: continue
                    possible=True
                    env=dict(v.split(b'=',1) for v in _proc_bytes(p/'environ',1024*1024).split(b'\0') if b'=' in v)
                    if env.get(b'XDG_SESSION_ID') != session: continue
                    if name not in found: raise ValueError('renamed_required_executable')
                    before=_stat(p)
                    if any(env.get(k.encode()) for k in LOADER_ENV): raise ValueError('process_loader_environment')
                    actual=self.stamp((p/'exe').stat()); expected=self.files[Path(self.processes[name])][1]
                    if actual != expected: raise ValueError('old_or_replaced_executable')
                    if _stat(p) != before: raise ValueError('process_changed')
                    found[name].append((int(entry.name),before))
                except (OSError,ValueError,UnicodeError,IndexError):
                    if possible: raise ValueError('required_process_unavailable')
        for name,actors in found.items():
            if len(actors)!=1: return False
        core={n:found[n] for n in CORE}
        if self.core is None: self.core=core
        elif self.core!=core: return False
        self.check(); return True

    def applet_issues(self, host=None):
        """Applet observations are diagnostics, never panel readiness authority."""
        issues = [dict(applet=name, reason=a['issue'])
            for name,a in self.configuration.applets.items() if 'issue' in a]
        if host is None: return issues[:64]
        expected = {}
        for name,a in self.configuration.applets.items():
            if 'resolved' not in a or a['wrapped'] or not self.configuration.counts[name]: continue
            key = (self.files[Path(a['resolved'])][1], Path(a['executable']).name, tuple(a['args']))
            row = expected.setdefault(key, dict(count=0, ids=[], seen=0))
            row['count'] += self.configuration.counts[name]; row['ids'].append(name)
        with os.scandir(self.proc) as entries:
            for number,entry in enumerate(entries):
                if number > 65536: raise ValueError('process_inventory_limit')
                if not entry.name.isdigit(): continue
                p = self.proc/entry.name
                try:
                    if p.stat().st_uid != os.getuid(): continue
                    before = _stat(p)
                    if not _owned_by_host(before,host,self.proc): continue
                    fields = _proc_bytes(p/'stat').rsplit(b') ',1)[-1].split()
                    if fields[0] in (b'Z',b'X') and int(fields[17])==1: continue
                    argv = _proc_bytes(p/'cmdline',65536).rstrip(b'\0').decode().split('\0')
                    key = (self.stamp((p/'exe').stat()), Path(argv[0]).name, tuple(argv[1:]))
                    if key in expected and _stat(p)==before: expected[key]['seen'] += 1
                except (OSError, ValueError, IndexError, UnicodeError): continue
        for row in expected.values():
            if row['seen'] < row['count']:
                issues.extend(dict(applet=name,reason='applet_not_observed') for name in row['ids'])
        return issues[:64]

    def close(self):
        if self.configuration: self.configuration.close()
        for fd,_ in self.files.values(): os.close(fd)
        self.files.clear(); self.watch.close()


def prepare(manifest,catalog,context):
    validate_bundle(manifest,catalog)
    validate_platform(catalog)
    session=Session(policy=catalog['startup_policy']); completed=False; owner=None
    try:
        if not session.inventory(): session.close(); return None
    except BaseException: session.close(); raise
    def guard(child):
        nonlocal owner
        try:
            current=host_process(child.pid,manifest['binary_sha256'])
            if owner is None: owner=current
            context.check()
            return current==owner and child.poll() is None and session.inventory(owner)
        except (OSError,ValueError,KeyError): return False
    def accepted():
        nonlocal completed
        completed=True; session.close()
    def invalidated():
        try: context.check(); session.check(); return False
        except (OSError,ValueError): return True
    guard.accepted=accepted; guard.invalidated=invalidated
    guard.configuration=session.configuration
    guard.changed=lambda: not completed and session.configuration.changed()
    def close(): session.close(); context.close()
    guard.close=close
    return guard
