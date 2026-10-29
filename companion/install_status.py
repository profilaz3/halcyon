#!/usr/bin/python3
"""Read-only Islands installation capability v1. Never starts a companion."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import stat
import subprocess
import time
import panel_launcher
from bundle_contract import installed_package_status

PACKAGE = 'cosmic-ext-halcyon-islands'
from bundle_contract import package_owner
FORMAT = 'halcyon-islands-installation'


def regular(path):
    return not any(p.is_symlink() for p in (path, *path.parents)) and path.is_file()


def record(path):
    if not regular(path) or path.stat().st_size > 65536:
        raise ValueError('Not a bounded regular record')
    value = json.loads(path.read_bytes())
    if not isinstance(value, dict):
        raise ValueError('Not an object')
    return value


def digest(path):
    if not regular(path) or path.stat().st_size > 512 * 1024 * 1024:
        raise ValueError('Not a bounded regular binary')
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verified(bundle, manifest_name):
    try:
        manifest = record(bundle / manifest_name)
        binary = bundle / 'cosmic-panel-islands'
        if manifest.get('upstream') != 'ca20ce3185b3b4404f429d59d429c40b58a2ddef' or manifest.get('protocol') != 1 or digest(binary) != manifest.get('binary_sha256') or not binary.stat().st_mode & stat.S_IXUSR:
            return None
        return manifest
    except (OSError, ValueError, TypeError):
        return None


def detect(home, library, package_installed, versions, *, installed=None, running=None, host=None, context=None):
    result = dict(format=FORMAT, version=1, installation='absent', provenance=None,
                  enabled=False, compatible=None, running=None, healthy=None)
    result['package_versions'] = {name: value for name, value in versions.items()
        if name in ('cosmic-panel', 'cosmic-comp', 'cosmic-session', 'cosmic-settings-daemon')
        and isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9.+:~_-]{1,128}', value)}
    bundle = home / '.local/share/cosmic-desktop-cards/companion'
    manifest = None
    payload_qualified = False
    if package_installed:
        # dpkg + package CLI establish provenance. Payload qualification is a
        # separate health/compatibility result, never an installation detector.
        result.update(installation='installed', provenance='package')
        try:
            manifest = record(library / 'companion-manifest.json')
            if manifest.get('version') == 2:
                return generation_status(result, home, library, installed, running, host, context=context)
            binary = library / 'cosmic-panel-islands'
            intact = digest(binary) == manifest.get('binary_sha256') and bool(binary.stat().st_mode & stat.S_IXUSR)
            payload_qualified = manifest.get('upstream') == 'ca20ce3185b3b4404f429d59d429c40b58a2ddef' and manifest.get('protocol') == 1
            if not intact:
                result['healthy'] = False
        except (OSError, ValueError, TypeError):
            result['healthy'] = False
    else:
        # A former packaged user bundle is not a supported manual install.
        # Only a completed, unrestored installer record with exact payloads is.
        for path in sorted((home / '.local/share/cosmic-desktop-cards/backups').glob('panel-*/install-record.json'), reverse=True)[:128]:
            try:
                saved = record(path)
                if saved.get('root') != str(home) or not saved.get('complete') or saved.get('restored') or saved.get('owner') is not None:
                    continue
                required = [str((bundle / n).relative_to(home)) for n in ('cosmic-panel-islands', 'manifest.json')]
                required.append('.local/bin/cosmic-panel')
                if not all(digest(home / rel) == saved['installed'][rel] and (home / rel).stat().st_mode & 0o777 == saved['modes'][rel] for rel in required):
                    continue
                manifest = verified(bundle, 'manifest.json')
                if manifest is not None:
                    result.update(installation='installed', provenance='manual')
                    payload_qualified = True
                    break
            except (OSError, ValueError, TypeError, KeyError):
                continue
    if result['installation'] != 'installed':
        return result
    # V2 retains installation provenance even if the optional payload is broken.
    if (manifest or {}).get('version') == 2 or (library / 'catalog.json').exists() or (library / 'bundle_contract.py').exists():
        return generation_status(result, home, library, installed, running, host, context=context)
    # Compatibility is the selector's single reviewed-source policy. It is
    # independent of installation and remains weaker than running/ABI health.
    result['compatible'] = payload_qualified and panel_launcher.supported_system(versions) and (
        any(versions == row for row in panel_launcher.SUPPORTED_SYSTEMS)
        or (manifest or {}).get('runtime_probe') == 1)
    try:
        selection = record(bundle / 'selection.json')
        result['enabled'] = selection.get('enabled') is True and verified(bundle, 'manifest.json') is not None
    except (OSError, ValueError, TypeError):
        pass
    # Running and runtime ABI/health are separately verified through the socket.
    return result


def generation_status(result, home, library, installed=None, running=None, host=None, context=None):
    from bundle_contract import VerificationContext
    if context is None:
        with VerificationContext() as operation:
            return generation_status(result, home, library, installed, running, host, operation)
    from bundle_contract import verify_bundle, classify_compatibility
    from journal import selected_generation, records, verify_owned
    state = dict(state='invalid_bundle', reason='bundle_not_verified', cohort=None)
    generation = active_generation = None
    try:
        manifest, catalog = verify_bundle(library, 'companion-manifest.json', context=context)
        generation = digest(library / 'companion-manifest.json')
        from runtime_compatibility import enabled as family_policy, state as family_state
        if installed is None:
            state = dict(state='unknown', reason='not_measured', cohort=None)
            result['compatible'] = None
        elif family_policy(catalog):
            # This read-only worker observation is not reusable write authority.
            state = family_state(installed, host is True) if (installed or {}).get('family') else dict(
                state='unsupported', reason='unsupported_cosmic_family', cohort=None)
            result['compatible'] = state['state'] in ('ready', 'eligible')
            result['package_versions'] = (installed or {}).get('package_versions', {})
        else:
            state = classify_compatibility(installed or {}, running or {}, {**manifest, 'verified': True}, catalog)
            result['compatible'] = state['state'] in ('ready', 'logout_required')
    except (OSError, ValueError, KeyError, TypeError):
        result.update(compatible=False, healthy=False)
    management_error = None
    try:
        selection_path = home / '.local/share/cosmic-desktop-cards/companion/selection.json'
        selection = record(selection_path) if selection_path.exists() else {}
        result['enabled'] = selection.get('enabled') is True
        active = records(home)
        try:
            if active or selection_path.exists():
                verify_owned(home, active)
        except (OSError, ValueError, KeyError, TypeError):
            management_error = 'ownership_conflict'
            raise
        _, _, _, pointer = selected_generation(home, context=context)
        active_generation = pointer['generation']
        current = panel_launcher.session_identity()
        deferred = current is None or current == pointer.get('defer_session')
    except (OSError, ValueError, KeyError, TypeError):
        deferred = True
        if management_error or result['enabled'] or selection_path.exists():
            management_error = management_error or 'active_generation_invalid'
            result['healthy'] = False
            state = dict(state='invalid_bundle', reason=management_error, cohort=None)
    failed = (panel_launcher.state_directory(home) / 'failed').exists()
    if management_error:
        label, reason = 'Compatibility update needed', management_error
    elif failed or not result['enabled']:
        label = 'Compatibility update needed' if failed else 'Islands disabled'
        reason = 'previous_failure' if failed else 'disabled'
    elif state['state'] in ('unsupported', 'invalid_bundle'):
        label, reason = 'Compatibility update needed', state['reason']
    elif deferred or active_generation != generation or state['state'] == 'logout_required':
        label, reason = 'Log out to activate', state['reason']
    elif host is not True:
        label, reason = ('Using standard panel', (installed or {}).get('startup_error') or 'startup_not_ready') if state.get('policy') else ('Checking compatibility', 'host_not_verified')
    else:
        label, reason = 'Ready', state['reason']
    result['applet_issues'] = (installed or {}).get('applet_issues', [])[:64]
    result['running'] = host
    if host is not None and result['healthy'] is not False:
        result['healthy'] = host
    # Eligibility measurements may be supplied by callers, but never describe
    # a running process inventory or establish Ready.
    running_profiles = (running or {}).get('profiles', []) if (running or {}).get('phase') == 'running' else []
    result.update(status_version=2, status=label, reason=reason,
                  installed_generation=generation, active_generation=active_generation,
                  installed_cohort=None, qualification_cohort=state.get('cohort'), runtime_targets=(installed or {}).get('targets', []),
                  running_profiles=running_profiles, compatibility=state)
    # Deliberate allowlist: no filesystem paths, session IDs, environment or
    # exception strings. Diagnostic output cannot change the activation state.
    result['diagnostics'] = dict(version=1, installation=result['installation'],
        provenance=result['provenance'], enabled=result['enabled'], status=label,
        reason=reason, applet_issues=result['applet_issues'], installed_generation=generation, active_generation=active_generation,
        installed_cohort=None, qualification_cohort=state.get('cohort'), runtime_targets=(installed or {}).get('targets', []), running_profiles=running_profiles)
    return result


def query(package, field):
    try:
        return subprocess.check_output(['/usr/bin/dpkg-query', '-W', '-f=' + field, package], stderr=subprocess.DEVNULL, text=True, timeout=1).strip()
    except (OSError, subprocess.SubprocessError):
        return None


def inspect(home, library, installed, versions, initial):
    from bundle_contract import VerificationContext, verify_bundle
    from runtime_identity import installed_identity, running_identity, host_identity
    from journal import selected_generation
    measured = active = host = None
    with VerificationContext() as context:
        try:
            manifest, catalog = verify_bundle(library, 'companion-manifest.json', context=context)
            # Disabled management needs no OS or live-process measurement.
            if initial['enabled']:
                from runtime_compatibility import enabled as family_policy, family, Session
                if family_policy(catalog):
                    measured = family()
                    _, selected, _, _ = selected_generation(home, context=context)
                    owner = host_identity(selected['binary_sha256'])
                    host = False
                    try:
                        observation = Session(versions=measured['package_versions'])
                        try:
                            if owner is not None and selected['payload_sha256'] == manifest['payload_sha256']:
                                host = observation.inventory(owner)
                            measured['applet_issues'] = observation.applet_issues(owner if host else None)
                            observation.check()
                        finally: observation.close()
                    except (OSError, ValueError) as error:
                        host = False
                        code = str(error)
                        measured['startup_error'] = code if re.fullmatch(r'[a-z_]{1,80}',code) else 'startup_check_failed'
                else:
                    measured = installed_identity(catalog)
                    _, selected, _, _ = selected_generation(home, context=context)
                    owner = host_identity(selected['binary_sha256'])
                    active = running_identity(catalog, phase='running', host=owner) if owner else None
                    host = owner is not None
            result = detect(home, library, installed, versions, installed=measured,running=active,host=host,context=context)
            context.check()
            return result
        except (OSError, ValueError, KeyError, TypeError):
            return {**initial,'status':'Compatibility update needed','reason':'bundle_or_measurement_invalid','healthy':False}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--library', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--progress', action='store_true', help='Emit initial provenance and final status as JSON lines')
    parser.add_argument('--presence', action='store_true', help='Installation provenance only; no compatibility admission')
    args = parser.parse_args()
    deadline = time.monotonic()+5
    home = (args.root or Path.home()).resolve()
    if args.root:
        # Explicit fixtures never query host packages, XDG data or session IPC.
        os.environ['XDG_STATE_HOME'] = str(home / '.local/state')
        library = args.library or home / 'package'
        versions = {}
        installed = library.is_dir()
    else:
        library = Path('/usr/lib') / PACKAGE
        versions = {} # Relevant versions are measured only by the bounded verifier.
        try:
            owner = package_owner(record(library / 'companion-manifest.json'))
            installed = installed_package_status(query(owner, '${Status}')) and regular(Path('/usr/bin/halcyon-islands'))
        except (OSError, ValueError):
            installed = False
    if args.root or not installed:
        print(json.dumps(detect(home, library, installed, versions), separators=(',', ':')))
        return
    initial = dict(format=FORMAT,version=1,installation='installed',provenance='package',
        enabled=False,compatible=None,running=None,healthy=None,status_version=2,
        status='Checking compatibility',reason='verification_pending')
    try:
        initial['enabled'] = record(home/'.local/share/cosmic-desktop-cards/companion/selection.json').get('enabled') is True
    except (OSError, ValueError): pass
    if not initial['enabled']:
        initial.update(status='Islands disabled',reason='disabled')
    if args.progress or args.presence: print(json.dumps(initial,separators=(',', ':')), flush=True)
    if args.presence: return
    from verification import bounded
    try:
        result = bounded(lambda: inspect(home, library, installed, versions, initial), max(.001,deadline-time.monotonic()))
    except (OSError, ValueError, TimeoutError):
        result = {**initial, 'reason':'verification_timeout'}
    print(json.dumps(result,separators=(',', ':')))



if __name__ == '__main__':
    main()
