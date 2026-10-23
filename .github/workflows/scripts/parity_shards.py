#!/usr/bin/env python3
"""Shard the framework and blueprint jobs of rust_parity.yml.

Each job runs once per shard (a `shard` matrix). Setup steps (actions,
installs, service containers, Node pins) run in every shard, in their
original order, so each test step still sees the environment it saw when the
job ran as one; every test step gets `if: matrix.shard == '<shard>'`.

The layout is data (SHARDS below), not hand-written conditions:

  - `start`: a test step matching it, and the test steps after it, belong to
    that shard, until the next `start` matches;
  - `also`: test steps matching it belong to that shard wherever they are.

Test steps added later without running this script have no shard condition
and run in every shard: duplicated, never lost. Run it after adding steps:

    python3 .github/workflows/scripts/parity_shards.py          # rewrite
    python3 .github/workflows/scripts/parity_shards.py --check  # verify

--check exits 1 when a shard would run no test step or a job's matrix
lists different shards than SHARDS (both fail CI), and 2 when the
conditions are merely out of date (CI warns).

Shards listed in FULL_ONLY run only on full runs (push to main, nightly,
dispatch with full); the matrix in the workflow says which.
"""
import re
import sys
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parents[1] / 'rust_parity.yml'
MARK = '# shard'

SHARDS = {
    'framework': [
        # format, clippy, docs, workspace and vendored-crate tests, backends, benchmarks
        {'shard': 'core', 'start': r''},
        {'shard': 'http', 'start': r'rust_redis_queue_parity\.mjs'},
        {'shard': 'zod', 'start': r'rust_zod_string_parity\.mjs'},
        {'shard': 'zod', 'start': r'Exact numeric bounds in default mode'},
        {'shard': 'http', 'start': r'rust_versioned_wire_parity\.mjs'},
    ],
    'blueprint': [
        {'shard': 'billing', 'start': r''},
        {'shard': 'modules', 'start': r'Pin Node for configured HMAC date comparisons|rust_billing_hmac_dates_http\.mjs'},
        {'shard': 'core', 'start': r'rust_format\.py'},
        {'shard': 'modules', 'start': r'-p forklaunch-billing-base .*--test services'},
        {'shard': 'core', 'also': r'-p forklaunch-worker-(redis|kafka) '},
        # Sleep- and timeout-bound tests that flake on loaded runners.
        {'shard': 'timing', 'also': r'--test bound_creation |-p forklaunch-worker-database .*postgres_worker'},
        {'shard': 'billing', 'also': r'Install isolated Chromium relay fixture'},
    ],
}
FULL_ONLY = {'timing'}

SETUP = re.compile(
    r'^(npm |mkdir |sudo |docker |echo |cp |# shellcheck)'
)
SETUP_NAMES = re.compile(
    r'^(Start isolated|Stop isolated|Remove isolated|Install native|Install legacy|Pin Node|Restore Node|Restore configured-module|Compiler cache statistics)'
)


def jobs(lines):
    """(job, first, end) line ranges of the sharded jobs."""
    starts = [i for i, line in enumerate(lines) if re.match(r'^  [A-Za-z0-9_-]+:\s*$', line)]
    starts.append(len(lines))
    for a, b in zip(starts, starts[1:]):
        name = lines[a].strip()[:-1]
        if name in SHARDS:
            yield name, a, b


def steps(lines, first, end):
    """(start, end) line ranges of each step in a job."""
    top = next(i for i in range(first, end) if lines[i] == '    steps:')
    marks = [i for i in range(top + 1, end) if lines[i].startswith('      - ')]
    for a, b in zip(marks, marks[1:] + [end]):
        while b > a + 1 and not lines[b - 1].strip():
            b -= 1
        yield a, b


def describe(block):
    text = '\n'.join(block)
    name = re.search(r'^      - name: (.*)$|^        name: (.*)$', text, re.M)
    name = (name.group(1) or name.group(2)).strip() if name else ''
    run = re.search(r'run: (\|\n)?(.*)', text)
    run = run.group(2).strip() if run else ''
    uses = re.search(r'uses: (\S+)', text)
    return name, run, uses.group(1) if uses else '', text


def shard_of(job, block, current):
    name, run, uses, text = describe(block)
    if name in ('Save compiler cache',):
        return 'save', current
    for rule in SHARDS[job]:
        if 'also' in rule and re.search(rule['also'], text):
            return rule['shard'], current
    if (uses and 'upload-artifact' not in uses) or SETUP.match(run) or SETUP_NAMES.match(name):
        return None, current
    for rule in SHARDS[job]:
        if rule.get('start') and re.search(rule['start'], text):
            current = rule['shard']
    return current, current


def rewrite(source):
    lines = source.split('\n')
    out = list(lines)
    edits = []
    for job, first, end in jobs(lines):
        current = SHARDS[job][0]['shard']
        for a, b in steps(lines, first, end):
            block = [line for line in lines[a:b] if not line.rstrip().endswith(MARK)]
            shard, current = shard_of(job, block, current)
            if shard == 'save':
                block = [line for line in block if not line.startswith('        if:')]
                block.append(f"        if: always() {MARK}")
            elif shard:
                if any(line.startswith('        if:') for line in block):
                    raise SystemExit(f'{job}: test step at line {a + 1} already has an if:; combine it by hand')
                block.append(f"        if: matrix.shard == '{shard}' {MARK}")
            edits.append((a, b, block))
    for a, b, block in reversed(edits):
        out[a:b] = block
    return '\n'.join(out)


def problems(source):
    """Shards that would run no test step, and matrices that disagree with SHARDS."""
    lines = source.split('\n')
    found = []
    for job, first, end in jobs(lines):
        declared = {rule['shard'] for rule in SHARDS[job]}
        counts = dict.fromkeys(declared, 0)
        for a, b in steps(lines, first, end):
            match = re.search(r"matrix\.shard == '([a-z-]+)' " + MARK + r'$', '\n'.join(lines[a:b]), re.M)
            if match and 'Save compiler cache' not in '\n'.join(lines[a:b]):
                counts[match.group(1)] = counts.get(match.group(1), 0) + 1
        found += [f'{job}: shard {name!r} has no test steps' for name, count in sorted(counts.items()) if not count]
        block = '\n'.join(lines[first:end])
        listed = set(re.findall(r'"([a-z-]+)"|\b([a-z-]+)\b(?=[,\]])', block.split('shard:', 1)[1].split('\n', 1)[0])) if 'shard:' in block else set()
        listed = {a or b for a, b in listed}
        if listed and listed != declared:
            found.append(f'{job}: matrix shards {sorted(listed)} differ from SHARDS {sorted(declared)}')
    return found


def main():
    source = WORKFLOW.read_text()
    result = rewrite(source)
    errors = problems(result)
    for error in errors:
        print(f'error: {error}')
    if '--check' in sys.argv:
        if result != source:
            print(f'{WORKFLOW.name} is not sharded as {Path(__file__).name} says; run it without --check')
            return 2 if not errors else 1
        return 1 if errors else 0
    if errors:
        return 1
    WORKFLOW.write_text(result)
    return 0


if __name__ == '__main__':
    sys.exit(main())
