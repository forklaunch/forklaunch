#!/usr/bin/env python3
"""Reject Rust framework packages without an existing TypeScript project owner.

This checks ownership, not behavioral parity. Compiler helpers belong to core;
only the separate proc-macro compiler target is an internal package exception.
"""
import json
from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
OWNERS = {
    'common': ('forklaunch-common', 'framework/common'),
    'core': ('forklaunch-core', 'framework/core'),
    'core-macros': ('forklaunch-core-macros', 'framework/core'),
    'validator': ('forklaunch-validator', 'framework/validator'),
    'axum': ('forklaunch-axum', 'framework/express'),
    'ws': ('forklaunch-ws', 'framework/ws'),
    'universal-sdk': ('forklaunch-universal-sdk', 'framework/universal-sdk'),
    'redis': ('forklaunch-infrastructure-redis', 'framework/infrastructure/redis'),
    's3': ('forklaunch-infrastructure-s3', 'framework/infrastructure/S3'),
    'testing': ('forklaunch-testing', 'framework/testing'),
}

def check(root: Path):
    workspace = root / 'framework-rs'
    manifest = tomllib.loads((workspace / 'Cargo.toml').read_text())
    errors, packages = [], []
    declared = set()
    for member in manifest['workspace']['members']:
        # Keep this policy explicit: wildcard membership can silently adopt a crate.
        if any(char in member for char in '*?['):
            errors.append(f'Use explicit workspace membership: {member}')
            continue
        path = workspace / member / 'Cargo.toml'
        if path.resolve() in declared:
            errors.append(f'Duplicate workspace member: {member}')
        declared.add(path.resolve())
        if member != f'crates/{path.parent.name}':
            errors.append(f'Unexpected framework package location: {member}')
        if not path.is_file():
            errors.append(f'Missing member manifest: {member}')
            continue
        package = tomllib.loads(path.read_text())['package']['name']
        folder = path.parent.name
        expected = OWNERS.get(folder)
        if expected is None:
            errors.append(f'Unowned framework crate: {member} ({package})')
            continue
        name, owner = expected
        if package != name:
            errors.append(f'{member}: expected package {name}, found {package}')
        if not (root / owner / 'package.json').is_file():
            errors.append(f'{member}: missing TypeScript project {owner}')
        packages.append({'package': package, 'owner': owner, 'status': 'mapped'})
    # Also reject abandoned product manifests outside workspace membership.
    # Third-party patches under crates/vendor are deliberately not products.
    for path in (workspace / 'crates').glob('*/Cargo.toml'):
        if path.resolve() not in declared:
            errors.append(f'Orphan framework crate: {path.relative_to(root)}')
    return {'scope': 'Package ownership only; does not establish feature parity',
            'exceptions': ['Axum is the native HTTP adapter equivalent of Express',
                           'core-macros is the separate compiler target Rust requires'],
            'packages': packages, 'errors': errors}


def main():
    report = check(ROOT)
    print(json.dumps(report, indent=2))
    return bool(report['errors'])


if __name__ == '__main__':
    sys.exit(main())
