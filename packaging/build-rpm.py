#!/usr/bin/env python3
"""Build one native Fedora package from qualified Fedora-built executables.

Produces local artifacts only. Repository signing and publication are separate.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = 'cosmic-ext-halcyon'


def module(name, path):
    spec = importlib.util.spec_from_file_location(name,path)
    result = importlib.util.module_from_spec(spec); spec.loader.exec_module(result)
    return result


def build(args):
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:[.~][A-Za-z0-9.]+)?',args.version):
        raise ValueError('Invalid product version')
    if not re.fullmatch(r'[0-9][A-Za-z0-9.]*',args.release) or not re.fullmatch(r'\d{2}',args.fedora):
        raise ValueError('Invalid RPM release or Fedora target')
    if subprocess.check_output(['rpm','--eval','%{_arch}'],text=True).strip() != 'x86_64':
        raise ValueError('Only x86_64 is supported')
    app = module('halcyon_deb_payload',ROOT/'packaging/build-deb.py')
    app.require_main_version(ROOT,args.version)
    native = module('halcyon_native_payload',ROOT/'packaging/islands/build-deb.py')
    proof = json.loads(args.provenance.read_text())
    expected = dict(format='halcyon-fedora-build',version=1,fedora=args.fedora,architecture='x86_64',
                    app_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                    host_sha256=hashlib.sha256(args.islands_binary.read_bytes()).hexdigest())
    if any(proof.get(k) != v for k,v in expected.items()):
        raise ValueError('Fedora build provenance does not match this target and both executables')
    evr = f'0:{args.version}-{args.release}.fc{args.fedora}'
    args.out.mkdir(parents=True,exist_ok=True)
    wallpapers = sorted((args.wallpapers or app.default_wallpapers()).glob('*.jpg'))
    if not wallpapers: raise ValueError('Missing default wallpapers')
    if not args.public_key.read_bytes().startswith(b'-----BEGIN PGP PUBLIC KEY BLOCK-----'):
        raise ValueError('An armored public signing key is required')
    with tempfile.TemporaryDirectory(prefix='halcyon-rpm-') as temporary:
        top = Path(temporary); payload = top/'payload'
        app.stage_application(payload,args.binary,wallpapers,args.google_client,PACKAGE)
        native.stage_payload(payload,args.islands_binary,args.islands_manifest,evr,
                             args.islands_licenses,args.islands_catalog,owner=PACKAGE,strip_binary=False)
        metadata = dict(format='halcyon-installation',version=1,app_version=args.version,
                        package_version=evr,package_format='rpm',architecture='x86_64')
        target = payload/'usr/share'/PACKAGE/'installation.json'
        target.parent.mkdir(parents=True,exist_ok=True);target.write_text(json.dumps(metadata,sort_keys=True)+'\n')
        updates = payload/'usr/share'/PACKAGE/'updates-rpm'
        app.install(ROOT/'packaging/updates/halcyon.repo',updates/'source.repo')
        app.install(args.public_key,updates/'archive-key.asc')
        app.install(ROOT/'packaging/updates/rpm_defaults.py',payload/'usr/lib'/PACKAGE/'rpm-enrollment.py',0o755)
        app.install(args.provenance,payload/'usr/share/doc'/PACKAGE/'fedora-build.json')
        files = []
        for file in sorted(payload.rglob('*')):
            if not file.is_file(): continue
            relative = '/'+str(file.relative_to(payload))
            if '%' in relative or '\n' in relative or ' ' in relative: raise ValueError('Unexpected RPM payload path')
            files.append(('%config(noreplace) ' if relative.startswith('/etc/') else '')+relative)
        (top/'payload-files').write_text('\n'.join(files)+'\n')
        spec = top/'halcyon.spec'
        spec.write_text(f'''Name: {PACKAGE}
Version: {args.version}
Release: {args.release}.fc{args.fedora}
Summary: Desktop widgets and themes for COSMIC
License: GPL-3.0-or-later
URL: https://github.com/profilaz3/halcyon
ExclusiveArch: x86_64
Requires: python3, git, systemd, cosmic-panel, cosmic-comp, cosmic-session, cosmic-settings-daemon
Requires: PackageKit, pulseaudio-utils
Requires(post): python3
Requires(preun): python3
%global debug_package %{{nil}}
# Exact reviewed payload hashes must survive packaging; symbols are stripped
# before the native manifest is finalized.
%global __os_install_post %{{nil}}

%description
Desktop widgets, coordinated appearances and optional Islands and Penumbra for
COSMIC. One package updates the application and panel component together.
Fresh installs configure the signed Halcyon update source. Existing source
choices are preserved. The system updater owns installation and authorization.

%install
mkdir -p %{{buildroot}}
cp -a {payload}/. %{{buildroot}}/

%post
/usr/bin/python3 -I /usr/lib/{PACKAGE}/rpm-enrollment.py configure

%preun
if [ "$1" -eq 0 ]; then
  /usr/bin/python3 -I /usr/lib/{PACKAGE}/rpm-enrollment.py remove
fi

%files -f {top}/payload-files
''')
        subprocess.run(['rpmbuild','-bb','--define',f'_topdir {top}','--define',f'_rpmdir {args.out.resolve()}',str(spec)],check=True)
    artifact = args.out/'x86_64'/f'{PACKAGE}-{args.version}-{args.release}.fc{args.fedora}.x86_64.rpm'
    if not artifact.is_file(): raise ValueError('RPM build did not produce the expected artifact')
    artifact.with_suffix('.rpm.sha256').write_text(hashlib.sha256(artifact.read_bytes()).hexdigest()+'  '+artifact.name+'\n')
    print(artifact)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('binary','islands-binary','islands-manifest','islands-licenses','islands-catalog','provenance','public-key','out'):
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--version',required=True)
    parser.add_argument('--release',default='0.1.next')
    parser.add_argument('--fedora',default='43')
    parser.add_argument('--wallpapers',type=Path)
    parser.add_argument('--google-client',type=Path)
    try: build(parser.parse_args())
    except (OSError,ValueError,subprocess.SubprocessError) as error: parser.exit(1,f'build-rpm: {error}\n')
