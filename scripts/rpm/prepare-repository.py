#!/usr/bin/env python3
"""Prepare and verify a signed DNF repository in a NEW local directory.

No network publication. Final package hashes are calculated AFTER signing.
Use a private fixture key for acceptance; official signing is a later release step.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

PACKAGE='cosmic-ext-halcyon'


def run(args,**kwargs):
    return subprocess.run(args,check=True,text=True,capture_output=True,**kwargs).stdout


def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


def prepare(args):
    if args.out.exists():raise ValueError('Choose a new repository directory')
    if not re.fullmatch('[0-9A-F]{40,64}',args.fingerprint):raise ValueError('Full signing-key fingerprint required')
    if args.fedora not in ('43', '44'):raise ValueError('Only Fedora 43 and 44 x86_64 are supported')
    identity=run(['rpm','-qp','--qf','%{NAME}\t%{VERSION}\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\t%{ARCH}',str(args.package)]).split('\t')
    if len(identity)!=4 or identity[0]!=PACKAGE or identity[3]!='x86_64' or not identity[2].endswith('.fc'+args.fedora):
        raise ValueError('Unexpected native package target')
    if not re.fullmatch(r'\d+\.\d+\.\d+',identity[1]):raise ValueError('Stable product version required')
    keys=run(['gpg','--batch','--with-colons','--show-keys',str(args.public_key)])
    fingerprints=[line.split(':')[9] for line in keys.splitlines() if line.startswith('fpr:')]
    if args.fingerprint not in fingerprints:raise ValueError('Signing key is absent from the supplied public key')
    args.out.mkdir(parents=True)
    destination=args.out/'fedora'/args.fedora/'x86_64';destination.mkdir(parents=True)
    package=destination/args.package.name;shutil.copyfile(args.package,package)
    public=destination/'RPM-GPG-KEY-halcyon';shutil.copyfile(args.public_key,public)
    run(['rpmsign','--define',f'_gpg_path {args.gnupg_home.resolve()}',
         '--define',f'_gpg_name {args.fingerprint}','--addsign',str(package)])
    # Verify in a private RPM database; do not trust or alter the host keyring.
    with tempfile.TemporaryDirectory(prefix='halcyon-rpm-keys-') as db:
        run(['rpmkeys','--dbpath',db,'--import',str(public)])
        verified=run(['rpmkeys','--dbpath',db,'--checksig',str(package)])
        if 'signatures OK' not in verified:raise ValueError('Package signature was not verified')
    run(['createrepo_c','--checksum','sha256',str(destination)])
    metadata=destination/'repodata/repomd.xml'
    run(['gpg','--homedir',str(args.gnupg_home),'--batch','--yes','--local-user',args.fingerprint,
         '--armor','--detach-sign','--output',str(metadata)+'.asc',str(metadata)])
    with tempfile.TemporaryDirectory(prefix='halcyon-repo-keys-') as home:
        run(['gpg','--homedir',home,'--batch','--import',str(public)])
        run(['gpg','--homedir',home,'--batch','--verify',str(metadata)+'.asc',str(metadata)])
    proof={'format':'halcyon-rpm-repository-candidate','version':1,'published':False,
           'target':f'fedora{args.fedora}-x86_64','product_version':identity[1],'package_version':identity[2],
           'signing_fingerprint':args.fingerprint,'artifacts':{str(p.relative_to(args.out)):digest(p)
             for p in sorted(args.out.rglob('*')) if p.is_file()},
           'release_gate':'held; corresponding source and final native acceptance required'}
    (args.out/'candidate-manifest.json').write_text(json.dumps(proof,indent=2)+'\n')
    return proof


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('package','public-key','gnupg-home','out'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--fingerprint',required=True);p.add_argument('--fedora',default='43')
    try:print(json.dumps(prepare(p.parse_args()),indent=2))
    except (OSError,ValueError,subprocess.SubprocessError) as e:p.exit(1,f'prepare-repository: {e}\n')
