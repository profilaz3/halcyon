#!/usr/bin/python3
"""One-time DNF enrollment. Local files only; no keys imported or services run."""
import hashlib
import json
import os
from pathlib import Path
import stat
import tempfile

BASE = '/usr/share/cosmic-ext-halcyon/updates-rpm'
STATE = '/var/lib/halcyon/updates/rpm-source.json'
SOURCE = '/etc/yum.repos.d/halcyon.repo'
KEY = '/etc/pki/rpm-gpg/RPM-GPG-KEY-halcyon'
TARGETS = {'archive-key.asc': KEY, 'source.repo': SOURCE}
FORMAT = 'halcyon-rpm-updates'


def path(root, name):
    return root / name.lstrip('/')


def read(file, limit=65536):
    try:
        fd = os.open(file, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return None
    with os.fdopen(fd, 'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError('Not a regular enrollment file')
        data = stream.read(limit + 1)
        if len(data) > limit: raise ValueError('Enrollment file exceeds its limit')
        return data


def sync_directory(folder):
    fd = os.open(folder, os.O_DIRECTORY)
    try: os.fsync(fd)
    finally: os.close(fd)


def write(file, data, replace=False):
    file.parent.mkdir(parents=True, exist_ok=True)
    fd, staging = tempfile.mkstemp(prefix='.halcyon-enroll-', dir=file.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            os.fchmod(stream.fileno(), 0o644)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        if replace: os.replace(staging, file)
        else: os.link(staging, file, follow_symlinks=False)
        sync_directory(file.parent)
    finally: Path(staging).unlink(missing_ok=True)


def receipt(root, value):
    write(path(root, STATE), (json.dumps(value, sort_keys=True)+'\n').encode(), replace=True)


def existing(root, allowed=()):
    allowed = {path(root, n) for n in allowed}
    for name in TARGETS.values():
        if path(root, name) not in allowed and os.path.lexists(path(root, name)):
            return True
    folder = path(root, '/etc/yum.repos.d')
    if not folder.exists(): return False
    candidates = list(folder.iterdir())
    if len(candidates) > 256: return True
    for file in candidates:
        if file in allowed or file.name.startswith('.halcyon-enroll-'): continue
        if 'halcyon' in file.name.lower(): return True
        try: content = read(file, 1024*1024) or b''
        except (OSError, ValueError): return True
        if b'halcyon' in content.lower(): return True
    return False


def configure(root):
    try:
        raw = read(path(root, STATE))
        state = json.loads(raw) if raw is not None else None
    except (OSError, ValueError): return
    if state is not None and (not isinstance(state, dict) or
            state.get('format') != FORMAT or state.get('version') != 1): return
    if state is not None and state.get('phase') not in ('pending','complete','key-update'): return
    payload = {name: read(path(root, BASE+'/'+name)) for name in TARGETS}
    if any(not data for data in payload.values()): raise ValueError('Missing packaged enrollment files')
    hashes = {TARGETS[name]: hashlib.sha256(data).hexdigest() for name, data in payload.items()}
    if state is not None and state['phase'] in ('complete','key-update'):
        # A signed package may deliver overlapping public keys before the
        # repository changes signer. Rotate only our unchanged enrollment;
        # never recreate a removed key/source or adopt an administrator edit.
        old=state.get('files',{});current={}
        try:
            for target in TARGETS.values():
                data=read(path(root,target))
                if data is None:return
                current[target]=hashlib.sha256(data).hexdigest()
        except (OSError,ValueError):return
        if current.get(SOURCE)!=old.get(SOURCE) or hashes.get(SOURCE)!=old.get(SOURCE):return
        if existing(root,TARGETS.values()):return
        if current.get(KEY) not in (old.get(KEY),hashes.get(KEY)):return
        if old.get(KEY)==hashes.get(KEY) and state['phase']=='complete':return
        state['phase']='key-update';receipt(root,state)
        if current[KEY]!=hashes[KEY]:write(path(root,KEY),payload['archive-key.asc'],replace=True)
        state.update(phase='complete',files=hashes);receipt(root,state);return
    if state is None:
        state = dict(format=FORMAT, version=1, phase='preserved' if existing(root) else 'pending', files=hashes)
        receipt(root, state)
        if state['phase'] == 'preserved': return
    if state.get('files') != hashes:
        state['phase'] = 'conflict'; receipt(root, state); return
    missing = []
    for name, target in TARGETS.items():
        file = path(root, target)
        try: current = read(file)
        except (OSError, ValueError): current = b''
        if current is None: missing.append((file, payload[name]))
        elif current != payload[name]:
            state['phase'] = 'conflict'; receipt(root, state); return
    if existing(root, TARGETS.values()):
        state['phase'] = 'preserved'; receipt(root, state); return
    # Public key first; publish the source only after its dependency is durable.
    for file, data in missing: write(file, data)
    state['phase'] = 'complete'; receipt(root, state)
    print('Halcyon system updates enabled. Your system updater manages future updates.')


def remove(root):
    try:
        state = json.loads(read(path(root, STATE)) or b'null')
        if not isinstance(state,dict) or state.get('format') != FORMAT or state.get('version') != 1: return
        source = path(root, SOURCE)
        data = read(source)
        if data is not None and hashlib.sha256(data).hexdigest() == state.get('files',{}).get(SOURCE):
            fd, saved = tempfile.mkstemp(prefix='halcyon.repo.removed.', dir=source.parent)
            os.close(fd)
            os.replace(source, saved)
            sync_directory(source.parent)
        # Keep trust material: an administrator may have another active source.
        # The receipt prevents a later reinstall undoing the disabled decision.
        state['phase'] = 'disabled'; receipt(root, state)
    except (OSError, ValueError): return


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('configure','remove'))
    parser.add_argument('--root', type=Path, default=Path('/'), help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not args.root.is_absolute(): parser.error('root must be absolute')
    (configure if args.action == 'configure' else remove)(args.root)
