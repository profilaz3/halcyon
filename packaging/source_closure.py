#!/usr/bin/env python3
"""Vendor and check the locked Cargo dependency closure of a source tree.

Corresponding source must build with `cargo build --release --locked --offline`
and an empty Cargo home. Every registry and Git package in Cargo.lock needs a
vendored directory, and every Git source needs its own replacement entry in
`.cargo/config.toml`. Regenerate that file with `vendor` for each release:
a configuration copied from an earlier release misses Git sources added since,
such as the patched iced crates.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
from urllib.parse import parse_qsl

CONFIG = Path('.cargo/config.toml')
VENDORED = 'vendored-sources'
PLATFORM = 'x86_64-unknown-linux-gnu'
REFERENCES = ('rev', 'tag', 'branch')


def git_source(source):
    """Split a locked `git+URL?kind=value#commit` source into its replacement key."""
    if not source.startswith('git+'):
        raise ValueError(f'not a Git source: {source}')
    location = source[4:].split('#', 1)[0]
    url, _, query = location.partition('?')
    pairs = parse_qsl(query, keep_blank_values=True)
    if len(pairs) > 1 or any(kind not in REFERENCES for kind, _ in pairs):
        raise ValueError(f'unexpected Git reference: {source}')
    return (url, *pairs[0]) if pairs else (url, None, None)


def locked_packages(tree):
    lock = tomllib.loads((tree/'Cargo.lock').read_text(encoding='utf-8'))
    return [(item['name'], item['version'], item['source'])
            for item in lock.get('package', []) if item.get('source')]


def replacements(tree):
    """Git replacements and the vendored directory named by the tree's config."""
    try:
        config = tomllib.loads((tree/CONFIG).read_text(encoding='utf-8'))
    except FileNotFoundError:
        raise ValueError(f'{CONFIG} is missing; run the vendor command') from None
    sources = config.get('source', {})
    directory = sources.get(VENDORED, {}).get('directory')
    if not isinstance(directory, str) or not directory:
        raise ValueError(f'{CONFIG} names no [source.{VENDORED}] directory')
    replaced = set()
    for name, table in sources.items():
        if name == VENDORED or table.get('replace-with') != VENDORED:
            continue
        if name == 'crates-io':
            replaced.add('crates-io')
        elif 'git' in table:
            kinds = [kind for kind in REFERENCES if kind in table]
            if len(kinds) > 1:
                raise ValueError(f'replacement {name} names more than one Git reference')
            replaced.add((table['git'], *((kinds[0], str(table[kinds[0]])) if kinds else (None, None))))
    # Cargo resolves a relative directory from the folder holding `.cargo`.
    return replaced, (tree/directory).resolve()


def check(tree):
    """Raise ValueError naming every source or package that would need a download."""
    tree = Path(tree)
    replaced, directory = replacements(tree)
    packages = locked_packages(tree)
    errors, git = [], set()
    for name, version, source in packages:
        if source.startswith('registry+'):
            if 'crates-io' not in replaced:
                errors.append(f'crates.io is not replaced (first needed by {name} {version})')
                replaced.add('crates-io')
        elif source.startswith('git+'):
            key = git_source(source)
            git.add(key)
            if key not in replaced:
                errors.append('no replacement for ' + source.split('#', 1)[0])
                replaced.add(key)
        else:
            errors.append(f'unsupported source for {name} {version}: {source}')
            continue
        if not (directory/f'{name}-{version}'/'.cargo-checksum.json').is_file():
            errors.append(f'not vendored: {name} {version}')
    if errors:
        raise ValueError('; '.join(errors))
    return dict(vendored=len(packages), git_sources=len(git))


def vendor(tree, destination):
    """Run `cargo vendor` and write a relative replacement config for the tree."""
    tree, destination = Path(tree).resolve(), Path(destination).resolve()
    config = tree/CONFIG
    if config.exists():
        raise ValueError(f'{config} already exists; vendor into a fresh source tree')
    if destination.exists() and any(destination.iterdir()):
        raise ValueError(f'{destination} is not empty')
    # Never vendor into the tree's own vendor/ folder: cargo vendor clears it.
    output = subprocess.run(
        ['cargo', 'vendor', '--locked', '--versioned-dirs', str(destination)],
        cwd=tree, check=True, capture_output=True, text=True).stdout
    relative = Path(os.path.relpath(destination, tree)).as_posix()
    text, count = re.subn(r'(?m)^directory = ".*"$', 'directory = ' + json.dumps(relative), output)
    if count != 1 or f'[source.{VENDORED}]' not in text:
        raise ValueError('cargo vendor printed an unexpected source configuration')
    config.parent.mkdir(exist_ok=True)
    config.write_text(text.strip() + '\n', encoding='utf-8')
    return check(tree)


def offline(tree, platform=PLATFORM):
    """Resolve the closure with an empty Cargo home and no network."""
    with tempfile.TemporaryDirectory(prefix='halcyon-empty-cargo-home-') as home:
        result = subprocess.run(
            ['cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
             '--filter-platform', platform],
            cwd=tree, env={**os.environ, 'CARGO_HOME': home},
            check=True, capture_output=True, text=True)
    return len(json.loads(result.stdout)['resolve']['nodes'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    make = commands.add_parser('vendor', help='vendor dependencies and write .cargo/config.toml')
    make.add_argument('--tree', type=Path, required=True, help='source tree holding Cargo.lock')
    make.add_argument('--into', type=Path, required=True, help='new dependency directory')
    verify = commands.add_parser('check', help='check an existing source tree')
    verify.add_argument('--tree', type=Path, required=True)
    for command in (make, verify):
        command.add_argument('--offline-check', action='store_true',
                             help='also resolve with cargo metadata, offline, with an empty Cargo home')
    args = parser.parse_args()
    try:
        result = vendor(args.tree, args.into) if args.command == 'vendor' else check(args.tree)
        if args.offline_check:
            result['offline_packages'] = offline(args.tree)
        print(json.dumps(result, sort_keys=True))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        detail = getattr(error, 'stderr', None) or error
        parser.exit(1, f'source-closure: {str(detail).strip()}\n')


if __name__ == '__main__':
    main()
