#!/usr/bin/env python3
"""Container stage of Fedora RPM acceptance, signed with a throwaway key.

For each Fedora target, in disposable registry.fedoraproject.org/fedora:N
containers:

1. build the application natively with Rust 1.93.0, locked and offline, from a
   vendored source closure of the committed tree;
2. package it with packaging/build-rpm.py, reusing the previous release's
   qualified Fedora panel, and sign a local repository with a throwaway key;
3. run DNF fresh install, reinstall, removal and reinstall, upgrade from the
   previous released RPM (with an untouched and an administrator-disabled
   source), and package and repository signature rejection checks.

Nothing is published, tagged or signed with a release key. Needs git, cargo,
Python 3.11+, network access and podman or docker usable without sudo:

  python3 scripts/rpm/container_acceptance.py run --previous 0.6.1 --out ../fb25-acceptance

Use --dry-run to print the steps without running them. The output directory
holds acceptance.json, per-stage logs and the throwaway packages; delete it
afterwards and never upload its contents.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
PACKAGE = 'cosmic-ext-halcyon'
RELEASES = 'https://github.com/profilaz3/halcyon/releases/download/v{version}/{name}'
TARGETS = ('43', '44')
IMAGE = 'registry.fedoraproject.org/fedora:{fedora}'
RUST = '1.93.0'
RUSTUP = ('1.28.2', '20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c')
STAGES = ('build', 'fresh', 'upgrade', 'upgrade-disabled')
DNF_CACHE = '/var/cache/libdnf5'
MAX_DOWNLOAD = 512 * 1024 * 1024
ENROLLMENT = ('/etc/yum.repos.d/halcyon.repo', '/etc/pki/rpm-gpg/RPM-GPG-KEY-halcyon',
              '/var/lib/halcyon/updates/rpm-source.json')
NOT_COVERED = ('full COSMIC login session', 'SELinux enforcing', 'installer and PolicyKit prompts',
               'physical GPU and display', 'final release-signed artifacts')


def sha256(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def parse_sums(text):
    sums = {}
    for line in text.splitlines():
        match = re.fullmatch(r'([0-9a-f]{64}) [ *](\S+)', line.strip())
        if match:
            sums[match[2]] = match[1]
    return sums


def previous_rpm(sums, version, fedora):
    pattern = re.compile(re.escape(f'{PACKAGE}-{version}-') + r'([0-9][A-Za-z0-9.]*)'
                         + re.escape(f'.fc{fedora}.x86_64.rpm'))
    names = sorted(name for name in sums if pattern.fullmatch(name))
    if len(names) != 1:
        raise ValueError(f'Expected one Fedora {fedora} RPM for {version} in SHA256SUMS, found {len(names)}')
    return names[0], pattern.fullmatch(names[0])[1]


def default_release(version, previous_version, previous_release):
    """Sort after the previous release; below a final -1 when the version is new."""
    return f'{previous_release}.1.test' if version == previous_version else '0.1.test'


def fetch(url, destination):
    with urllib.request.urlopen(url, timeout=60) as response, destination.open('xb') as output:
        total = 0
        while chunk := response.read(1 << 20):
            total += len(chunk)
            if total > MAX_DOWNLOAD:
                raise ValueError(f'{url} exceeds the download limit')
            output.write(chunk)


def previous_assets(version, folder, targets, source=None):
    """Download or copy the previous release's RPMs and check them against SHA256SUMS."""
    folder.mkdir(parents=True)

    def obtain(name):
        path = folder/name
        if source is not None:
            shutil.copyfile(Path(source)/name, path)
        else:
            fetch(RELEASES.format(version=version, name=name), path)
        return path

    sums = parse_sums(obtain('SHA256SUMS').read_text())
    rpms = {fedora: previous_rpm(sums, version, fedora) for fedora in targets}
    for name in ('release-provenance.json', *(name for name, _ in rpms.values())):
        if name not in sums:
            raise ValueError(f'{name} is not listed in SHA256SUMS')
        if sha256(obtain(name)) != sums[name]:
            raise ValueError(f'{name} does not match SHA256SUMS')
    provenance = json.loads((folder/'release-provenance.json').read_text())
    if provenance.get('release') != version:
        raise ValueError('release-provenance.json belongs to another release')
    return provenance, rpms


def panel_guard(provenance, targets, root=ROOT):
    """Reuse the previous Fedora panel only while its source and policies are unchanged."""
    version = provenance.get('release')
    advice = (f'The Islands panel changed since {version}. This script reuses only an unchanged, '
              'qualified Fedora panel: build it natively with companion/build.py --with-penumbra '
              'and qualify a new Fedora startup policy first.')
    build = load('halcyon_companion_build', root/'companion/build.py')
    native = provenance.get('native', {})
    patches = {path.name: sha256(path) for path in sorted((root/'companion/patches').glob('*.patch'))}
    if native.get('upstream') != build.REVISION or native.get('patches') != patches:
        raise ValueError(advice)
    companion = str(root/'companion')
    added = companion not in sys.path
    if added:
        sys.path.insert(0, companion)
    try:
        policies = load('halcyon_runtime_compatibility', root/'companion/runtime_compatibility.py')
    finally:
        if added:
            sys.path.remove(companion)
    selected = {}
    for fedora in targets:
        policy = getattr(policies, f'FEDORA{fedora}_POLICY', None)
        recorded = provenance.get('platforms', {}).get(f'fedora{fedora}', {}).get('host_sha256')
        if policy is None or recorded != policy.get('binary_sha256'):
            raise ValueError(f'Fedora {fedora}: ' + advice)
        selected[fedora] = policy
    return selected


def catalog(policy):
    """The startup catalog a release built from this tree ships for the policy's platform."""
    record = dict(format='halcyon-islands-catalog', version=2, cohorts=[], startup_policy=policy)
    return json.dumps(record, sort_keys=True, separators=(',', ':'), ensure_ascii=True) + '\n'


def git(*args, root=ROOT):
    return subprocess.run(['git', '-C', str(root), *args], check=True,
                          capture_output=True, text=True).stdout.strip()


def source_tree(out, root=ROOT):
    """Committed files only, as in the corresponding source; never the work tree."""
    commit = git('rev-parse', 'HEAD', root=root)
    dirty = bool(git('status', '--porcelain', '--untracked-files=no', root=root))
    main = out/'source/main'
    main.mkdir(parents=True)
    with subprocess.Popen(['git', '-C', str(root), 'archive', '--format=tar', commit],
                          stdout=subprocess.PIPE) as process:
        with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
            archive.extractall(main, filter='data')
    if process.returncode:
        raise ValueError('git archive failed')
    return commit, dirty, main


def licenses(main, supplied, root=ROOT):
    """The dependency license bundle CI assembles; it is not committed."""
    target = main/'packaging/licenses/THIRD_PARTY_LICENSES.txt'
    if supplied:
        shutil.copyfile(supplied, target)
        return 'supplied'
    if not shutil.which('cargo-about'):
        raise ValueError('Pass --licenses with the CI-built THIRD_PARTY_LICENSES.txt, '
                         'or install cargo-about 0.9.2 to generate it')
    if subprocess.run(['git', '-C', str(root), 'diff', '--quiet', 'HEAD', '--',
                       'Cargo.toml', 'Cargo.lock', 'packaging/licenses']).returncode:
        raise ValueError('Commit the Cargo and license changes first, or pass --licenses')
    work = main.parent.parent/'licenses'
    work.mkdir()
    metadata = subprocess.run(['cargo', 'metadata', '--locked', '--format-version', '1',
                               '--filter-platform', 'x86_64-unknown-linux-gnu'],
                              cwd=root, check=True, capture_output=True).stdout
    (work/'cargo-metadata.json').write_bytes(metadata)
    cargo_home = os.environ.get('CARGO_HOME', str(Path.home()/'.cargo'))
    subprocess.run([sys.executable, str(root/'packaging/collect-licenses.py'),
                    '--metadata', str(work/'cargo-metadata.json'), '--cargo-home', cargo_home,
                    '--allow-missing', '--out', str(work/'source-license-notices.txt')], check=True)
    subprocess.run(['cargo', 'about', 'generate', '--locked', '--fail',
                    '--config', 'packaging/licenses/about.toml', 'packaging/licenses/about.hbs',
                    '--output-file', str(work/'cargo-about.txt')], cwd=root, check=True)
    target.write_bytes((work/'cargo-about.txt').read_bytes()
                       + (work/'source-license-notices.txt').read_bytes())
    return 'generated'


def default_engine():
    return ['podman'] if shutil.which('podman') else ['docker']


def rootless(engine):
    """Whether container root maps to the caller; also proves the engine is reachable."""
    errors = []
    for template, marker in (('{{.Host.Security.Rootless}}', 'true'),
                             ('{{json .SecurityOptions}}', 'rootless')):
        result = subprocess.run([*engine, 'info', '--format', template], capture_output=True, text=True)
        if result.returncode == 0:
            return marker in result.stdout.lower()
        errors.append(result.stderr.strip().splitlines()[-1:] or [''])
    raise ValueError(f'Cannot use {shlex.join(engine)}: {errors[-1][0] or "no response"}')


def image_digest(engine, image):
    result = subprocess.run([*engine, 'image', 'inspect', '--format', '{{json .RepoDigests}}', image],
                            capture_output=True, text=True)
    digests = json.loads(result.stdout or 'null') if result.returncode == 0 else None
    return digests[0] if digests else None


def stage_environment(version, release, previous, host, rootless_engine):
    env = dict(PACKAGE=PACKAGE, VERSION=version, RPM_RELEASE=release, PREVIOUS_RPM='previous/' + previous,
               HOST_SHA256=host, RUST_TOOLCHAIN=RUST, RUSTUP_VERSION=RUSTUP[0], RUSTUP_SHA256=RUSTUP[1])
    if not rootless_engine:
        env.update(HOST_UID=str(os.getuid()), HOST_GID=str(os.getgid()))
    return env


def container_command(engine, image, out, stage, fedora, env):
    command = [*engine, 'run', '--rm', '-v', f'{out}:/work:Z',
               '-v', f'{out}/dnf-cache/fc{fedora}:{DNF_CACHE}:Z']
    for key, value in sorted(env.items()):
        command += ['-e', f'{key}={value}']
    return command + [image, 'bash', '/work/tools/container-stage.sh', stage, fedora]


def read_results(path):
    if not path.is_file():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def plan(args, engine):
    out = args.out.resolve()
    version = tomllib.loads((ROOT/'Cargo.toml').read_text())['package']['version']
    lines = [
        f'# Download v{args.previous} SHA256SUMS, release-provenance.json and the Fedora RPMs '
        f'into {out}/previous; check every hash',
        '# Check the panel revision, patches and Fedora startup policies match that release',
        f'git -C {shlex.quote(str(ROOT))} archive --format=tar HEAD | tar -x -C {out}/source/main',
        f'python3 packaging/source_closure.py vendor --tree {out}/source/main '
        f'--into {out}/source/dependencies/main --offline-check',
        '# Add the dependency license bundle (--licenses, or cargo-about as in CI)',
    ]
    for fedora in args.fedora:
        image = args.image.format(fedora=fedora)
        name = f'{PACKAGE}-{args.previous}-<release>.fc{fedora}.x86_64.rpm'
        env = stage_environment(version, args.rpm_release or '<after previous>', name,
                                f'<release-provenance fedora{fedora} host_sha256>', False)
        lines.append(shlex.join([*engine, 'pull', image]))
        lines += [shlex.join(container_command(engine, image, out, stage, fedora, env)) for stage in STAGES]
    return lines


def run(args):
    engine = shlex.split(args.engine) if args.engine else default_engine()
    if args.dry_run:
        print('\n'.join(plan(args, engine)))
        return 0
    out = args.out.resolve()
    if out.exists():
        raise ValueError('Choose a new output directory')
    rootless_engine = rootless(engine)
    out.mkdir(parents=True)
    provenance, rpms = previous_assets(args.previous, out/'previous', args.fedora, args.previous_assets)
    policies = panel_guard(provenance, args.fedora)
    commit, dirty, main = source_tree(out)
    if dirty:
        print('Note: uncommitted changes are not part of the tested source.', file=sys.stderr)
    version = tomllib.loads((main/'Cargo.toml').read_text())['package']['version']
    release = args.rpm_release or default_release(version, args.previous, rpms[args.fedora[0]][1])
    closure = load('halcyon_source_closure', ROOT/'packaging/source_closure.py')
    source = closure.vendor(main, out/'source/dependencies/main')
    source['offline_packages'] = closure.offline(main)
    license_bundle = licenses(main, args.licenses)
    tools = out/'tools'
    tools.mkdir()
    shutil.copyfile(HERE/'container-stage.sh', tools/'container-stage.sh')
    shutil.copyfile(Path(__file__), tools/'container_acceptance.py')

    report = dict(format='halcyon-rpm-container-acceptance', version=1, published=False,
                  signing='throwaway key per target; never publish these packages',
                  commit=commit, uncommitted_changes_excluded=dirty, product_version=version,
                  rpm_release=release, previous=dict(version=args.previous,
                  rpms={fedora: name for fedora, (name, _) in rpms.items()}),
                  engine=shlex.join(engine), rust=RUST, source_closure=source,
                  license_bundle=license_bundle, targets={}, not_covered=list(NOT_COVERED))
    for fedora in args.fedora:
        image = args.image.format(fedora=fedora)
        subprocess.run([*engine, 'pull', image], check=True)
        (out/'dnf-cache'/f'fc{fedora}').mkdir(parents=True)
        (out/f'fc{fedora}'/'logs').mkdir(parents=True)
        (out/f'fc{fedora}'/'catalog.json').write_text(catalog(policies[fedora]))
        env = stage_environment(version, release, rpms[fedora][0], policies[fedora]['binary_sha256'],
                                rootless_engine)
        target = dict(image=image, image_digest=image_digest(engine, image), stages={})
        for stage in STAGES:
            if stage != 'build' and not target['stages']['build']['passed']:
                target['stages'][stage] = dict(passed=False, skipped='build failed')
                continue
            log = out/f'fc{fedora}'/'logs'/f'{stage}.log'
            with log.open('w') as stream:
                code = subprocess.run(container_command(engine, image, out, stage, fedora, env),
                                      stdout=stream, stderr=subprocess.STDOUT).returncode
            checks = read_results(out/f'fc{fedora}'/'results'/f'{stage}.jsonl')
            passed = code == 0 and bool(checks) and all(check['passed'] for check in checks)
            target['stages'][stage] = dict(exit=code, passed=passed, checks=checks)
            print(f'Fedora {fedora} {stage}: {"passed" if passed else "FAILED"} '
                  f'({sum(check["passed"] for check in checks)}/{len(checks)} checks); log {log}')
        target['passed'] = all(stage['passed'] for stage in target['stages'].values())
        report['targets'][fedora] = target
    report['passed'] = all(target['passed'] for target in report['targets'].values())
    (out/'acceptance.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    print(f'{"Passed" if report["passed"] else "FAILED"}: {out/"acceptance.json"}')
    return 0 if report['passed'] else 1


def provenance_record(fedora, app, host, rust):
    """The checksummed Fedora build record packaging/build-rpm.py requires."""
    return dict(format='halcyon-fedora-build', version=1, fedora=fedora, architecture='x86_64',
                app_sha256=sha256(app), host_sha256=sha256(host), rust=rust, locked=True, offline=True)


def snapshot(root):
    """Enrollment state a package transaction must leave alone, or change as documented."""
    root = Path(root)
    files = {}
    for name in ENROLLMENT:
        path = root/name.lstrip('/')
        files[name] = sha256(path) if path.is_file() else None
    receipt = root/ENROLLMENT[2].lstrip('/')
    phase = json.loads(receipt.read_text()).get('phase') if receipt.is_file() else None
    removed = sorted(path.name for path in (root/'etc/yum.repos.d').glob('halcyon.repo.removed.*'))
    return dict(files=files, phase=phase, removed=removed)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    accept = commands.add_parser('run', help='run the container stage on this machine')
    accept.add_argument('--previous', required=True, help='previous released version, for example 0.6.1')
    accept.add_argument('--out', type=Path, required=True, help='new output directory')
    accept.add_argument('--fedora', nargs='+', choices=TARGETS, default=list(TARGETS))
    accept.add_argument('--engine', help='container command, for example "podman" or "docker"')
    accept.add_argument('--image', default=IMAGE, help='image name with {fedora}')
    accept.add_argument('--rpm-release', help='RPM release of the test build (default sorts after the previous one)')
    accept.add_argument('--previous-assets', type=Path, help='folder with already downloaded release assets')
    accept.add_argument('--licenses', type=Path, help='THIRD_PARTY_LICENSES.txt assembled as in CI')
    accept.add_argument('--dry-run', action='store_true')
    record = commands.add_parser('provenance', help=argparse.SUPPRESS)
    record.add_argument('--fedora', choices=TARGETS, required=True)
    for name in ('app', 'host', 'out'):
        record.add_argument('--' + name, type=Path, required=True)
    record.add_argument('--rust', default=RUST)
    state = commands.add_parser('snapshot', help=argparse.SUPPRESS)
    state.add_argument('--root', type=Path, default=Path('/'))
    args = parser.parse_args(argv)
    try:
        if args.command == 'run':
            return run(args)
        if args.command == 'provenance':
            record = provenance_record(args.fedora, args.app, args.host, args.rust)
            args.out.write_text(json.dumps(record, indent=2, sort_keys=True) + '\n')
        else:
            print(json.dumps(snapshot(args.root), sort_keys=True))
        return 0
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'container-acceptance: {error}\n')


if __name__ == '__main__':
    sys.exit(main())
