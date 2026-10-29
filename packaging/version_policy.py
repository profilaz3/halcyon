"""Release identity shared by package assembly and final artifact checks."""
import json
import re
import subprocess
import tomllib


def main_version(root):
    with (root / 'Cargo.toml').open('rb') as source:
        return tomllib.load(source)['package']['version']


def require_main_version(root, version):
    if version != main_version(root):
        raise ValueError('release_version_must_match_Cargo')


def companion_version(root):
    pin = json.loads((root / 'packaging/islands/download.json').read_text())
    version = pin.get('package_version')
    if (pin.get('format') != 'halcyon-islands-download' or pin.get('version') != 1
            or pin.get('main_version') != main_version(root)
            or not isinstance(version, str) or not re.fullmatch(r'[0-9][A-Za-z0-9.+~-]*', version)):
        raise ValueError('invalid_companion_pair')
    return version


def bundled(root):
    with (root / 'Cargo.toml').open('rb') as source:
        return tomllib.load(source)['package'].get('metadata', {}).get('halcyon', {}).get('bundled-islands') is True


def main_breaks(root):
    if bundled(root):
        return 'cosmic-ext-halcyon-islands (<< 0.5.0)'
    version = companion_version(root)
    return f'cosmic-ext-halcyon-islands (<< {version}), cosmic-ext-halcyon-islands (>> {version})'


def companion_depends(root):
    return f'cosmic-ext-halcyon (>= {main_version(root)})'


def field(package, name):
    try:
        return subprocess.check_output(['dpkg-deb', '-f', str(package), name],
            text=True, stderr=subprocess.DEVNULL, timeout=10).strip()
    except (OSError, subprocess.SubprocessError) as error:
        raise ValueError('package_control_unreadable') from error


def verify_pair(root, main, companion):
    if (field(main, 'Breaks') != main_breaks(root)
            or field(companion, 'Package') != 'cosmic-ext-halcyon-islands'
            or field(companion, 'Version') != companion_version(root)):
        raise ValueError('package_pair_mismatch')
    depends = field(companion, 'Depends').split(',')
    bounds = [d.strip() for d in depends if re.match(r'\s*cosmic-ext-halcyon(?:\s|$)', d)]
    match = re.fullmatch(r'cosmic-ext-halcyon \(>= ([0-9][A-Za-z0-9.+~-]*)\)', bounds[0]) if len(bounds) == 1 else None
    if match is None or subprocess.run(['dpkg', '--compare-versions', main_version(root), 'ge', match[1]], timeout=5).returncode:
        raise ValueError('companion_main_requirement_mismatch')


def verify_main_package(root, package, release):
    require_main_version(root, release)
    try:
        fields = subprocess.check_output([
            'dpkg-deb', '-W', '--showformat=${Package}\n${Version}\n${Architecture}',
            str(package),
        ], text=True, stderr=subprocess.DEVNULL, timeout=10).splitlines()
    except (OSError, subprocess.SubprocessError) as error:
        raise ValueError('main_package_unreadable') from error
    if fields != ['cosmic-ext-halcyon', release, 'amd64']:
        raise ValueError('main_package_identity_mismatch')


def verify_bundled(root, main, transition=None):
    """Bind both executables to one package; the optional bridge has no payload."""
    if (not bundled(root) or field(main, 'X-Halcyon-Islands') != 'bundled'
            or field(main, 'Breaks') != main_breaks(root)
            or field(main, 'Replaces') != main_breaks(root)):
        raise ValueError('bundled_package_contract')
    if transition is not None:
        if (field(transition, 'Package') != 'cosmic-ext-halcyon-islands'
                or field(transition, 'X-Halcyon-Islands') != 'transitional'
                or field(transition, 'Depends') != 'cosmic-ext-halcyon (>= 0.5.0)'
                or field(transition, 'Version') != main_version(root)):
            raise ValueError('transition_package_contract')
        listing = subprocess.check_output(['dpkg-deb', '--fsys-tarfile', str(transition)], timeout=10)
        import io
        import tarfile
        with tarfile.open(fileobj=io.BytesIO(listing)) as archive:
            if any(not item.isdir() for item in archive):
                raise ValueError('transition_must_be_empty')
        scripts = subprocess.check_output(['dpkg-deb', '--ctrl-tarfile', str(transition)], timeout=10)
        with tarfile.open(fileobj=io.BytesIO(scripts)) as archive:
            if any(not item.isdir() and item.name.removeprefix('./') != 'control' for item in archive):
                raise ValueError('transition_has_control_actions')
