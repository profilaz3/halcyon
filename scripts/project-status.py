#!/usr/bin/env python3
"""Read local repository and listening-process status without changing a repository."""
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from pathlib import Path
import argparse
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
import time

SKIP = {'.git', 'node_modules', 'target', 'dist', 'build', 'venv', '.venv', '__pycache__'}


# Leave time for JSON output and process cleanup before the app's 25s deadline.
SCAN_BUDGET = 20


def remaining(deadline, limit=4):
    seconds = limit if deadline is None else min(limit, deadline - time.monotonic())
    if seconds <= 0:
        raise subprocess.TimeoutExpired('project scan', 0)
    return seconds


def command(args, timeout=4, deadline=None):
    return subprocess.run(args, capture_output=True, timeout=remaining(deadline, timeout), check=True).stdout


def git_command(path, args, deadline=None):
    env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    env.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_SYSTEM=os.devnull,
               GIT_CONFIG_GLOBAL=os.devnull, GIT_ATTR_NOSYSTEM='1',
               GIT_NO_LAZY_FETCH='1', GIT_TERMINAL_PROMPT='0', GIT_ALLOW_PROTOCOL='')
    options = ['core.fsmonitor=false', 'core.hooksPath=' + os.devnull,
               'core.sshCommand=' + os.devnull, 'core.pager=', 'diff.external=',
               'core.attributesFile=' + os.devnull, 'protocol.allow=never',
               'maintenance.auto=false', 'gc.auto=0']
    argv = ['git', '--no-pager', '--no-optional-locks']
    for option in options:
        argv.extend(['-c', option])
    return subprocess.run([*argv, '-C', str(path), *args], env=env,
                          capture_output=True, timeout=remaining(deadline), check=True).stdout


def status(path, deadline=None):
    git_dir = Path(os.fsdecode(git_command(path, ['rev-parse', '--absolute-git-dir'], deadline=deadline)).rstrip('\n'))
    common = Path(os.fsdecode(git_command(path, ['rev-parse', '--path-format=absolute', '--git-common-dir'], deadline=deadline)).rstrip('\n'))
    config = git_command(path, ['config', '--null', '--list'], deadline=deadline).split(b'\0')
    settings = []
    for entry in config:
        if not entry:
            continue
        key, separator, value = entry.partition(b'\n')
        key, value = os.fsdecode(key), os.fsdecode(value) if separator else 'true'
        if key.startswith('filter.'):
            raise ValueError('Repository filters make working-tree status unavailable')
        if key in {'core.repositoryformatversion', 'core.filemode', 'core.symlinks',
                   'core.ignorecase', 'core.precomposeunicode', 'core.autocrlf', 'core.eol',
                   'extensions.objectformat', 'extensions.refstorage'} or re.fullmatch(
                       r'branch\..+\.(remote|merge)|remote\..+\.fetch', key):
            settings.append((key, value))
    # A private metadata view reads only data settings, never the repository's
    # live config. A concurrent config change therefore cannot enable filters.
    with tempfile.TemporaryDirectory(prefix='halcyon-git-') as temporary:
        view = Path(temporary)
        for name in ['objects', 'refs', 'packed-refs', 'shallow', 'reftable', 'info']:
            source = common / name
            if source.exists():
                (view / name).symlink_to(source)
        (view / 'HEAD').write_bytes((git_dir / 'HEAD').read_bytes())
        for source in [git_dir / 'index', *git_dir.glob('sharedindex.*')]:
            if source.exists():
                (view / source.name).symlink_to(source)
        def quoted(value):
            return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\t', '\\t').replace('\b', '\\b') + '"'
        lines = ['[core]\n\tbare = false\n']
        for key, value in settings:
            section, name = key.rsplit('.', 1)
            if '.' in section:
                section, subsection = section.split('.', 1)
                section += ' ' + quoted(subsection)
            lines.append('[' + section + ']\n\t' + name + ' = ' + quoted(value) + '\n')
        (view / 'config').write_text(''.join(lines), errors='surrogateescape')
        raw = git_command(path, ['--git-dir=' + str(view), '--work-tree=' + str(path),
                                'status', '--porcelain=v2', '--branch', '-z',
                                '--untracked-files=normal', '--ignore-submodules=all'], deadline=deadline)
    return raw, git_dir


def discover(roots, deadline=None):
    repos = set()
    for item in roots:
        base = Path(item).expanduser().resolve()
        if not base.is_dir():
            continue
        visited = 0
        for current, directories, files in os.walk(base):
            if deadline is not None and time.monotonic() >= deadline:
                break
            visited += 1
            depth = len(Path(current).relative_to(base).parts)
            if '.git' in directories or '.git' in files:
                repos.add(Path(current))
                directories[:] = []
            else:
                directories[:] = [d for d in directories if d not in SKIP and not d.startswith('.')]
                if depth >= 4:
                    directories[:] = []
            if visited >= 600 or len(repos) >= 48:
                break
    # Include linked worktrees so a running branch is not mistaken for the main checkout.
    for repo in sorted(repos):
        try:
            entries = git_command(repo, ['worktree', 'list', '--porcelain', '-z'], deadline=deadline).split(b'\0')
            for entry in entries:
                if entry.startswith(b'worktree '):
                    path = Path(os.fsdecode(entry[9:]))
                    if path.is_dir():
                        repos.add(path.resolve())
        except (OSError, subprocess.SubprocessError):
            pass
    return sorted(repos)[:64]


def parse_status(raw):
    state = {'branch': 'Unknown', 'changed': 0, 'untracked': 0, 'conflicts': 0,
             'ahead': 0, 'behind': 0, 'upstream': False}
    oid = ''
    entries = iter(raw.decode('utf-8', 'replace').split('\0'))
    for entry in entries:
        if entry.startswith('# branch.head '):
            state['branch'] = entry[14:]
        elif entry.startswith('# branch.oid '):
            oid = entry[13:]
        elif entry.startswith('# branch.upstream '):
            state['upstream'] = True
        elif entry.startswith('# branch.ab '):
            fields = entry.split()
            state['ahead'], state['behind'] = int(fields[2][1:]), int(fields[3][1:])
        elif entry.startswith(('1 ', '2 ', 'u ')):
            state['changed'] += 1
            if entry.startswith('u '):
                state['conflicts'] += 1
            if entry.startswith('2 '):
                next(entries, None)  # Rename source path is a separate NUL-delimited field.
        elif entry.startswith('? '):
            state['untracked'] += 1
    if state['branch'] == '(detached)':
        state['branch'] = (oid[:8] or 'HEAD') + ' · detached'
    return state


def listeners(deadline=None):
    found = []
    try:
        rows = command(['ss', '-H', '-ltnp'], deadline=deadline).decode('utf-8', 'replace').splitlines()
    except (OSError, subprocess.SubprocessError):
        return found
    for line in rows:
        fields = line.split()
        if len(fields) < 5:
            continue
        try:
            port = int(fields[3].rsplit(':', 1)[1])
        except (ValueError, IndexError):
            continue
        for process, pid in re.findall(r'\("([^"]+)",pid=(\d+)', line):
            try:
                proc = Path('/proc') / pid
                if proc.stat().st_uid != os.getuid():
                    continue
                found.append({'port': port, 'process': process, 'cwd': (proc / 'cwd').resolve(strict=True)})
            except OSError:
                continue
    return found


def development_port(port):
    return any(low <= port <= high for low, high in
               [(3000, 3010), (4000, 4010), (4173, 4180), (5000, 5010), (5173, 5190),
                (8000, 8010), (8080, 8090), (8787, 8790), (9000, 9010)])


def readable_title(value):
    """Accept a title, never a filesystem destination or an unknown placeholder."""
    if not isinstance(value, str) or not value or len(value) > 120:
        return None
    if any(not char.isprintable() for char in value) or any(char in value for char in '/\\'):
        return None
    value = re.sub(r'[-_][a-fA-F0-9]{6,}$', '', value.strip())
    if not value or value.casefold() in {'unknown', 'none', 'null', 'untitled'}:
        return None
    if re.fullmatch(r'[a-fA-F0-9]{6,}', value):
        return None
    return value


def project_title(root):
    # Scaffold package names such as vite_react_shadcn_ts are often unchanged.
    # Only explicit product metadata overrides the stable repository folder.
    manifest = root / 'package.json'
    try:
        limit = 128 * 1024
        fd = os.open(manifest, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
        with os.fdopen(fd, 'rb') as stream:
            info = os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
                raise ValueError('Not bounded regular project metadata')
            encoded = stream.read(limit + 1)
        if len(encoded) <= limit:
            data = json.loads(encoded.decode('utf-8'))
            if isinstance(data, dict):
                for key in ['displayName', 'productName']:
                    if title := readable_title(data.get(key)):
                        return title
    except (OSError, ValueError, UnicodeError):
        pass
    name = root.name if root.name != 'v2' else root.parent.name + ' v2'
    name = readable_title(name)
    if not name:
        return 'Untitled project'
    return ' '.join(word[0].upper() + word[1:] for word in re.split(r'[-_\s]+', name) if word)


def repository(path, sockets, deadline=None):
    path = path.resolve()
    root, identity = path, str(path)
    try:
        common_dir = Path(os.fsdecode(git_command(path,
            ['rev-parse', '--path-format=absolute', '--git-common-dir'], deadline=deadline
        )).rstrip('\n')).resolve()
        identity = str(common_dir)
        if common_dir.name == '.git':
            root = common_dir.parent
        else:
            # Separate Git directories can identify the primary checkout with
            # core.worktree; it is not reliably returned by `worktree list`.
            root = common_dir
            try:
                configured = Path(os.fsdecode(git_command(path,
                    ['config', '--path', '--get', 'core.worktree'], deadline=deadline
                )).rstrip('\n'))
                root = configured if configured.is_absolute() else (common_dir / configured).resolve()
            except (OSError, subprocess.SubprocessError, ValueError):
                pass
    except (OSError, subprocess.SubprocessError, ValueError):
        pass
    title = project_title(root)
    result = {'path': str(path), 'name': title, 'title': title, 'repository_id': identity,
              'group': root.parent.name, 'error': False, 'servers': []}
    try:
        raw, git_dir = status(path, deadline=deadline)
        result.update(parse_status(raw))
        result['recent'] = max((p.stat().st_mtime for p in [git_dir / 'index', git_dir / 'HEAD'] if p.exists()), default=0)
    except (OSError, subprocess.SubprocessError, ValueError):
        result.update(parse_status(b''))
        result.update(error=True, recent=0)
    ports = {}
    for socket in sockets:
        if socket['cwd'] == path or path in socket['cwd'].parents:
            port = socket['port']
            ports[port] = {'port': port, 'process': socket['process'], 'web': development_port(port)}
    result['servers'] = sorted(ports.values(), key=lambda s: (not s['web'], s['port']))
    return result


def scan(roots=None, budget=SCAN_BUDGET):
    deadline = time.monotonic() + budget
    # CLI callers may still use the config default. The app passes its immutable
    # request inputs instead of asking this process to re-read mutable settings.
    if roots is None:
        config_home = Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config')
        path = Path(os.environ.get('DESKTOP_CARDS_CONFIG', config_home / 'halcyon/config.json'))
        try:
            config = json.loads(path.read_text())
        except (OSError, ValueError):
            config = {}
        roots = config.get('project_roots', [])
    paths = discover(roots, deadline=min(deadline, time.monotonic() + 3))
    # Empty/unavailable folders have no project servers to match. Avoid starting
    # ss and walking process metadata on every refresh of an empty widget.
    if not paths:
        return {'updated_at': datetime.now(timezone.utc).isoformat(), 'projects': []}
    sockets = listeners(deadline=min(deadline, time.monotonic() + 1))
    with ThreadPoolExecutor(max_workers=4) as pool:
        projects = list(pool.map(lambda path: repository(path, sockets, deadline=deadline), paths))
    projects.sort(key=lambda p: (not bool(p['conflicts']), not any(s['web'] for s in p['servers']),
                                 not bool(p['changed'] + p['untracked']), p['error'], -p['recent'], p['title']))
    return {'updated_at': datetime.now(timezone.utc).isoformat(), 'projects': projects}


def read_roots(stream):
    limit = 1024 * 1024  # same bound as the app's config reader
    payload = stream.read(limit + 1)
    if len(payload) > limit:
        raise ValueError('Project folder input is too large')
    roots = json.loads(payload)
    if not isinstance(roots, list) or len(roots) > 128 or not all(isinstance(p, str) for p in roots):
        raise ValueError('Invalid project folder input')
    return roots


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--roots-stdin', action='store_true')
    parser.add_argument('--budget-seconds', type=float, default=SCAN_BUDGET)
    args = parser.parse_args()
    if not 0 < args.budget_seconds <= SCAN_BUDGET:
        parser.error('Budget must be between 0 and 20 seconds')
    if args.roots_stdin:
        roots = read_roots(sys.stdin.buffer)
        result = scan(roots, budget=args.budget_seconds)
        result['requested_roots'] = roots
    else:
        result = scan(budget=args.budget_seconds)
    print(json.dumps(result))
