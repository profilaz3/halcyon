"""Versioned, local-only bundle integrity and compatibility policy.

No IO in classify_compatibility. Callers supply measured identities, never version
suffixes. A catalog is shipped in the package, not downloaded during a login.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat

FORMAT = 'halcyon-islands-bundle'
CATALOG = 'halcyon-islands-catalog'
# Full 1,783-package discovery measured a 25,689,147-byte cohort envelope.
# Room for two comparable cohorts plus runtime/evidence; never a global cap.
CATALOG_BYTES = 64 * 1024 * 1024
RECORD_BYTES = 16 * 1024 * 1024
HELPERS = ('panel_launcher.py', 'bundle_contract.py', 'runtime_identity.py',
           'install_status.py', 'journal.py', 'install.py', 'refresh.py', 'selector.py', 'panel_configuration.py',
           'runtime_target.py', 'verification.py', 'runtime_compatibility.py')
FILES = (*HELPERS, 'cosmic-panel-islands', 'catalog.json')
HEX = re.compile(r'[0-9a-f]{64}')
CONFIGURATION_RESOLVER = 'cosmic-panel-ca20ce3-islands-v2'
# Reviewed resolver dependencies, not a claim that any ELF was built from them.
# Final native build provenance/qualification must establish that relationship.
CONFIGURATION_SOURCE = {
    "upstream": "ca20ce3185b3b4404f429d59d429c40b58a2ddef",
    "patch_sha256": "56a7ae6237cf37f99ed5884eb0c976d2d711581b928e28a5d14822ef5e162845",
    "core_sha256": "181c76a7f9903a6e3212e123264d8503d26a3f2fccb681666256bc85ebe99546",
    "islands_sha256": "28ae30ded1bc2060b1db72f28978408661691b2916fb65d6358462cafec25985"
}


def installed_package_status(value):
    """dpkg's requested action (including hold) is separate from installed state."""
    fields = value.split() if isinstance(value, str) else []
    return (len(fields) == 3
            and fields[0] in ('unknown', 'install', 'hold', 'deinstall', 'purge')
            and fields[1:] == ['ok', 'installed'])


class FrozenList(list):
    def __init__(self, values): super().__init__(freeze(v) for v in values)
    def _deny(self, *args, **kwargs): raise TypeError('immutable_catalog')
    __setitem__ = __delitem__ = append = clear = extend = insert = pop = remove = reverse = sort = __iadd__ = __imul__ = _deny
    def __deepcopy__(self, memo): return self


class FrozenDict(dict):
    def __init__(self, value): super().__init__((k, freeze(v)) for k,v in value.items())
    def _deny(self, *args, **kwargs): raise TypeError('immutable_catalog')
    __setitem__ = __delitem__ = clear = pop = popitem = setdefault = update = __ior__ = _deny
    def __deepcopy__(self, memo): return self


def freeze(value):
    if isinstance(value, dict): return FrozenDict(value)
    if isinstance(value, list): return FrozenList(value)
    return value


class Catalog(FrozenDict):
    """Only created after complete bounded capture and semantic validation."""
    def __init__(self, value):
        # Direct construction must not become a validation bypass.
        valid_catalog(value)
        super().__init__(value)


class VerificationContext:
    """One operation's immutable captures, guarded by file-change watches.

    This is not a persistent stat/path acceptance cache. Every new operation
    captures bytes anew; staging boundaries use a fresh context.
    """
    def __init__(self):
        self.bundles = {}; self.catalogs = {}; self.watches = []

    def bundle(self, path, manifest_name='manifest.json', owner=None):
        from panel_configuration import Watch
        path = Path(path)
        key = (path, manifest_name, owner)
        self.check()
        if key in self.bundles: return self.bundles[key]
        watch = Watch(); self.watches.append(watch)
        for name in (manifest_name, *FILES): watch.add(path/name)
        result = _verify_bundle(path, manifest_name, owner, self.catalogs)
        self.check()
        self.bundles[key] = result
        return result

    def check(self):
        if any(w.changed() for w in self.watches): raise ValueError('bundle_changed_during_operation')

    def close(self):
        for watch in self.watches: watch.close()
        self.bundles.clear(); self.catalogs.clear(); self.watches.clear()

    def __enter__(self): return self
    def __exit__(self, *args): self.close()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True).encode()


def identity(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def regular(path):
    return not any(p.is_symlink() for p in (path, *path.parents)) and path.is_file()


def digest(path):
    path = Path(path)
    if not regular(path): raise ValueError('not_bounded_regular_file')
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size > 512*1024*1024:
            raise ValueError('not_bounded_regular_file')
        h = hashlib.sha256(); remaining = before.st_size
        while remaining:
            data = stream.read(min(1024*1024,remaining))
            if not data: raise ValueError('file_changed_during_digest')
            h.update(data); remaining -= len(data)
        if stream.read(1): raise ValueError('file_changed_during_digest')
        def stamp(st): return (st.st_dev,st.st_ino,st.st_size,st.st_mtime_ns,st.st_ctime_ns)
        if stamp(before)!=stamp(os.fstat(stream.fileno())) or stamp(before)!=stamp(path.stat(follow_symlinks=False)):
            raise ValueError('file_changed_during_digest')
        return h.hexdigest()


def _record_bytes(path, limit, expected_sha256=None):
    # Bound the opened file, not just a pathname stat followed by read_bytes().
    # NONBLOCK prevents a raced FIFO/device from hanging before fstat refusal.
    path = Path(path)
    if any(p.is_symlink() for p in (path, *path.parents)):
        raise ValueError('not_bounded_record')
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
            raise ValueError('not_bounded_record')
        stream = os.fdopen(fd, 'rb')
    except BaseException:
        os.close(fd)
        raise
    with stream:
        # Do not reserve the full 64 MiB for a smaller catalog (or 16 MiB for
        # every tiny manifest). Growth/truncation still fails the snapshot check.
        data = stream.read(min(limit, before.st_size) + 1)
        if len(data) > limit:
            raise ValueError('not_bounded_record')
        after = os.fstat(stream.fileno())
        current = path.stat(follow_symlinks=False)
        def stamp(info):
            return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
        if (stamp(before) != stamp(after) or stamp(after) != stamp(current)
                or len(data) != before.st_size or not regular(path)):
            raise ValueError('record_changed_during_read')
    if expected_sha256 is not None and hashlib.sha256(data).hexdigest() != expected_sha256:
        raise ValueError('bundle_component_mismatch')
    return data


def _json_object(data):
    value = json.loads(data)
    if not isinstance(value, dict):
        raise ValueError('not_object')
    return value


def read_json(path):
    return _json_object(_record_bytes(path, RECORD_BYTES))


def read_catalog_bytes(path, expected_sha256=None):
    """Bounded exact bytes for packaging; no reserialization of authored input."""
    return _record_bytes(path, CATALOG_BYTES, expected_sha256)


def read_catalog(path, expected_sha256=None, cache=None):
    """Validate the same bounded bytes whose digest the manifest binds."""
    data = read_catalog_bytes(path, expected_sha256)
    key = hashlib.sha256(data).hexdigest()
    if cache is not None and key in cache:
        return cache[key]
    result = Catalog(_json_object(data))
    if cache is not None: cache[key] = result
    return result


def release_catalog(catalog):
    """Keep historical attestation in source, outside family-policy startup IO.

    Validate before projecting: an unknown policy must never become an empty,
    permissive catalog. Legacy exact-cohort packages retain their full records.
    The current startup policy is already authoritative over historical cohorts.
    """
    valid_catalog(catalog)
    return {**catalog, 'cohorts': []} if 'startup_policy' in catalog else catalog


def cohort_id(cohort):
    # Proof locations and refreshed signed index identities are retained on the
    # candidate, but do not turn identical installed bytes into a new cohort.
    def payload(value):
        if isinstance(value, dict):
            return {k: payload(v) for k, v in value.items() if k not in ('provenance', 'url')}
        if isinstance(value, list):
            return [payload(v) for v in value]
        return value
    return identity(payload({k: cohort[k] for k in ('distribution', 'suite', 'architecture', 'packages', 'processes')}))


def runtime_id(profile):
    return identity(profile)


def valid_drm_devices(devices):
    """Exact PCI DRM inventory, not renderer text or generic GPU compatibility."""
    if not isinstance(devices, list) or not 1 <= len(devices) <= 8:
        raise ValueError('runtime_drm_devices')
    fields = {'vendor','device','subsystem_vendor','subsystem_device','pci_class',
              'drivers','virtio_features','primary_nodes','render_nodes'}
    for item in devices:
        if not isinstance(item, dict) or set(item) != fields:
            raise ValueError('runtime_drm_device')
        if any(not isinstance(item[k],str) or not re.fullmatch(r'0x[0-9a-f]{4}',item[k])
               for k in ('vendor','device','subsystem_vendor','subsystem_device')):
            raise ValueError('runtime_drm_pci_id')
        if not isinstance(item['pci_class'],str) or not re.fullmatch(r'0x03[0-9a-f]{4}',item['pci_class']):
            raise ValueError('runtime_drm_pci_class')
        if (not isinstance(item['drivers'],list) or not 1 <= len(item['drivers']) <= 8
                or any(not isinstance(n,str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,64}',n) for n in item['drivers'])):
            raise ValueError('runtime_drm_driver')
        if item['virtio_features'] is not None and (not isinstance(item['virtio_features'],str)
                or not re.fullmatch(r'[01]{64}(?:[01]{64})?',item['virtio_features'])):
            raise ValueError('runtime_drm_features')
        if ('virtio-pci' in item['drivers'] or (item['vendor'],item['device'])==('0x1af4','0x1050')):
            if item['virtio_features'] is None or len(item['drivers'])!=2 or item['drivers'][-1]!='virtio-pci':
                raise ValueError('runtime_drm_virtio_binding')
        for key,minimum in (('primary_nodes',1),('render_nodes',0)):
            if type(item[key]) is not int or not minimum <= item[key] <= 8:
                raise ValueError('runtime_drm_nodes')
    if devices != sorted(devices, key=canonical):
        raise ValueError('runtime_drm_order')
    return devices


def valid_runtime(profile):
    if profile.get('version') not in (3, 4) or not profile.get('processes'):
        raise ValueError('runtime_schema')
    if not {'cosmic-comp', 'cosmic-session', 'cosmic-settings-daemon'} <= profile['processes'].keys():
        raise ValueError('runtime_required_processes')
    for name, item in profile['processes'].items():
        if name in ('cosmic-panel', 'cosmic-panel-islands'):
            raise ValueError('runtime_host_is_not_a_dependency')
        if item.get('lifecycle') not in ('session', 'panel-child'):
            raise ValueError('runtime_lifecycle')
        if name in ('cosmic-comp', 'cosmic-session', 'cosmic-settings-daemon', 'cosmic-notifications') and item['lifecycle'] != 'session':
            raise ValueError('runtime_core_lifecycle')
        if Path(item['executable']).name != name or not Path(item['executable']).is_absolute() or '..' in Path(item['executable']).parts:
            raise ValueError('runtime_executable')
        count_valid = item.get('count') == 'configured' if item['lifecycle'] == 'panel-child' else type(item.get('count')) is int and 1 <= item['count'] <= 64
        if not HEX.fullmatch(item['sha256']) or not count_valid or not isinstance(item['mapped'], dict):
            raise ValueError('runtime_process')
        for path, sha in item['mapped'].items():
            if not Path(path).is_absolute() or '..' in Path(path).parts or not HEX.fullmatch(sha):
                raise ValueError('runtime_mapping')
    children = {n for n,p in profile['processes'].items() if p['lifecycle'] == 'panel-child'}
    config = profile.get('configuration', {})
    if (config.get('resolver') != CONFIGURATION_RESOLVER
            or type(config.get('max_outputs')) is not int or not 1 <= config['max_outputs'] <= 32
            or not isinstance(config.get('applets'), dict)):
        raise ValueError('runtime_configuration')
    if config.get('native_source') != CONFIGURATION_SOURCE:
        raise ValueError('configuration_native_source_mismatch')
    for desktop, entry in config['applets'].items():
        if (not re.fullmatch(r'[A-Za-z0-9_.-]{1,200}', desktop) or entry['process'] not in children
                or not isinstance(entry.get('args'), list) or len(entry['args']) > 64
                or not all(isinstance(a, str) and len(a) <= 4096 and '\0' not in a for a in entry['args'])):
            raise ValueError('runtime_applet')
    if {a['process'] for a in config['applets'].values()} != children:
        raise ValueError('runtime_applet_inventory')
    constraints = profile['constraints']
    if not isinstance(constraints, dict) or set(constraints) - {'kernel_release', 'kernel_version_sha256', 'module_versions', 'drm_devices'}:
        raise ValueError('runtime_constraints')
    if 'kernel_release' in constraints and (not isinstance(constraints['kernel_release'], str) or not 1 <= len(constraints['kernel_release']) <= 256):
        raise ValueError('runtime_kernel_release')
    if 'kernel_version_sha256' in constraints and not HEX.fullmatch(constraints['kernel_version_sha256']):
        raise ValueError('runtime_kernel')
    for name, attrs in constraints.get('module_versions', {}).items():
        if not re.fullmatch('[A-Za-z0-9_]+', name) or not attrs or set(attrs) - {'version', 'srcversion'} or not all(isinstance(v, str) and v for v in attrs.values()):
            raise ValueError('runtime_module')
    if 'drm_devices' in constraints: valid_drm_devices(constraints['drm_devices'])
    # These are deliberately NOT asserted to be observed package identities.
    if profile.get('unmeasured') != ['previously-read-resources', 'kernel-bytes', 'driver-firmware', 'anonymous-code']:
        raise ValueError('runtime_measurement_limits')
    return profile


def valid_catalog(catalog):
    if isinstance(catalog, Catalog): return catalog
    if not isinstance(catalog, dict) or catalog.get('format') != CATALOG or catalog.get('version') not in (1, 2) or not isinstance(catalog.get('cohorts'), list):
        raise ValueError('catalog_schema')
    if 'startup_policy' in catalog:
        from runtime_compatibility import enabled
        enabled(catalog)
    seen = set()
    for row in catalog['cohorts']:
        if not isinstance(row, dict):
            raise ValueError('catalog_row')
        if row['id'] != cohort_id(row) or row['id'] in seen:
            raise ValueError('catalog_cohort_identity')
        seen.add(row['id'])
        if row['qualification'] not in ('native-session', 'development', 'source-only'):
            raise ValueError('catalog_qualification')
        if not HEX.fullmatch(row['payload_sha256']):
            raise ValueError('catalog_payload')
        if row['qualification'] == 'native-session':
            valid_runtime(row['runtime'])
            if catalog['version'] == 1 and {name:item['executable'] for name,item in row['runtime']['processes'].items()} != row['processes']:
                raise ValueError('runtime_process_inventory_mismatch')
            bindings = row['runtime_compatibility']
            if not bindings or len({b['id'] for b in bindings}) != len(bindings):
                raise ValueError('runtime_bindings')
            for binding in bindings:
                if not HEX.fullmatch(binding['id']) or binding['evidence_id'] != row['evidence']['id'] or not binding.get('reason'):
                    raise ValueError('runtime_equivalence_evidence')
            # Evidence IDs are allocated before finalization to avoid a digest
            # cycle. Promotion validates the external result for this exact deb.
            evidence = row['evidence']
            if not re.fullmatch(r'[a-zA-Z0-9._-]{1,128}', evidence['id']) or not HEX.fullmatch(evidence['image_sha256']):
                raise ValueError('catalog_evidence')
            if evidence['scope'] != 'complete-session':
                raise ValueError('catalog_scope')
            if catalog['version'] == 2:
                from runtime_target import valid_target
                valid_target(row.get('runtime_target'), row)
        if not {'cosmic-comp', 'cosmic-session', 'cosmic-settings-daemon'} <= row['processes'].keys():
            raise ValueError('catalog_processes')
        if not {'cosmic-panel', 'cosmic-comp', 'cosmic-session', 'cosmic-settings-daemon'} <= row['packages'].keys():
            raise ValueError('catalog_packages')
        for package in row['packages'].values():
            if not package['version'] or not HEX.fullmatch(package['sha256']) or not isinstance(package['files'], dict):
                raise ValueError('catalog_package')
            for path, sha in package['files'].items():
                if not path.startswith('/usr/') and not path.startswith('/lib/'):
                    raise ValueError('catalog_file_path')
                # POSIX absolute inventory paths: the traversal rule is an
                # exact '..' component, without constructing 165k Path objects
                # on every status/readiness classification. Do not normalize.
                if '..' in path.split('/') or not HEX.fullmatch(sha):
                    raise ValueError('catalog_file_digest')
    profiles = {runtime_id(r['runtime']): r['runtime'] for r in catalog['cohorts'] if r.get('runtime')}
    for row in catalog['cohorts']:
        if any(b['id'] not in profiles for b in row.get('runtime_compatibility', [])):
            raise ValueError('runtime_profile_missing')
        # An equivalent profile may change code hashes, never hide a required
        # child behind a core-only profile or move it into the wrong phase.
        def roles(profile):
            return {n: (p['executable'], p['count'], p['lifecycle']) for n, p in profile['processes'].items()}
        for binding in row.get('runtime_compatibility', []):
            if roles(profiles[binding['id']]) != roles(row['runtime']) or profiles[binding['id']].get('configuration') != row['runtime'].get('configuration'):
                raise ValueError('runtime_equivalence_lifecycle')
    return catalog


LEGACY_PACKAGE = 'cosmic-ext-halcyon-islands'
BUNDLED_PACKAGE = 'cosmic-ext-halcyon'
PACKAGE_OWNERS = (LEGACY_PACKAGE, BUNDLED_PACKAGE)


def package_owner(manifest):
    owner = manifest.get('package_owner', LEGACY_PACKAGE)
    if owner not in PACKAGE_OWNERS:
        raise ValueError('bundle_package_owner')
    return owner


def verify_bundle(bundle, manifest_name='manifest.json', owner=None, context=None):
    if context is not None:
        return context.bundle(bundle, manifest_name, owner)
    return _verify_bundle(bundle, manifest_name, owner)


def _verify_bundle(bundle, manifest_name='manifest.json', owner=None, catalogs=None):
    manifest = read_json(bundle / manifest_name)
    package_owner(manifest)
    if owner is not None and (bundle / manifest_name).stat().st_uid != owner:
        raise ValueError('manifest_owner')
    if (bundle / manifest_name).stat().st_mode & 0o022:
        raise ValueError('manifest_writable')
    if manifest.get('format') != FORMAT or manifest.get('version') != 2 or manifest.get('protocol') != 1:
        raise ValueError('bundle_schema')
    if not re.fullmatch(r'[0-9a-f]{40}', manifest.get('upstream', '')):
        raise ValueError('unresolved_upstream')
    if not isinstance(manifest.get('files'), dict):
        raise ValueError('bundle_inventory')
    # Preserve hash-bound historical generations when adding the journal
    # reader. The oldest inventory still cannot claim the startup policy.
    previous_files = set(FILES) - {'journal.py'}
    legacy_files = previous_files - {'runtime_compatibility.py'}
    if set(manifest['files']) not in (set(FILES), previous_files, legacy_files) or not re.fullmatch(r'[0-9][A-Za-z0-9.+:~\-]*', manifest['package_version']):
        raise ValueError('bundle_inventory')
    for name, entry in manifest['files'].items():
        if not isinstance(entry, dict) or not isinstance(entry.get('mode'), int) or not isinstance(entry.get('sha256'), str):
            raise ValueError('bundle_entry')
        path = bundle / name
        if name == 'catalog.json':
            catalog = read_catalog(path, entry['sha256'], catalogs)
        elif digest(path) != entry['sha256']:
            raise ValueError('bundle_component_mismatch')
        if stat.S_IMODE(path.stat().st_mode) != entry['mode']:
            raise ValueError('bundle_component_mismatch')
        if owner is not None and path.stat().st_uid != owner:
            raise ValueError('bundle_owner')
        if entry['mode'] & 0o022:
            raise ValueError('bundle_writable')
    payload = {k: v for k, v in manifest['files'].items() if k != 'catalog.json'}
    if identity(payload) != manifest['payload_sha256'] or manifest['binary_sha256'] != payload['cosmic-panel-islands']['sha256']:
        raise ValueError('bundle_payload_mismatch')
    if any(r['qualification']=='native-session' for r in catalog['cohorts']) and manifest.get('upstream') != CONFIGURATION_SOURCE['upstream']:
        raise ValueError('configuration_native_source_mismatch')
    if catalog.get('startup_policy'):
        if set(manifest['files']) not in (set(FILES), previous_files): raise ValueError('startup_policy_helper_missing')
        from runtime_compatibility import validate_bundle
        validate_bundle(manifest, catalog)
    return FrozenDict(manifest), catalog


def measured_configuration(running, profile):
    configurations = running.get('configurations')
    if not isinstance(configurations, dict): return False
    observed = configurations.get(runtime_id(profile), {})
    if not isinstance(observed, dict): return False
    children = {n for n,p in profile['processes'].items() if p['lifecycle']=='panel-child'}
    counts = observed.get('children')
    return (isinstance(observed.get('sha256'), str) and HEX.fullmatch(observed['sha256']) is not None
            and isinstance(counts, dict) and set(counts)==children
            and all(type(n) is int and 0 <= n <= 64 for n in counts.values()))


def classify_compatibility(installed, running, manifest, catalog, *, phase='running'):
    def result(state, reason, cohort=None):
        return dict(version=1, state=state, reason=reason, cohort=cohort)
    try:
        valid_catalog(catalog)
        if manifest.get('format') != FORMAT or manifest.get('version') != 2 or manifest.get('verified') is not True:
            return result('invalid_bundle', 'bundle_not_verified')
        if catalog['version'] != 2:
            return result('unsupported', 'runtime_policy_upgrade_required')
        from runtime_target import installed_matches
        admitted = {r['id']: r for r in catalog['cohorts'] if r['qualification'] == 'native-session'
                    and r['payload_sha256'] == manifest['payload_sha256']}
        if admitted and manifest.get('upstream') != CONFIGURATION_SOURCE['upstream']:
            return result('invalid_bundle', 'configuration_native_source_mismatch')
        matches = [r for r in admitted.values() if installed_matches(installed, r)]
        current = matches[0]['id'] if matches else None
        active = running.get('profiles') if running.get('verified') is True and running.get('format') == 'halcyon-runtime-measurement' and running.get('version') == 4 and running.get('phase') == phase and phase in ('prelaunch', 'running') else None
        if current not in admitted:
            return result('unsupported', 'installed_runtime_unqualified')
        if phase == 'running' and (not isinstance(running.get('host'), dict) or not manifest.get('binary_sha256')
                or running['host'].get('sha256') != manifest['binary_sha256']):
            active = None
        if active is None:
            return result('logout_required', 'running_identity_unknown', current)
        profiles = {runtime_id(r['runtime']): r['runtime'] for r in catalog['cohorts'] if r.get('runtime')}
        active = [id for id in active if id in profiles and measured_configuration(running, profiles[id])]
        if not set(active).intersection(b['id'] for b in admitted[current]['runtime_compatibility']):
            return result('logout_required', 'installed_running_differ', current)
        if phase == 'prelaunch':
            return result('eligible', 'qualified_prelaunch', current)
        return result('ready', 'qualified_cohort', current)
    except (KeyError, TypeError, ValueError, AttributeError):
        return result('invalid_bundle', 'catalog_invalid')
