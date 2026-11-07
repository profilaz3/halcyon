"""Local package -> immutable user generation. Never starts or restarts a host."""
import argparse
import contextlib
import fcntl
import json
import hashlib
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile

import install
from journal import records, verify_owned, selected_generation
from bundle_contract import canonical, digest, read_json, read_catalog_bytes, verify_bundle, installed_package_status
from panel_launcher import session_identity

PACKAGE = 'cosmic-ext-halcyon-islands'
from bundle_contract import package_owner
DATA = '.local/share/cosmic-desktop-cards/'
SELECTOR = '.local/bin/cosmic-panel'
SELECTION = DATA + 'companion/selection.json'
POINTER = DATA + 'companion/generation.json'
LIBRARY = Path('/usr/lib') / PACKAGE
# Every panel_launcher.py/selector.py Halcyon has shipped. A reinstall or a
# removed data directory can leave one of these at ~/.local/bin/cosmic-panel
# with no journal. Byte-identical copies are ours, so enable may adopt them.
SHIPPED_SELECTORS = frozenset((
    '044db18fd5256083a0af235ec61381984ba90545a48e1a2ea90eb5308ef53617',
    '1b5d57b52bfcb10615016c3f9ccb5300045ec11037d02cba877b69c985d01d14',
    '2445c851cbd274a5de841e11119904f0532044af15a9e9fc34e300e60a0a926e',
    '2afd5dc066e8ca674cf817c364f61769e8dff37a589162d07c296ac93d60db8c',
    '2b3374a53c7357bd632908e1c8e1fce31b75f9c2fdac5958985a1e9054c4ad58',
    '437fdc03a6c9e4cf99aab3c40c2e6b556e919aff5ead758a9e0d3d9120c1d29b',
    '60106cc52516fdff082aa77269e3112b816c7f39320c09232989407efc121187',
    '69c504b6cb094b79f08f0c6c913252864317347596f8203956f9a1486bf0c2d5',
    '6da8cf6e189d7fdd877fcbd3e119c2567ddbb699a332e5849a63157da4271de0',
    '835d960e44ede612920472e56b76cf2403afafe2070b1306e4cb1aba74ec545d',
    '83d839904cb5e44978ea7a74ddb3d5d44039b745ba71f9f706f11912e5e29507',
    '8ee52f89a0b215c6074ebc93ff14662bcc7df5aaed6b4c73752720e949f8ed97',
    '90f89a3b6dae5750f545d81e6825a4b40ea47af5e7bf62852c5fbb42e647e260',
    '9cb6b5514dc600d7328adbe9ff2524cfdfe3eee27354daf516aaba3e19627edf',
    '9ec2e47f8b3e20b608a1ec3c28eed216d948ceed97be2b38b7d5c497f60cbd7d',
    'ac1c56d1ed3b83b5f0f4a2afbb277defb6b4491d9154c04176bfad3ba50e5be5',
    'ac5285fa80a8d665254acbc9434fed79c27fa0c7d748b9e28ee4c807a4f4e851',
    'c0e77659ab6f1df7e2695ad10039fd0abca81269813d3dd23f32cef89f0418a8',
    'd2fc6ca6781785efe535584abf6f99967b6f502b2d85c782fc3d27dad22190f0',
    'db761a0bf455eadf35b75e34d2ed524e5b77bb0b6f33b20cff2fe4f39d969948',
    'dcfc9185f6f27695da5c95b1de46688e91b9a3d2f033576ab0228c134d91c085',
    'df6a8d1b2d9976656a12b44fa69d2d3f50239007bddf876098ad5c0e5a53aa36',
))


def adopt_orphans(home, library):
    """Remove Halcyon's own unjournaled selector/pointer; False if anything is foreign."""
    # Parents must not be symlinks; a symlinked leaf is reported as foreign.
    selector = install.safe_path(home, str(Path(SELECTOR).parent)) / Path(SELECTOR).name
    pointer = install.safe_path(home, str(Path(POINTER).parent)) / Path(POINTER).name
    known = SHIPPED_SELECTORS | {digest(library / name) for name in ('panel_launcher.py', 'selector.py')
                                 if (library / name).is_file()}
    if os.path.lexists(selector):
        info = os.lstat(selector)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                or digest(selector) not in known):
            return False
    if os.path.lexists(pointer) and not stat.S_ISREG(os.lstat(pointer).st_mode):
        return False
    for path in (selector, pointer):
        if os.path.lexists(path):
            os.unlink(path)
    return True


def response(state, reason, generation=None):
    return dict(state=state, reason=reason, generation=generation,
                next_login=state == 'staged', runtime_readiness='not_measured')


@contextlib.contextmanager
def locked(home):
    path = install.safe_path(home, DATA + 'backups/.islands.lock')
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield
    finally:
        os.close(fd)


def recover_interrupted(home):
    for path, saved in records(home):
        if not saved.get('complete'):
            # Existing restore validates all before/current bytes and is itself
            # replayable. Never roll back an external edit to finish an upgrade.
            install.restore(path)
        else:
            break


def stage_installed_update(home, installed_bundle, *, session=None, fixture=False, boundary=lambda _: None):
    home = home.resolve()
    try:
        with locked(home):
            return stage_locked(home, installed_bundle, session=session, fixture=fixture, boundary=boundary)
    except BlockingIOError:
        return response('blocked', 'busy')
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        return response('blocked', 'unsafe_installation')


def require_selector_path(home):
    paths = os.environ.get('PATH', '').split(os.pathsep)
    local = str(home / '.local/bin')
    if local not in paths or '/usr/bin' not in paths or paths.index(local) > paths.index('/usr/bin'):
        raise ValueError('The session PATH must put ~/.local/bin before /usr/bin')


def stage_locked(home, library, *, session=None, fixture=False, boundary=lambda _: None, enable=False):
    from bundle_contract import VerificationContext
    from runtime_identity import FileMeasurement
    with VerificationContext() as context, FileMeasurement() as guard:
        return _stage_locked(home, library, session=session, fixture=fixture, boundary=boundary, enable=enable, context=context, guard=guard)


def _stage_locked(home, library, *, session, fixture, boundary, enable, context, guard):
    if not fixture and Path(os.environ.get('XDG_DATA_HOME', home / '.local/share')) != home / '.local/share':
        return response('blocked', 'unsupported_data_location')
    if enable and not fixture:
        require_selector_path(home)
    recover_interrupted(home)
    active = records(home)
    if not active and not enable:
        return response('disabled', 'not_owned_enabled')
    if not active and not adopt_orphans(home, library):
        return response('external_change', 'unowned_selector')
    try:
        if active:
            verify_owned(home, active)
    except (OSError, ValueError):
        return response('external_change', 'owned_files_changed')
    selection = read_json(install.safe_path(home, SELECTION)) if active else {'enabled': enable}
    if selection.get('enabled') is not True:
        return response('disabled', 'disabled')
    if not fixture and (library != LIBRARY or library.is_symlink()):
        return response('blocked', 'not_installed_package')
    manifest, catalog = verify_bundle(library, 'companion-manifest.json', owner=os.getuid() if fixture else 0, context=context)
    owner = package_owner(manifest)
    from runtime_identity import rpm_system, package_version, watch_package_database, version_newer
    if not fixture:
        if rpm_system():
            installed_version = package_version(owner)
            if installed_version is None: return response('blocked', 'package_absent')
            fields = ['install ok installed', installed_version]
        else:
            result = subprocess.run(['/usr/bin/dpkg-query', '-W', '-f=${Status}\n${Version}', owner], capture_output=True, text=True, timeout=3)
            fields = result.stdout.splitlines()
            if result.returncode or len(fields) != 2 or not installed_package_status(fields[0]):
                return response('blocked', 'package_absent')
    if not fixture and manifest['package_version'] != fields[1]:
        return response('blocked', 'package_version_mismatch')
    current = session if session is not None else session_identity()
    if not current:
        return response('blocked', 'session_unknown')
    manifest_bytes = (library / 'companion-manifest.json').read_bytes()
    if json.loads(manifest_bytes) != manifest:
        return response('blocked', 'package_changed_during_staging')
    generation = hashlib.sha256(manifest_bytes).hexdigest()
    old_version = '0.2.0' if active else '0'
    try:
        _, old, _, pointer = selected_generation(home, context=context)
        old_version = old['package_version']
        if pointer['generation'] == generation:
            return response('unchanged', 'already_staged', generation)
    except FileNotFoundError:
        if install.safe_path(home, POINTER).exists():
            return response('external_change', 'generation_missing')
    from runtime_compatibility import enabled as family_policy, family, validate_platform
    from verification import bounded
    if family_policy(catalog):
        watch_package_database(guard.watch)
        try:
            if not fixture: validate_platform(catalog)
            # Staging is eligibility only. Actual interfaces, configuration and
            # child ownership are checked at the next launch, never live-swapped.
            if fixture:
                family(read_json(home / 'installed-identity.json').get('package_versions', {}), policy=catalog['startup_policy'])
            else:
                bounded(lambda: family(policy=catalog['startup_policy']), 2)
        except (OSError, ValueError, TimeoutError, subprocess.SubprocessError):
            return response('blocked', 'unsupported_cosmic_family')
    else:
        if not catalog['cohorts']:
            return response('blocked', 'no_qualified_cohort')
        from runtime_identity import installed_identity
        from runtime_target import installed_matches
        from verification import bounded
        if not fixture:
            watch_package_database(guard.watch)
            for row in catalog['cohorts']:
                for path in row.get('runtime_target', {}).get('installed_files', {}): guard.resolve(Path(path))
        installed = bounded(lambda: installed_identity(catalog), 5) if not fixture else read_json(home / 'installed-identity.json')
        if not any(installed_matches(installed, r)
                   and r['qualification'] == 'native-session' and r['payload_sha256'] == manifest['payload_sha256'] for r in catalog['cohorts']):
            return response('blocked', 'installed_runtime_unqualified')
    # A changed payload at the same version is also an unapproved replacement.
    if not version_newer(manifest['package_version'], old_version):
        return response('blocked', 'non_monotonic_version')
    generations = install.safe_path(home, DATA + 'companion/generations')
    generations.mkdir(parents=True, exist_ok=True, mode=0o700)
    target = generations / generation
    if not target.exists():
        with tempfile.TemporaryDirectory(prefix='.stage-', dir=generations) as temporary:
            staged = Path(temporary) / generation
            staged.mkdir(mode=0o700)
            for name, info in manifest['files'].items():
                data = (read_catalog_bytes(library / name, info['sha256']) if name == 'catalog.json'
                        else (library / name).read_bytes())
                install.place(staged / name, data, info['mode'])
            install.place(staged / 'manifest.json', manifest_bytes, 0o644)
            verify_bundle(staged, owner=os.getuid())
            boundary('copied')
            os.rename(staged, target)
            install.sync_directory(generations)
    verify_bundle(target, owner=os.getuid())
    if digest(target / 'manifest.json') != generation:
        raise ValueError('generation_changed')
    selector_bytes = (target / 'selector.py').read_bytes()
    selector_info = manifest['files']['selector.py']
    if hashlib.sha256(selector_bytes).hexdigest() != selector_info['sha256'] or selector_info['mode'] != 0o755:
        raise ValueError('generation_selector_mismatch')
    boundary('generation')
    pointer = dict(version=1, generation=generation, package_version=manifest['package_version'], defer_session=current)
    contents = {
        POINTER: (canonical(pointer) + b'\n', 0o600),
        SELECTOR: (selector_bytes, selector_info['mode']),
    }
    if not active:
        contents[SELECTION] = (canonical({'enabled': True, 'defer_session': current}) + b'\n', 0o600)
    # install_contents journals before the pointer or selector changes. Its
    # rollback restores the previous selector/pointer, preserving the 0.2.0
    # chain and the old flat payload. Immutable orphan generations are harmless.
    boundary('before_selection')
    guard.check()
    install.install_contents(home, contents, owner)
    boundary('selected')
    return response('staged', 'next_login_after_staging', generation)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--library', type=Path, default=LIBRARY, help=argparse.SUPPRESS)
    args = parser.parse_args()
    home = args.root or Path.home()
    # Only default XDG layout is supported by the historical ownership chain.
    if not args.root and 'COSMIC' not in os.environ.get('XDG_CURRENT_DESKTOP', '').split(':'):
        result = response('blocked', 'not_cosmic_session')
    elif not args.root and Path(os.environ.get('XDG_DATA_HOME', home / '.local/share')) != home / '.local/share':
        result = response('blocked', 'unsupported_data_location')
    else:
        result = stage_installed_update(home, args.library, fixture=args.root is not None)
    print(json.dumps(result))


if __name__ == '__main__':
    main()
