"""Read-only ownership and generation inspection. No staging or host actions."""
import os
from pathlib import Path

from bundle_contract import digest, read_json, verify_bundle

PACKAGE = 'cosmic-ext-halcyon-islands'
from bundle_contract import PACKAGE_OWNERS
DATA = '.local/share/cosmic-desktop-cards/'
SELECTOR = '.local/bin/cosmic-panel'
SELECTION = DATA + 'companion/selection.json'
POINTER = DATA + 'companion/generation.json'


def safe_path(root, relative):
    path = root / relative
    if Path(relative).is_absolute() or '..' in Path(relative).parts:
        raise ValueError('Invalid companion install path')
    for part in [path, *path.parents]:
        if part == root:
            break
        if part.is_symlink():
            raise ValueError('A symlink occupies a companion install path')
    return path


def scan(home):
    """Enumerate records only; callers retain their ownership policy."""
    backup = safe_path(home, DATA + 'backups')
    return [(path, read_json(safe_path(home, str(path.relative_to(home)))))
            for path in backup.glob('panel-*/install-record.json')]


def after(saved, relative):
    return {'sha256': saved['installed'][relative], 'mode': saved['modes'][relative]}


def legacy_order(active):
    # Old journals have no sequence. Derive dependencies from actual state
    # transitions, never from filenames or the current wall clock. Cycles or
    # overlapping, noncommuting choices require explicit repair before writes.
    newer = {path: set() for path, _ in active}
    for i, (a, left) in enumerate(active):
        for b, right in active[i + 1:]:
            for rel in left['installed'].keys() & right['installed'].keys():
                la, ra = after(left, rel), after(right, rel)
                lb, rb = left['originals'].get(rel), right['originals'].get(rel)
                if la == rb and (la != ra or lb != la):
                    newer[a].add(b)
                if ra == lb and (ra != la or rb != ra):
                    newer[b].add(a)
    pending = dict(active)
    result = []
    while pending:
        tips = sorted(path for path in pending if not newer[path] & pending.keys())
        if not tips:
            raise ValueError('ambiguous_legacy_install_order')
        for i, a in enumerate(tips):
            for b in tips[i + 1:]:
                left, right = pending[a], pending[b]
                for rel in left['installed'].keys() & right['installed'].keys():
                    if (after(left, rel) != after(right, rel)
                            or left['originals'].get(rel) != right['originals'].get(rel)):
                        raise ValueError('ambiguous_legacy_install_order')
        for path in tips:
            result.append((path, pending.pop(path)))
    return result


def ordered(home, active):
    """Newest first. Explicit predecessors anchor new layers to old journals."""
    legacy = legacy_order([(p, r) for p, r in active if 'journal_version' not in r])
    modern = {str(p.relative_to(home)): (p, r) for p, r in active if 'journal_version' in r}
    for _, saved in modern.values():
        if saved['journal_version'] != 1 or 'previous' not in saved:
            raise ValueError('invalid_install_order')
        if saved['previous'] is not None:
            safe_path(home, saved['previous'])
    result = []
    if modern:
        predecessors = {r['previous'] for _, r in modern.values()}
        tips = modern.keys() - predecessors
        if len(tips) != 1:
            raise ValueError('ambiguous_install_order')
        current = tips.pop()
        while current in modern:
            path, saved = modern.pop(current)
            result.append((path, saved))
            current = saved['previous']
        expected = str(legacy[0][0].relative_to(home)) if legacy else None
        if modern or current != expected:
            raise ValueError('broken_install_order')
    return result + legacy


def records(home):
    active = []
    for path, saved in scan(home):
        if saved.get('root') != str(home) or saved.get('restored'):
            continue
        if saved.get('owner') not in PACKAGE_OWNERS:
            raise ValueError('manual_install_requires_explicit_management')
        active.append((path, saved))
    return ordered(home, active)


def verify_owned(home, active):
    seen = set()
    for _, saved in active:
        if not saved.get('complete'):
            raise ValueError('interrupted_install')
        for relative, sha in saved['installed'].items():
            if relative in seen:
                continue
            seen.add(relative)
            path = safe_path(home, relative)
            if digest(path) != sha or path.stat().st_mode & 0o777 != saved['modes'][relative] or path.stat().st_uid != os.getuid():
                raise ValueError('external_change')
    if SELECTOR not in seen or SELECTION not in seen:
        raise ValueError('unowned_selection')


def selected_generation(home, context=None):
    pointer = read_json(safe_path(home, POINTER))
    generation = pointer['generation']
    if len(generation) != 64 or any(c not in '0123456789abcdef' for c in generation):
        raise ValueError('invalid_generation')
    path = safe_path(home, DATA + 'companion/generations/' + generation)
    if digest(path / 'manifest.json') != generation:
        raise ValueError('generation_identity')
    manifest, catalog = verify_bundle(path, owner=os.getuid(), context=context)
    return path, manifest, catalog, pointer
