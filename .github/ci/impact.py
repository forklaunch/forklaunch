#!/usr/bin/env python3
"""Decide which CI suites a change can affect.

Every pull-request workflow calls .github/workflows/changes.yml, which runs
this script on the list of changed paths and gates its jobs on the result.
Pushes to main, the nightly schedules and a dispatch with ``full`` skip the
classification and run everything.

Each changed path is matched against two lists:

* ``INERT``: paths that need no checks at all (documentation, plans, editor
  settings, changesets). The first match wins and nothing else is consulted.
* ``RULES``: additive. A path collects the impacts of every rule it matches.

A path that matches neither list is *unclassified*: it is reported as a
warning and the run falls back to every suite, so a new directory can never
be silently skipped. ``python3 .github/ci/test_impact.py`` asserts that every
tracked file is classified, and E2E runs it whenever this directory changes.

Paths under cli/tests are mapped through the shard layout
(cli/tests/shards.sh and .github/ci/e2e-suites.json): a changed script or
fixture runs only the E2E jobs whose scripts reach it by name.

Impacts:

  cli-tests       CLI Tests (cargo test in cli/)
  e2e-ts          every TypeScript scaffolding job
  e2e-rust        every Rust and mixed scaffolding job, and Verify packaged CLI
  suite:<shard>   the E2E job that runs that cli/tests/shards.sh shard
  packaged-cli    Verify packaged CLI
  framework       Build Framework
  blueprint       Build Blueprint
  framework-rs    Build Framework RS
  blueprint-rs    Build Blueprint RS
  depcheck        Blueprint Dependency Check
  rust-security   Rust Security Audit
  codeql          every CodeQL language (otherwise languages follow file types)
  impact-map      the self-test of this file
  all             everything above
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

CLI_TESTS = 'cli-tests'
E2E_TS = 'e2e-ts'
E2E_RUST = 'e2e-rust'
PACKAGED = 'packaged-cli'
FRAMEWORK = 'framework'
BLUEPRINT = 'blueprint'
FRAMEWORK_RS = 'framework-rs'
BLUEPRINT_RS = 'blueprint-rs'
DEPCHECK = 'depcheck'
RUST_SECURITY = 'rust-security'
CODEQL_ALL = 'codeql'
IMPACT_MAP = 'impact-map'
ALL = 'all'
SUITE = 'suite:'

EVERYTHING = (CLI_TESTS, E2E_TS, E2E_RUST, PACKAGED, FRAMEWORK, BLUEPRINT, FRAMEWORK_RS,
              BLUEPRINT_RS, DEPCHECK, RUST_SECURITY, CODEQL_ALL, IMPACT_MAP)
CODEQL_LANGUAGES = ('actions', 'javascript-typescript', 'python', 'rust')

# What generated projects are made of: both scaffolding families, the
# packaged install and the CLI's own tests.
GENERATION = (E2E_TS, E2E_RUST, PACKAGED, CLI_TESTS)
RUST_GENERATION = (E2E_RUST, PACKAGED, CLI_TESTS)

# CLI modules that only talk to the platform or report on a project: they
# scaffold nothing, and no E2E script runs them.
CLI_PLATFORM_MODULES = (
    'account', 'alerts', 'analyze', 'app', 'cloud_account', 'compliance', 'config', 'context',
    'data', 'deploy', 'dlq', 'drift', 'github', 'integrate', 'logout', 'managed', 'model',
    'notifiers', 'observe', 'org', 'score', 'version', 'whoami', 'worker',
)
# CLI modules that write or edit projects (everything E2E exercises).
CLI_GENERATION_MODULES = (
    'change', 'constants', 'core', 'delete', 'depcheck', 'dev', 'eject', 'infra', 'init', 'main',
    'openapi', 'prompt', 'sdk', 'sync', 'templates',
)
# Blueprint modules that the Rust and mixed suites scaffold as TypeScript
# services next to Rust ones (cli/src/templates/project links into them).
BLUEPRINT_MIXED_MODULES = ('cac-base', 'client-sdk', 'core', 'iam-base', 'iam-better-auth')
# Blueprint root files linked into cli/src/templates/application.
BLUEPRINT_APPLICATION_FILES = (
    '.dockerignore', '.oxlint.config.json', '.prettierignore', '.prettierrc', 'biome.json',
    'eslint.config.mjs', 'jest.config.ts', 'tsconfig.base.json',
)


def _alternatives(names, suffix=''):
    return '{' + ','.join(f'{name}{suffix}' for name in names) + '}'


class Rule:
    def __init__(self, patterns, *impacts, exclude=()):
        self.patterns = [patterns] if isinstance(patterns, str) else list(patterns)
        self.impacts = impacts
        self.exclude = [exclude] if isinstance(exclude, str) else list(exclude)

    def matches(self, path):
        return (any(glob_match(p, path) for p in self.patterns)
                and not any(glob_match(p, path) for p in self.exclude))


# Needs nothing. First match wins.
INERT = [
    Rule(['docs/**', 'plan/**', 'assets/**', '.claude/**', '.vscode/**', '.husky/**',
          '*.md', '*.html', 'LICENSE', '.cursorrules', '.gitignore',
          # Root tooling: husky only.
          'package.json', 'package-lock.json', 'bun.lockb']),
    # Documentation everywhere, except where the CLI embeds it: templates are
    # copied into generated projects and the skill pack ships in the binary.
    Rule(['**/*.md', '**/LICENSE', 'cli/docs/**'],
         exclude=['cli/src/templates/**', 'cli/assets/**']),
    # Changesets only feed the release workflows.
    Rule(['framework/.changeset/**', 'blueprint/.changeset/**']),
    # Workflows that are not pull-request checks, or filter themselves.
    Rule(['.github/dependabot.yml',
          '.github/workflows/{cli_release,rust_release,ts_release,rust_parity,blueprint_depcheck_sync,mlse_tests}.yml',
          '.github/workflows/scripts/ts_release/**', '.github/workflows/scripts/parity_shards.py']),
    # Release and maintenance scripts, run by the release workflows above or
    # by hand.
    Rule(['scripts/{rust_registry.py,rust_registry_smoke.rs,rust_feature_matrix.py,test_rust_release.py}',
          'scripts/{check-cli-routes.mjs,check_rust_symbol_map.mjs,rust_api_inventory.mjs,test_rust_api_inventory.mjs}',
          'scripts/{rust_zod_emoji_table.py,sync-skills.bash,publish-package.bash}',
          'scripts/eject-*.bash']),
]

RULES = [
    # --- CI itself -------------------------------------------------------
    Rule(['.github/ci/**', '.github/workflows/changes.yml'], ALL),
    # cli/src/core/github_configs.rs reads this file in a unit test.
    Rule('.github/workflows/e2e_tests.yml', *GENERATION),
    Rule('.github/actions/rust-build-cache/**', E2E_RUST, PACKAGED, CLI_TESTS, DEPCHECK, RUST_SECURITY),
    Rule('.github/workflows/build_framework.yml', FRAMEWORK),
    Rule('.github/workflows/build_blueprint.yml', BLUEPRINT),
    Rule('.github/workflows/build_framework_rs.yml', FRAMEWORK_RS),
    Rule('.github/workflows/build_blueprint_rs.yml', BLUEPRINT_RS),
    Rule('.github/workflows/cli_tests.yml', CLI_TESTS),
    Rule('.github/workflows/rust_security.yml', RUST_SECURITY),
    Rule(['.github/workflows/blueprint_depcheck.yml', '.github/workflows/scripts/check_blueprint_deps/**'], DEPCHECK),
    Rule(['.github/workflows/codeql.yml', '.github/codeql/**'], CODEQL_ALL),

    # --- CLI ---------------------------------------------------------------
    # Generation: init/change/delete/sync/eject/infra/sdk/dev/openapi, the
    # templates, and the core generators shared by every command.
    Rule([f'cli/src/{_alternatives(CLI_GENERATION_MODULES)}.rs',
          f'cli/src/{_alternatives(CLI_GENERATION_MODULES)}/**'], *GENERATION,
         exclude=['cli/src/templates/rust/**', 'cli/src/templates/sdk/rust/**',
                  'cli/src/init/rust_project.rs', 'cli/src/init/rust_project/**',
                  'cli/src/core/cargo_toml.rs', 'cli/src/core/cargo_toml/**',
                  # Platform-only parts of core (accounts, report card,
                  # static analysis for `score`/`analyze`).
                  'cli/src/core/{accounts.rs,accounts/**,report_card.rs,static_analysis/**}']),
    # Rust-only generation.
    Rule(['cli/src/templates/rust/**', 'cli/src/templates/sdk/rust/**',
          'cli/src/init/rust_project.rs', 'cli/src/init/rust_project/**',
          'cli/src/core/cargo_toml.rs', 'cli/src/core/cargo_toml/**'], *RUST_GENERATION),
    Rule('cli/src/core/{accounts.rs,accounts/**,report_card.rs,static_analysis/**}', CLI_TESTS),
    # Version constants are also checked against the blueprint workspaces.
    Rule(['cli/src/core/package_json/package_json_constants.rs',
          'cli/src/core/cargo_toml/cargo_toml_constants.rs', 'cli/src/depcheck.rs'], DEPCHECK),
    # Build Blueprint runs the scaffold .gitignore and IAM regression checks.
    Rule(['cli/src/core/gitignore.rs', 'cli/src/templates/application/.gitignore',
          'cli/src/templates/project/iam-better-auth/**'], BLUEPRINT),
    # Commands with one E2E script each, in the "TS environment and release"
    # job (environment validate/sync, release create) and "TS init, delete
    # and sync" (login in init_application.sh).
    Rule(['cli/src/environment.rs', 'cli/src/environment/**', 'cli/src/release/**'],
         CLI_TESTS, SUITE + 'ts-env-release'),
    Rule('cli/src/login.rs', CLI_TESTS, SUITE + 'ts-init'),
    # Platform and reporting commands: unit tests only.
    Rule([f'cli/src/{_alternatives(CLI_PLATFORM_MODULES)}.rs',
          f'cli/src/{_alternatives(CLI_PLATFORM_MODULES)}/**'], CLI_TESTS),
    # The skill pack `forklaunch context` embeds, and its consistency checks.
    Rule(['cli/assets/**', 'cli/scripts/check-skill-*.sh'], CLI_TESTS),
    Rule(['cli/Cargo.toml', 'cli/build.rs', 'cli/.rustfmt.toml'], *GENERATION),
    Rule('cli/Cargo.toml', RUST_SECURITY),
    # npm packaging of the CLI; the version prechecks rewrite package.json.
    Rule(['cli/package.json', 'cli/bun.lock', 'cli/pnpm-lock.yaml', 'cli/scripts/install.js',
          'cli/.npmignore', 'cli/.gitignore'], CLI_TESTS, PACKAGED, SUITE + 'ts-env-release'),
    # cli/tests/** is mapped by test_impacts().

    # --- TypeScript framework -----------------------------------------------
    Rule('framework/**', FRAMEWORK, E2E_TS),
    Rule('framework/**/package.json', FRAMEWORK_RS),  # check_rust_package_owners.py
    # Linked into the generated SDKs of both languages.
    Rule('framework/common/src/safeStringify.ts', *GENERATION),
    Rule(['framework/core/src/persistence/**', 'framework/universal-sdk/**'], E2E_RUST),

    # --- TypeScript blueprint -----------------------------------------------
    Rule('blueprint/**', BLUEPRINT, E2E_TS),
    Rule('blueprint/**/package.json', DEPCHECK),
    Rule([f'blueprint/{_alternatives(BLUEPRINT_MIXED_MODULES)}/**',
          f'blueprint/{_alternatives(BLUEPRINT_APPLICATION_FILES)}'], E2E_RUST, CLI_TESTS),
    # Linked into cli/src/templates/project, or read by CLI unit tests.
    Rule(['blueprint/{billing-base,billing-stripe,ecommerce-stripe,messaging-base,messaging-twilio,mlse-base,monitoring}/**',
          'blueprint/pnpm-workspace.yaml', 'blueprint/*/.gitignore'], CLI_TESTS),

    # --- Rust framework and blueprint ----------------------------------------
    Rule('framework-rs/**', FRAMEWORK_RS, BLUEPRINT_RS, *RUST_GENERATION),
    Rule('blueprint-rs/**', BLUEPRINT_RS, *RUST_GENERATION),
    # Linked into the generated TypeScript SDK.
    Rule('framework-rs/crates/core/src/{typescript,rust_sdk}/**', E2E_TS),
    Rule(['{framework-rs,blueprint-rs}/Cargo.lock', '{framework-rs,blueprint-rs}/**/Cargo.toml'],
         RUST_SECURITY, DEPCHECK),

    # --- Repository scripts ---------------------------------------------------
    Rule('scripts/rust_format.py', FRAMEWORK_RS, BLUEPRINT_RS),
    Rule('scripts/check_rust_package_owners.py', FRAMEWORK_RS),
    Rule('scripts/{sync_rust_embedded_sources.py,test_sync_rust_embedded_sources.py}', CLI_TESTS),
    Rule(['scripts/rust_security_audit*.py', 'scripts/*rust_ecs_smoke_plan.py'], RUST_SECURITY),
    # cli/tests/rust_registry_scaffold.sh (full tier only).
    Rule('scripts/rust_release.py', SUITE + 'rust-images'),
]

# cli/tests files that something besides the E2E scripts reads.
TEST_FILE_EXTRAS = {
    'rust_socket_contract.json': (CLI_TESTS,),   # include_str! in a unit test
    'rust_infra_app': (CLI_TESTS,),
    'rust-notes-assembly': (CLI_TESTS,),
    'iam_authorization_scope.cjs': (BLUEPRINT,),  # Build Blueprint
    'scaffold_gitignore.py': (BLUEPRINT,),
    'rust_security_advisories.json': (RUST_SECURITY,),
}


@lru_cache(maxsize=None)
def _glob_regex(pattern):
    out, i = [], 0
    while i < len(pattern):
        if pattern.startswith('**/', i):
            out.append('(?:.*/)?')
            i += 3
        elif pattern.startswith('**', i):
            out.append('.*')
            i += 2
        elif pattern[i] == '*':
            out.append('[^/]*')
            i += 1
        elif pattern[i] == '?':
            out.append('[^/]')
            i += 1
        elif pattern[i] == '{':
            end = pattern.index('}', i)
            out.append('(?:' + '|'.join(_glob_regex(alt).pattern[1:-1]
                                         for alt in pattern[i + 1:end].split(',')) + ')')
            i = end + 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return re.compile('^' + ''.join(out) + '$')


def glob_match(pattern, path):
    return _glob_regex(pattern).match(path) is not None


# --- cli/tests ---------------------------------------------------------------

def load_shards(root=ROOT):
    """{shard: [script, ...]} from cli/tests/shards.sh."""
    text = (root / 'cli/tests/shards.sh').read_text()
    body = text[text.index('shard_scripts()'):text.index('SHARDS=')]
    return {m.group(1): m.group(2).split()
            for m in re.finditer(r'^\s*([a-z0-9-]+)\) echo ([^;]+) ;;', body, re.M)}


@lru_cache(maxsize=None)
def _test_files(root):
    tests = root / 'cli/tests'
    files = {}
    for path in tests.rglob('*'):
        if path.is_file() and 'output' not in path.relative_to(tests).parts:
            try:
                files[path.relative_to(tests).as_posix()] = path.read_text(errors='replace')
            except OSError:
                continue
    return files


def _test_name(relative):
    """The name other test files use for a cli/tests path: a top-level file's
    stem, or the first path component under fixtures/."""
    parts = relative.split('/')
    name = parts[1] if parts[0] == 'fixtures' and len(parts) > 1 else parts[0]
    # Python modules are also imported by stem; everything else is named in
    # full (`bash rust_command_sweep.sh`, `fixtures/rust_infra_app`).
    return name, name[:-3] if name.endswith('.py') else name


def test_impacts(relative, root=ROOT):
    """Impacts of a path under cli/tests (relative to that directory)."""
    if relative == 'shards.sh':
        return {E2E_TS, E2E_RUST, PACKAGED}
    if relative == 'rust_packaged_install.sh':
        return {PACKAGED}
    files = _test_files(root)
    shards = load_shards(root)
    script_shard = {script: shard for shard, scripts in shards.items() for script in scripts}
    name, _ = _test_name(relative)
    impacts = set(TEST_FILE_EXTRAS.get(name, ()))

    # Everything that reaches the changed file by name, transitively.
    reached, frontier = set(), [_test_name(relative)]
    while frontier:
        name_, stem_ = frontier.pop()
        word = re.compile(r'(?<![\w-])' + re.escape(stem_) + r'(?![\w-])')
        for other, text in files.items():
            # The shard list and README name every script; they are not uses.
            if other in reached or other == relative or other in ('shards.sh', 'README.md'):
                continue
            if word.search(text):
                reached.add(other)
                frontier.append(_test_name(other))
    reached.add(relative)

    for other in reached:
        base = other.split('/')[-1]
        if base in script_shard:
            impacts.add(SUITE + script_shard[base])
        if base == 'rust_packaged_install.sh':
            impacts.add(PACKAGED)
    # A fixture is named after the script that runs it (fixtures/X.py <- X.sh).
    for other in list(reached):
        if other.startswith('fixtures/'):
            script = _test_name(other)[1] + '.sh'
            if script in script_shard:
                impacts.add(SUITE + script_shard[script])
    if not any(i.startswith(SUITE) or i == PACKAGED for i in impacts) and not TEST_FILE_EXTRAS.get(name):
        # A helper no script names: assume every suite needs it.
        impacts |= {E2E_TS, E2E_RUST, PACKAGED}
    return impacts


# --- classification ----------------------------------------------------------

def classify(path, root=ROOT):
    """(impacts, classified) for one repository path."""
    for rule in INERT:
        if rule.matches(path):
            return set(), True
    impacts, classified = set(), False
    if path.startswith('cli/tests/'):
        impacts |= test_impacts(path[len('cli/tests/'):], root)
        classified = True
    for rule in RULES:
        if rule.matches(path):
            impacts.update(rule.impacts)
            classified = True
    return impacts, classified


def codeql_languages(path):
    languages = set()
    if re.match(r'^\.github/(workflows/[^/]+|actions/.*/action)\.ya?ml$', path) or \
            re.match(r'^\.github/actions/action\.ya?ml$', path):
        languages.add('actions')
    suffix = path.rsplit('.', 1)[-1] if '.' in path.split('/')[-1] else ''
    if suffix in ('js', 'jsx', 'mjs', 'cjs', 'ts', 'tsx', 'mts', 'cts', 'vue', 'html', 'htm'):
        languages.add('javascript-typescript')
    if suffix == 'py':
        languages.add('python')
    if suffix == 'rs' or path.split('/')[-1] in ('Cargo.toml', 'Cargo.lock'):
        languages.add('rust')
    return languages


def expand(impacts):
    return set(EVERYTHING) if ALL in impacts else set(impacts)


def e2e_selection(impacts, layout, full):
    """The TypeScript and Rust job matrices for these impacts."""
    shards_wanted = {i[len(SUITE):] for i in impacts if i.startswith(SUITE)}
    selected = {}
    for kind, family in (('typescript', E2E_TS), ('rust', E2E_RUST)):
        chosen = []
        for entry in layout[kind]:
            if not full and entry['tier'] != 'fast':
                continue
            if full or family in impacts or shards_wanted & set(entry['shards'].split()):
                job = {k: v for k, v in entry.items() if k != 'tier'}
                # Each Rust job keeps its own compiler store.
                job.setdefault('family', 'e2e-' + re.sub(r'[^a-z0-9]+', '-', job['name'].lower()).strip('-'))
                chosen.append(job)
        selected[kind] = chosen
    return selected


def evaluate(paths, full, root=ROOT):
    rows, impacts, languages, unclassified = [], set(), set(), []
    if full:
        impacts, languages = set(EVERYTHING), set(CODEQL_LANGUAGES)
    else:
        for path in paths:
            found, classified = classify(path, root)
            langs = codeql_languages(path)
            if not classified:
                unclassified.append(path)
                found = {ALL}
                langs = set(CODEQL_LANGUAGES)
            rows.append((path, sorted(found), sorted(langs)))
            impacts |= found
            languages |= langs
        impacts = expand(impacts)
        if CODEQL_ALL in impacts:
            languages = set(CODEQL_LANGUAGES)
    layout = json.loads((root / '.github/ci/e2e-suites.json').read_text())
    matrices = e2e_selection(impacts, layout, full)
    packaged = full or PACKAGED in impacts or E2E_RUST in impacts
    return {
        'impacts': sorted(impacts),
        'codeql': [lang for lang in CODEQL_LANGUAGES if lang in languages],
        'typescript-matrix': matrices['typescript'],
        'rust-matrix': matrices['rust'],
        'typescript': bool(matrices['typescript']),
        'rust': bool(matrices['rust']),
        'packaged-cli': packaged,
        'build-cli': bool(matrices['typescript'] or matrices['rust']),
        'unclassified': unclassified,
        'rows': rows,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--full', action='store_true', help='run everything')
    parser.add_argument('--list', help='file with one changed path per line')
    parser.add_argument('paths', nargs='*')
    args = parser.parse_args(argv)
    paths = list(args.paths)
    if args.list:
        paths += [line for line in Path(args.list).read_text().splitlines() if line.strip()]
    paths = sorted(set(paths))
    result = evaluate(paths, args.full)

    for path in result['unclassified']:
        print(f'::warning file={path},title=Path matches no CI rule::'
              f'{path} is not in .github/ci/impact.py, so every suite runs. Add a rule for it.')
    summary = ['## CI impact', '',
               '**Full run**: every suite.' if args.full else
               f"{len(paths)} changed paths. Impacts: `{', '.join(result['impacts']) or 'none'}`. "
               f"CodeQL: `{', '.join(result['codeql']) or 'none'}`.", '']
    if not args.full and result['rows']:
        summary += ['| Path | Impacts | CodeQL |', '| --- | --- | --- |']
        summary += [f"| `{p}` | {', '.join(i) or '—'} | {', '.join(l) or '—'} |"
                    for p, i, l in result['rows'][:200]]
        if len(result['rows']) > 200:
            summary.append(f'| … {len(result["rows"]) - 200} more | | |')
    print('\n'.join(summary))
    print('TypeScript jobs:', [j['name'] for j in result['typescript-matrix']])
    print('Rust jobs:', [j['name'] for j in result['rust-matrix']])

    def emit(stream_name, lines):
        target = os.environ.get(stream_name)
        if target:
            with open(target, 'a') as out:
                out.write(lines)

    emit('GITHUB_STEP_SUMMARY', '\n'.join(summary) + '\n')
    emit('GITHUB_OUTPUT', ''.join(
        f'{key}={json.dumps(value) if isinstance(value, list) else str(value).lower()}\n'
        for key, value in result.items() if key not in ('rows', 'unclassified')))
    return 0


if __name__ == '__main__':
    sys.exit(main())
