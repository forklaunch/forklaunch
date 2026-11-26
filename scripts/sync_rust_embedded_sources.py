#!/usr/bin/env python3
"""Mirror the blueprint ignore files that CLI project templates embed.

Generated Rust modules bind handlers from published blueprint crates, so the
CLI embeds no blueprint Rust sources. Git refuses symlink ignore files, so the
embedded copies are kept identical to their blueprint owners instead.

framework-rs crates also embed a few TypeScript framework sources through
`include_str!`. Those are regular files owned by the TypeScript framework and
mirrored here, never symlinks: a container build, vendored copy or registry
package that holds only framework-rs cannot follow a link out of it.

Run with `--check` to fail (exit 1) instead of writing when a copy is stale.
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def mirror_git_metadata(source: Path, target: Path):
    """Git refuses symlink ignore/attribute files; these copies are owned catalogs."""
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_symlink():
        if target.resolve() != source.resolve():
            raise ValueError(f'Preserving unexpected Git metadata link: {target}')
        target.unlink()
    content = source.read_bytes()
    if not target.exists() or target.read_bytes() != content:
        target.write_bytes(content)


# (TypeScript framework owner, framework-rs embedded copy)
FRAMEWORK_SOURCES = [
    ('framework/common/src/safeStringify.ts', 'framework-rs/crates/core/src/typescript/safeStringify.ts'),
]


def mirror_framework_source(source: Path, target: Path, check: bool = False) -> bool:
    """Replace a link with a regular copy and keep it identical. Returns True when current."""
    content = source.read_bytes()
    current = not target.is_symlink() and target.is_file() and target.read_bytes() == content
    if current or check:
        return current
    if target.is_symlink():
        target.unlink()
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(content)
    return True


def sync_framework_sources(check: bool = False) -> list:
    stale = []
    for source, target in FRAMEWORK_SOURCES:
        if not mirror_framework_source(ROOT / source, ROOT / target, check):
            stale.append(target)
    return stale


def sync():
    # Cargo's Git status walker rejects symlink ignore files even when the
    # template link resolves. Mirror every catalog's owned ignore file.
    for source in (ROOT / 'blueprint').glob('*/.gitignore'):
        target = ROOT / 'cli/src/templates/project' / source.parent.name / '.gitignore'
        if target.exists() or target.is_symlink():
            owner = {'billing-stripe': 'billing-base', 'messaging-twilio': 'messaging-base'}.get(source.parent.name, source.parent.name)
            source = ROOT / 'blueprint' / owner / '.gitignore'
            mirror_git_metadata(source, target)


if __name__ == '__main__':
    if '--check' in sys.argv[1:]:
        stale = sync_framework_sources(check=True)
        if stale:
            sys.exit('Stale embedded framework sources (run scripts/sync_rust_embedded_sources.py): ' + ', '.join(stale))
    else:
        sync()
        sync_framework_sources()
