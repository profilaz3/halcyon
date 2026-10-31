"""Read-only measurements of installed payloads and session-bound mapped bytes.

Unrelated process churn is ignored; required candidates and inaccessible code fail closed.
There is no environment-variable compatibility override or generic version gate.
"""
import os
import re
import stat
import contextlib
import hashlib
from pathlib import Path
import subprocess
from bundle_contract import digest, runtime_id, valid_runtime


def rpm_system(root=Path('/')):
    try:
        with (root/'etc/os-release').open('rb') as stream:
            raw = stream.read(65537)
        if len(raw) > 65536: return False
        return any(line.partition('=')[0] == 'ID' and line.partition('=')[2].strip('"') == 'fedora'
                   for line in raw.decode().splitlines())
    except (OSError, UnicodeError): return False


def package_database_paths(root=Path('/')):
    if rpm_system(root):
        # Fedora's sqlite database may update via WAL or atomic replacement.
        base = root/'usr/lib/sysimage/rpm'
        # The SHM file is SQLite reader coordination, not package state. Readers
        # update it on every query; the database and WAL contain actual state.
        return (base, base/'rpmdb.sqlite', base/'rpmdb.sqlite-wal')
    return (root/'var/lib/dpkg/status',)


def watch_package_database(watch,root=Path('/')):
    rpm=rpm_system(root)
    for path in package_database_paths(root):
        (watch.add_content if rpm else watch.add)(path)


def rpm_query(arguments, timeout=3):
    return subprocess.run(['/usr/bin/rpm', '--dbpath', '/usr/lib/sysimage/rpm', *arguments],
                          capture_output=True, text=True, timeout=timeout)


def package_version(name):
    """Installed native version, or None. No filename or version guessing."""
    if not re.fullmatch(r'[a-z0-9][a-z0-9+.-]{0,127}', name): return None
    try:
        if rpm_system():
            result = rpm_query(['-q', '--qf', '%{NAME}\\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\\n', name])
            rows = result.stdout.splitlines()
            if result.returncode or len(rows) != 1: return None
            package, version = rows[0].split('\t')
            return version if package == name and re.fullmatch(r'[A-Za-z0-9.+:~^_-]{1,128}', version) else None
        result = subprocess.run(['/usr/bin/dpkg-query', '-W', '-f=${Status}\\n${Version}', name],
                                capture_output=True, text=True, timeout=3)
        rows = result.stdout.splitlines()
        from bundle_contract import installed_package_status
        return rows[1] if not result.returncode and len(rows) == 2 and installed_package_status(rows[0]) else None
    except (OSError, ValueError, subprocess.SubprocessError): return None


def version_newer(candidate, installed):
    if not all(isinstance(v,str) and re.fullmatch(r'[A-Za-z0-9.+:~^_-]{1,128}',v) for v in (candidate, installed)):
        return False
    if rpm_system():
        # Strict alphabet above excludes Lua quotes and RPM macro expansion.
        result = rpm_query(['--eval', "%{lua:print(rpm.vercmp('" + candidate + "','" + installed + "'))}"], timeout=2)
        return not result.returncode and result.stdout.strip() == '1'
    return subprocess.run(['/usr/bin/dpkg','--compare-versions',candidate,'gt',installed],
                          stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=2).returncode == 0


def versions():
    try:
        if rpm_system():
            result = rpm_query(['-qa', '--qf', '%{NAME}\\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\\t%{ARCH}\\n'])
            if result.returncode or len(result.stdout) > 4*1024*1024: return {}
            candidates = {}
            for name, version, arch in (line.split('\t') for line in result.stdout.splitlines()):
                candidates.setdefault(name, []).append((version, arch))
            result_rows = {}
            for name, installed in candidates.items():
                # Fedora retains older kernels and can have unrelated multilib
                # packages. Ambiguity invalidates that package, not all COSMIC.
                if len(installed) == 1:
                    result_rows[name] = installed[0]
                for version, arch in installed:
                    if sum(other_arch == arch for _, other_arch in installed) == 1:
                        result_rows[name + ':' + arch] = (version, arch)
            return result_rows
        result = subprocess.run(['/usr/bin/dpkg-query', '-W', '-f=${binary:Package}\t${Version}\t${Architecture}\t${db:Status-Status}\n'],
                                capture_output=True, text=True, timeout=3, check=True)
        return {name: (version, architecture) for name, version, architecture, state in
                (line.split('\t') for line in result.stdout.splitlines()) if state == 'installed'}
    except (OSError, ValueError, subprocess.SubprocessError):
        return {}


def inventory(row):
    files = {}
    for package in row['packages'].values():
        for path, sha in package['files'].items():
            if path in files and files[path] != sha:
                raise ValueError('conflicting_package_files')
            files[path] = sha
    return files


def installed_identity(catalog, root=Path('/'), package_versions=None, system=None, *, measurement=None):
    """Measure only explicitly authored public runtime targets; never the OS lock."""
    from runtime_target import SCOPE, public_path
    from bundle_contract import valid_catalog
    result = dict(format='halcyon-installed-runtime', version=1, scope=SCOPE,
                  verified=False, targets=[])
    valid_catalog(catalog)
    if catalog['version'] != 2: return result
    if system is None:
        try:
            path = root/'etc/os-release'
            with path.open('rb') as stream: data = stream.read(65537)
            if len(data)>65536: return result
            values = dict(line.split('=', 1) for line in data.decode().splitlines() if '=' in line)
            rpm = rpm_system(root)
            architecture = (rpm_query(['--eval', '%{_arch}']).stdout.strip() if rpm else subprocess.check_output(['/usr/bin/dpkg','--print-architecture'], text=True, timeout=2).strip()) if root == Path('/') else None
            system = dict(distribution=values['ID'].strip('"'), suite=values.get('VERSION_ID' if rpm else 'VERSION_CODENAME',values.get('UBUNTU_CODENAME','')).strip('"'), architecture=architecture)
        except (OSError, KeyError, ValueError, subprocess.SubprocessError): return result
    # The loader can select code outside the declared closure. Do not silently
    # sanitize an already running user's processes and claim they match.
    if any(os.environ.get(k) for k in LOADER_ENV): return result
    matches = []
    work = runtime_work(catalog)
    with (contextlib.nullcontext(measurement) if measurement is not None else FileMeasurement(root=root, **work)) as measurement:
        measurement.max_files=work['max_files']; measurement.max_bytes=work['max_bytes']
        if package_versions is None:
            watch_package_database(measurement.watch, root)
            package_versions=versions()
        for row in catalog['cohorts']:
            if row['qualification'] != 'native-session': continue
            target = row['runtime_target']
            if target['system'] != system: continue
            if not all(package_versions.get(n, package_versions.get(n+':'+p['architecture'])) in
                       ((p['version'],p['architecture']), p['version'] if root != Path('/') else None)
                       for n,p in target['packages'].items()): continue
            try:
                for path, info in target['installed_files'].items():
                    resolved = measurement.resolve(root/path.lstrip('/'))
                    if not public_path('/'+str(resolved.relative_to(root))):
                        raise ValueError('nonpublic_runtime_resolution')
                    if measurement.hash(resolved, expected_size=info['size']) != info['sha256']:
                        raise ValueError('installed_runtime_bytes_changed')
                measurement.check()
                matches.append(target['id'])
            except (OSError, ValueError): continue
        measurement.check()
        actual_work=dict(files=len(measurement.files),bytes=measurement.bytes)
    return {**result, 'verified':bool(matches), 'targets':sorted(matches),'work':actual_work}


def runtime_work(catalog, host=False):
    # All explicitly declared alternatives, never the complete package file map.
    # One additional payload ELF is manifest-bound and individually capped.
    from runtime_target import MAX_FILE_BYTES
    files = [item for row in catalog['cohorts'] if row.get('runtime_target')
             for item in row['runtime_target']['installed_files'].values()]
    return dict(max_files=len(files)+int(host), max_bytes=sum(f['size'] for f in files)+(MAX_FILE_BYTES if host else 0))


LOADER_ENV = ('LD_PRELOAD','LD_LIBRARY_PATH','LD_AUDIT','LIBGL_DRIVERS_PATH',
              'MESA_LOADER_DRIVER_OVERRIDE','__EGL_VENDOR_LIBRARY_FILENAMES',
              '__EGL_VENDOR_LIBRARY_DIRS','GBM_BACKENDS_PATH','GALLIUM_DRIVER','GLIBC_TUNABLES')


class FileMeasurement:
    """Pinned content reads, deduplicated only within one bounded phase.

    Inotify is invalidation, not content evidence. Each new phase hashes anew.
    Descriptor identity, before/after metadata, mapping inode and path watches
    reject ordinary package/config replacement, including ABA.
    """
    def __init__(self, root=Path('/'), *, max_files=None, max_bytes=None):
        from panel_configuration import Watch
        self.root = root.resolve(); self.watch = Watch(); self.files = {}; self.aliases = {}
        self.bytes = 0; self.max_files=max_files; self.max_bytes=max_bytes
    def resolve(self, path):
        # Watch each parent and symlink before resolution; the existing config
        # Watch deliberately rejects symlinks, so watch their parent entries.
        path = Path(path); todo = list(path.parts[1:]); current = Path('/'); count = 0
        while todo:
            part = todo.pop(0)
            if part in ('', '.'): continue
            if part == '..': current = current.parent; continue
            candidate = current/part
            self.watch.add(candidate, entry=True)
            if candidate.is_symlink():
                count += 1
                if count>40: raise ValueError('symlink_loop')
                target = Path(os.readlink(candidate))
                if target.is_absolute(): current=Path('/'); todo=list(target.parts[1:])+todo
                else: todo=list(target.parts)+todo
            else: current=candidate
        if not current.is_relative_to(self.root): raise ValueError('root_escape')
        self.aliases[str(path)] = str(current)
        return current
    @staticmethod
    def stamp(st): return (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_ctime_ns, st.st_mode, st.st_uid)
    def hash(self, path, expected_size=None, inode=None):
        import stat
        from runtime_target import MAX_FILE_BYTES
        path = Path(path)
        if not str(path).startswith('/proc/'):
            self.watch.add(path, entry=True)
        fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NONBLOCK)
        try:
            st = os.fstat(fd)
            if (not stat.S_ISREG(st.st_mode) or st.st_size>MAX_FILE_BYTES
                    or expected_size is not None and st.st_size!=expected_size
                    or inode is not None and (st.st_dev,st.st_ino)!=inode):
                raise ValueError('unbounded_or_replaced_file')
            if expected_size is not None and self.root == Path('/') and (st.st_uid != 0 or st.st_mode & 0o022) and not str(path).startswith('/proc/'):
                raise ValueError('runtime_file_owner_or_mode')
            key = self.stamp(st)
            if key in self.files: return self.files[key][1]
            if ((self.max_files is not None and len(self.files) >= self.max_files)
                    or (self.max_bytes is not None and self.bytes+st.st_size > self.max_bytes)):
                raise ValueError('runtime_work_exceeded')
            with os.fdopen(os.dup(fd), 'rb') as stream:
                h = hashlib.sha256(); remaining = st.st_size
                while remaining:
                    data = stream.read(min(1024*1024, remaining))
                    if not data: raise ValueError('runtime_file_truncated')
                    h.update(data); remaining -= len(data)
                if stream.read(1): raise ValueError('runtime_file_grew')
                value = h.hexdigest()
            if self.stamp(os.fstat(fd)) != key: raise ValueError('runtime_file_changed')
            self.bytes += st.st_size
            self.files[key] = (fd, value); fd = None
            return value
        finally:
            if fd is not None: os.close(fd)
    def check(self):
        if self.watch.changed(): raise ValueError('runtime_files_changed')
        for key,(fd,_) in self.files.items():
            if self.stamp(os.fstat(fd)) != key: raise ValueError('runtime_file_changed')
        for path,resolved in self.aliases.items():
            if str(Path(path).resolve()) != resolved: raise ValueError('runtime_resolution_changed')
    def close(self):
        for fd,_ in self.files.values(): os.close(fd)
        self.files.clear(); self.watch.close()
    def __enter__(self): return self
    def __exit__(self,*args): self.close()


def _stream_hash(path):
    # procfs links pin live inodes; kernel text has stat size zero. Bound both.
    limit=65536 if path.name=='version' else 512*1024*1024
    h=hashlib.sha256(); total=0
    with path.open('rb') as stream:
        while True:
            data=stream.read(min(1024*1024,limit-total+1))
            if not data: break
            total+=len(data)
            if total>limit: raise ValueError('runtime_digest_limit')
            h.update(data)
    return h.hexdigest()


def _proc_bytes(path, limit=65536):
    with path.open('rb') as stream: value=stream.read(limit+1)
    if len(value)>limit: raise ValueError('proc_record_limit')
    return value


def _stat(process):
    parts = _proc_bytes(process / 'stat').rsplit(b') ', 1)
    if len(parts) != 2:
        raise ValueError('process_stat_incomplete')
    fields = parts[1].split()
    if len(fields) < 20:
        raise ValueError('process_stat_incomplete')
    if fields[0] in (b'Z', b'X'):
        raise ValueError('process_exited')
    return dict(start=fields[19].decode(), parent=int(fields[1]), group=int(fields[2]), sid=int(fields[3]))


def host_process(pid, binary_sha256, proc=Path('/proc'), session=None, uid=None, measurement=None):
    """Pin the actual supervised process, not a second discovery host."""
    session = os.environ.get('XDG_SESSION_ID') if session is None else session
    uid = os.getuid() if uid is None else uid
    process = proc / str(pid)
    before = _stat(process)
    env = dict(item.split(b'=', 1) for item in _proc_bytes(process/'environ',1024*1024).split(b'\0') if b'=' in item)
    with (process/'exe').open('rb') as executable:
        executable_hash = measurement.hash(Path('/proc/self/fd')/str(executable.fileno())) if measurement else _stream_hash(process/'exe')
    if (not session or process.stat().st_uid != uid or env.get(b'XDG_SESSION_ID') != session.encode()
            or before['group'] != pid or before['sid'] != pid
            or executable_hash != binary_sha256 or _stat(process) != before):
        raise ValueError('host_identity_unknown')
    return dict(pid=pid, sha256=binary_sha256, **before)


def _owned_by_host(process, host, proc):
    # Applets must descend from this host and remain in its supervised process
    # group/session. Matching old or unrelated applets cannot satisfy readiness.
    if process['group'] != host['pid'] or process['sid'] != host['pid'] or int(process['start']) < int(host['start']):
        return False
    parent = process['parent']
    seen = set()
    for _ in range(64):
        if parent == host['pid']:
            return True
        if parent <= 1 or parent in seen:
            return False
        seen.add(parent)
        ancestor = _stat(proc / str(parent))
        if ancestor['group'] != host['pid'] or ancestor['sid'] != host['pid'] or int(ancestor['start']) < int(host['start']):
            return False
        parent = ancestor['parent']
    return False


def _process(process, measurement=None):
    before = _stat(process)
    exe_link = os.readlink(process / 'exe')
    exe_path = exe_link.removesuffix(' (deleted)')
    with (process / 'exe').open('rb') as stream:
        st = os.fstat(stream.fileno())
        exe_hash = measurement.hash(Path('/proc/self/fd')/str(stream.fileno())) if measurement else hashlib.file_digest(stream, 'sha256').hexdigest()
        maps_before = _proc_bytes(process / 'maps',4*1024*1024).decode()
        mapped = {}
        for line in maps_before.splitlines():
            fields = line.split(None, 5)
            if len(fields) != 6 or 'x' not in fields[1] or not fields[5].startswith('/'):
                continue
            name = fields[5].removesuffix(' (deleted)')
            major, minor = (int(v, 16) for v in fields[3].split(':'))
            inode = int(fields[4])
            dev = os.makedev(major, minor)
            if st.st_ino == inode and st.st_dev == dev:
                value = exe_hash
            else:
                # map_files may require ptrace privileges. The fallback is safe
                # only when the open file is the inode recorded in /proc/maps.
                try:
                    file = (process / 'map_files' / fields[0]).open('rb')
                except OSError:
                    file = Path(name).open('rb')
                with file:
                    mst = os.fstat(file.fileno())
                    if mst.st_ino != inode or mst.st_dev != dev:
                        raise ValueError('mapping_replaced')
                    value = measurement.hash(Path('/proc/self/fd')/str(file.fileno()), inode=(dev,inode)) if measurement else hashlib.file_digest(file, 'sha256').hexdigest()
            if name in mapped and mapped[name] != value:
                raise ValueError('conflicting_mapping')
            mapped[name] = value
        if maps_before != _proc_bytes(process / 'maps',4*1024*1024).decode() or exe_link != os.readlink(process / 'exe'):
            raise ValueError('process_changed')
    if _stat(process) != before:
        raise ValueError('process_reused_or_reparented')
    return dict(executable=exe_path, sha256=exe_hash, mapped=mapped, **before)


def _snapshot(proc, names, session, uid, measurement=None):
    found = {name: [] for name in names}
    errors = set()
    for process in list(proc.iterdir()):
        if not process.name.isdigit():
            continue
        possible = set()
        try:
            if process.stat().st_uid != uid:
                continue
            comm = _proc_bytes(process / 'comm',4096).decode().strip()
            possible = {n for n in names if comm == n[:15] or comm == n}
            try:
                name = Path(os.readlink(process / 'exe').removesuffix(' (deleted)')).name
            except OSError:
                # A zombie or vanished unrelated process says nothing about the
                # required COSMIC inventory. A potential required one is unsafe.
                errors.update(possible)
                continue
            if name not in names:
                continue
            possible = {name}
            env = dict(item.split(b'=', 1) for item in _proc_bytes(process / 'environ',1024*1024).split(b'\0') if b'=' in item)
            if env.get(b'XDG_SESSION_ID') != session.encode():
                continue
            if any(env.get(k.encode()) for k in LOADER_ENV):
                raise ValueError('loader_environment_unqualified')
            found[name].append((process.name, _process(process, measurement)))
        except (OSError, ValueError, UnicodeError, IndexError):
            errors.update(possible)
    return found, errors


def _bounded_attribute(path, identities=None):
    # sysfs reports a page-sized st_size for short attributes. Bound actual read,
    # refuse links/devices/FIFOs and compare opened/path metadata around the IO.
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
    with os.fdopen(fd, 'rb') as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode): raise ValueError('device_attribute_type')
        value = os.read(stream.fileno(), 257)
        if len(value)>256: raise ValueError('device_attribute_limit')
        stamp = FileMeasurement.stamp(before)
        if stamp != FileMeasurement.stamp(os.fstat(stream.fileno())) or stamp != FileMeasurement.stamp(path.stat(follow_symlinks=False)):
            raise ValueError('device_attribute_changed')
    if identities is not None: identities.append((str(path), stamp))
    return value.decode('ascii').strip()


def drm_device_snapshot(sysroot=Path('/sys')):
    """Bounded PCI DRM inventory. Never opens a GPU node or creates a context.

    Policy ignores card numbers/PCI addresses but includes multiplicity, driver
    binding and complete feature bits for supported virtio GPU relationships.
    Private inode/path stamps detect replacement between the phase's before/after snapshots.
    """
    from bundle_contract import canonical, valid_drm_devices
    root=Path(sysroot).resolve(); base=root/'class/drm'; identities=[]
    def names(directory):
        with os.scandir(directory) as entries:
            result=[]
            for entry in entries:
                result.append(entry.name)
                if len(result)>256: raise ValueError('drm_inventory_limit')
        return sorted(result)
    def resolved(path, area):
        target=path.resolve(strict=True)
        if not target.is_relative_to(root/area): raise ValueError('device_path_escape')
        identities.append((str(path),FileMeasurement.stamp(path.lstat()),str(target),FileMeasurement.stamp(target.stat())))
        return target
    def bus(device):
        target=resolved(device/'subsystem','bus')
        if target not in (root/'bus/pci',root/'bus/virtio'):
            raise ValueError('drm_unsupported_bus')
        if resolved(target/'devices'/device.name,'devices')!=device:
            raise ValueError('drm_bus_device_binding')
        return target.name
    def driver(device, bus_name):
        target=resolved(device/'driver','bus')
        if target.parent!=root/'bus'/bus_name/'drivers':
            raise ValueError('drm_driver_bus')
        return target.name
    initial=names(base); nodes=[n for n in initial if re.fullmatch(r'card[0-9]+|renderD[0-9]+',n)]
    if not 1 <= len(nodes) <= 16: raise ValueError('drm_node_limit')
    groups={}
    for name in nodes:
        node=resolved(base/name,'devices'); device=resolved(node/'device','devices')
        if node.parent!=device/'drm': raise ValueError('drm_device_ancestry')
        # Two explicit Linux layouts: DRM below PCI, or below its direct virtio
        # child. Bus links, not vendor/device filenames, determine the PCI node.
        owner_bus=bus(device)
        pci=device if owner_bus=='pci' else device.parent
        if bus(pci)!='pci': raise ValueError('drm_pci_parent')
        pci_driver=driver(pci,'pci')
        values={k:_bounded_attribute(pci/k,identities) for k in ('vendor','device','subsystem_vendor','subsystem_device')}
        values.update(pci_class=_bounded_attribute(pci/'class',identities),drivers=[pci_driver],
                      virtio_features=None,primary_nodes=0,render_nodes=0)
        # PCI-parent DRM still uses the virtio child GPU. Never silently freeze
        # just the transport. Exact child inventory and links are phase stamps.
        children=names(pci)
        virtio=[n for n in children if re.fullmatch(r'virtio[0-9]+',n)]
        identities.append((str(pci),FileMeasurement.stamp(pci.stat()),tuple(children)))
        if (virtio or owner_bus=='virtio' or pci_driver=='virtio-pci'
                or (values['vendor'],values['device'])==('0x1af4','0x1050')):
            if pci_driver!='virtio-pci' or len(virtio)!=1:
                raise ValueError('drm_virtio_binding')
            child=resolved(pci/virtio[0],'devices')
            if child.parent!=pci or bus(child)!='virtio' or device not in (pci,child):
                raise ValueError('drm_virtio_binding')
            # This deliberately supports the observed virtio GPU relationship,
            # not arbitrary virtio devices attached to a PCI transport.
            if (_bounded_attribute(child/'vendor',identities)!='0x1af4'
                    or values['vendor']!='0x1af4'
                    or _bounded_attribute(child/'device',identities)!='0x0010'):
                raise ValueError('drm_virtio_gpu_identity')
            values['drivers']=[driver(child,'virtio'),pci_driver]
            values['virtio_features']=_bounded_attribute(child/'features',identities)
        if names(pci)!=children: raise ValueError('drm_virtio_inventory_changed')
        group=groups.setdefault(str(pci),values)
        if {k:v for k,v in group.items() if k not in ('primary_nodes','render_nodes')} != {k:v for k,v in values.items() if k not in ('primary_nodes','render_nodes')}:
            raise ValueError('drm_device_changed')
        group['primary_nodes' if name.startswith('card') else 'render_nodes']+=1
    if names(base)!=initial: raise ValueError('drm_inventory_changed')
    devices=sorted(groups.values(),key=canonical);valid_drm_devices(devices)
    return devices,tuple(identities)


def _constraint_snapshot(expected, proc, sysroot):
    actual={};identities=()
    if 'kernel_release' in expected:
        actual['kernel_release']=_bounded_attribute(proc/'sys/kernel/osrelease')
    if 'kernel_version_sha256' in expected:
        actual['kernel_version_sha256']=_stream_hash(proc/'version')
    if 'module_versions' in expected:
        actual['module_versions']={name:{key:_bounded_attribute(sysroot/'module'/name/key)
            for key in attrs} for name,attrs in expected['module_versions'].items()}
    if 'drm_devices' in expected:
        actual['drm_devices'],identities=drm_device_snapshot(sysroot)
    return actual,identities


def _constraints(expected, proc, sysroot):
    return _constraint_snapshot(expected,proc,sysroot)[0] == expected


def running_identity(catalog, proc=Path('/proc'), session=None, uid=None, sysroot=Path('/sys'), *, phase='running', host=None, configuration=None):
    session = os.environ.get('XDG_SESSION_ID') if session is None else session
    uid = os.getuid() if uid is None else uid
    result = dict(format='halcyon-runtime-measurement', version=4, phase=phase, verified=False, profiles=[])
    if not session or phase not in ('prelaunch', 'running') or phase=='running' and host is None:
        return result
    if catalog.get('version') != 2: return result
    snapshots = {}
    measurement = FileMeasurement(**runtime_work(catalog, host=True))
    owned_snapshots = []
    try:
        profiles = {runtime_id(r['runtime']): valid_runtime(r['runtime']) for r in catalog['cohorts'] if r.get('runtime')}
        names = {name for p in profiles.values() for name in p['processes']}
        from panel_configuration import Snapshot, policy
        for id, profile in profiles.items():
            if configuration is not None:
                snapshot = configuration
                if snapshot.policy != policy(profile): continue
            else:
                try: snapshot = Snapshot(policy(profile))
                except (OSError, ValueError, KeyError): continue
                owned_snapshots.append(snapshot)
            if not snapshot.changed(): snapshots[id] = snapshot
        if phase == 'running' and host is not None:
            if host_process(host['pid'], host['sha256'], proc, session, uid, measurement) != host:
                return result
        # Two bounded attempts, each with two snapshots. Never wait for a process
        # to exit, and never substitute installed package versions for live code.
        for _ in range(2):
            found, errors = _snapshot(proc, names, session, uid, measurement)
            second, second_errors = _snapshot(proc, names, session, uid, measurement)
            matches = []
            for id, profile in profiles.items():
                required = profile['processes']
                snapshot = snapshots.get(id)
                if snapshot is None:
                    continue
                counts = {n: (0 if phase == 'prelaunch' else snapshot.counts[n]) if item['lifecycle']=='panel-child' else item['count']
                          for n,item in required.items()}
                if set(required) & (errors | second_errors):
                    continue
                if any(found[n] != second[n] or len(found[n]) != counts[n] for n, item in required.items()):
                    continue
                try:
                    constraint_snapshot = _constraint_snapshot(profile['constraints'], proc, sysroot)
                    if constraint_snapshot[0] != profile['constraints']:
                        continue
                except (OSError, ValueError):
                    continue
                try:
                    valid = True
                    for name, item in required.items():
                        if phase == 'prelaunch' and item['lifecycle'] == 'panel-child':
                            # Installed lock verification covers package resources;
                            # this also binds the exact executable about to spawn
                            # to the chosen profile (including equivalence rows).
                            valid &= measurement.hash(Path(item['executable']).resolve()) == item['sha256']
                            continue
                        for _, observed in found[name]:
                            valid &= observed['executable'] == item['executable'] and observed['sha256'] == item['sha256']
                            valid &= all(item['mapped'].get(path) == sha for path, sha in observed['mapped'].items())
                            if item['lifecycle'] == 'panel-child':
                                valid &= host is not None and _owned_by_host(observed, host, proc)
                    if phase == 'running' and host is not None:
                        observed_host = _process(proc/str(host['pid']), measurement)
                        valid &= all(path == observed_host['executable'] or profile['host_mapped'].get(path) == sha for path,sha in observed_host['mapped'].items())
                    measurement.check()
                    if valid and _constraint_snapshot(profile['constraints'], proc, sysroot) != constraint_snapshot:
                        continue
                    if valid and (snapshot is None or not snapshot.changed()):
                        matches.append(id)
                except (OSError, ValueError, IndexError):
                    continue
            if matches:
                if phase == 'running' and host is not None and host_process(host['pid'], host['sha256'], proc, session, uid, measurement) != host:
                    return result
                return {**result, 'verified': True, 'profiles': sorted(matches), 'host':host if phase=='running' else None,
                        'work':dict(files=len(measurement.files),bytes=measurement.bytes),
                        'configurations': {id: dict(sha256=snapshots[id].identity, children=snapshots[id].counts) for id in matches if id in snapshots}}
    except (OSError, KeyError, TypeError, ValueError):
        pass
    finally:
        measurement.close()
        for snapshot in owned_snapshots: snapshot.close()
    return result


def host_identity(binary_sha256):
    """Read-only IPC-owner process identity, independent of installation."""
    import json
    import socket
    import struct
    runtime = os.environ.get('XDG_RUNTIME_DIR')
    session = os.environ.get('XDG_SESSION_ID')
    if not runtime or not session:
        return None
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(.3)
            client.connect(str(Path(runtime) / 'desktop-cards-islands-v1.sock'))
            pid, uid, _ = struct.unpack('3i', client.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize('3i')))
            if uid != os.getuid():
                return None
            host = host_process(pid, binary_sha256)
            client.sendall(b'"Capabilities"\n')
            with client.makefile('rb') as stream:
                line = stream.readline(65537)
            if len(line) > 65536 or not line.endswith(b'\n'):
                return None
            cap = json.loads(line).get('Capabilities', {})
            if cap.get('protocol') == 1 and cap.get('together') is True and cap.get('separate') is True and host_process(pid, binary_sha256) == host:
                return host
            return None
    except (OSError, ValueError, TypeError, AttributeError):
        return None


def host_health(binary_sha256):
    if not os.environ.get('XDG_RUNTIME_DIR') or not os.environ.get('XDG_SESSION_ID'):
        return None
    return host_identity(binary_sha256) is not None


def main():
    import argparse
    import json
    from bundle_contract import verify_bundle
    parser = argparse.ArgumentParser(description='Bounded read-only Islands target and live-code measurements')
    parser.add_argument('--catalog', type=Path, required=True)
    parser.add_argument('--phase', choices=('prelaunch', 'running'), default='running')
    args = parser.parse_args()
    if args.catalog.name != 'catalog.json':
        parser.error('--catalog must identify a verified bundle catalog.json')
    bundle = args.catalog.parent
    from bundle_contract import VerificationContext
    from verification import bounded
    def inspect():
        with VerificationContext() as context:
            manifest, catalog = verify_bundle(bundle, 'companion-manifest.json' if (bundle/'companion-manifest.json').exists() else 'manifest.json', context=context)
            installed = installed_identity(catalog)
            host = host_identity(manifest['binary_sha256']) if args.phase == 'running' else None
            runtime = running_identity(catalog, phase=args.phase, host=host)
            context.check()
            return dict(installed=installed,runtime=runtime)
    try: result = bounded(inspect,5)
    except (OSError, ValueError, TimeoutError):
        print(json.dumps(dict(reason='verification_incomplete',installed=dict(verified=False),runtime=dict(verified=False))))
        raise SystemExit(2)
    print(json.dumps(result))



if __name__ == '__main__':
    main()
