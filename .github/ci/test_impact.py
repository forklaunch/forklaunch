#!/usr/bin/env python3
"""Tests for .github/ci/impact.py. Run: python3 .github/ci/test_impact.py"""
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import impact  # noqa: E402

ROOT = impact.ROOT


def run(*paths):
    return impact.evaluate(list(paths), full=False)


def jobs(result):
    return sorted(j['name'] for j in result['typescript-matrix'] + result['rust-matrix'])


TS_JOBS = sorted(e['name'] for e in json.loads((ROOT / '.github/ci/e2e-suites.json').read_text())['typescript'])
RUST_FAST = sorted(e['name'] for e in json.loads((ROOT / '.github/ci/e2e-suites.json').read_text())['rust']
                   if e['tier'] == 'fast')


class Scenarios(unittest.TestCase):
    def test_platform_command_runs_unit_tests_only(self):
        result = run('cli/src/score.rs')
        self.assertEqual(result['impacts'], ['cli-tests'])
        self.assertEqual(jobs(result), [])
        self.assertFalse(result['build-cli'])
        self.assertEqual(result['codeql'], ['rust'])

    def test_platform_modules(self):
        for path in ('cli/src/observe/logs.rs', 'cli/src/deploy/create.rs', 'cli/src/account.rs',
                     'cli/src/managed/mod.rs', 'cli/src/core/report_card.rs', 'cli/assets/skills.toml'):
            self.assertEqual(run(path)['impacts'], ['cli-tests'], path)

    def test_rust_template_runs_rust_jobs(self):
        result = run('cli/src/templates/rust/apalis-worker/bootstrapper.rs')
        self.assertEqual(result['impacts'], ['cli-tests', 'e2e-rust', 'packaged-cli'])
        self.assertEqual(jobs(result), RUST_FAST)
        self.assertTrue(result['build-cli'] and result['packaged-cli'])
        self.assertFalse(run('cli/tests/rust_packaged_install.sh')['build-cli'])
        self.assertEqual(result['codeql'], ['rust'])

    def test_shared_generation_runs_both(self):
        result = run('cli/src/init/service.rs')
        self.assertEqual(jobs(result), sorted(TS_JOBS + RUST_FAST))
        self.assertIn('cli-tests', result['impacts'])

    def test_framework_runs_ts_jobs(self):
        result = run('framework/core/src/http/router.ts')
        self.assertEqual(result['impacts'], ['e2e-ts', 'framework'])
        self.assertEqual(jobs(result), TS_JOBS)
        self.assertEqual(result['codeql'], ['javascript-typescript'])

    def test_docs_run_nothing(self):
        result = run('docs/cli/init.md', 'README.md', 'plan/SYNC.md', 'cli/README.md',
                     'framework-rs/crates/core/README.md', 'blueprint/.changeset/x.md')
        self.assertEqual(result['impacts'], [])
        self.assertEqual(result['codeql'], [])
        self.assertFalse(result['build-cli'])
        self.assertEqual(result['unclassified'], [])

    def test_embedded_markdown_is_not_docs(self):
        self.assertIn('cli-tests', run('cli/assets/forklaunch-skills/README.md')['impacts'])
        self.assertIn('e2e-ts', run('cli/src/templates/basic/README.md')['impacts'])

    def test_unknown_runs_everything(self):
        result = run('newdir/thing.txt')
        self.assertEqual(result['unclassified'], ['newdir/thing.txt'])
        self.assertEqual(set(result['impacts']), set(impact.EVERYTHING))
        self.assertEqual(result['codeql'], list(impact.CODEQL_LANGUAGES))
        self.assertEqual(run('cli/src/brand_new_command.rs')['unclassified'], ['cli/src/brand_new_command.rs'])

    def test_workflow_runs_itself(self):
        self.assertEqual(run('.github/workflows/build_blueprint.yml')['impacts'], ['blueprint'])
        self.assertEqual(run('.github/workflows/build_blueprint.yml')['codeql'], ['actions'])
        self.assertEqual(set(run('.github/workflows/codeql.yml')['codeql']), set(impact.CODEQL_LANGUAGES))
        self.assertEqual(set(run('.github/ci/impact.py')['impacts']), set(impact.EVERYTHING))

    def test_single_suite_command(self):
        result = run('cli/src/environment/validate.rs')
        self.assertEqual(jobs(result), ['TS environment and release'])

    def test_test_script_maps_to_its_suite(self):
        self.assertEqual(jobs(run('cli/tests/eject.sh')), ['TS build and change'])
        self.assertEqual(jobs(run('cli/tests/fixtures/rust_mixed_sync.py')), ['Mixed workspaces'])
        sweep = jobs(run('cli/tests/fixtures/rust_command_sweep.py'))
        self.assertEqual(sweep, ['Rust change sweep', 'Rust delete sweep'])
        self.assertEqual(jobs(run('cli/tests/shards.sh')), sorted(TS_JOBS + RUST_FAST))

    def test_full_runs_everything(self):
        result = impact.evaluate([], full=True)
        self.assertEqual(set(result['impacts']), set(impact.EVERYTHING))
        layout = json.loads((ROOT / '.github/ci/e2e-suites.json').read_text())
        self.assertEqual(len(jobs(result)), len(layout['typescript']) + len(layout['rust']))


class Coverage(unittest.TestCase):
    """Needs a full checkout (E2E runs it when .github/ci changes)."""

    @classmethod
    def setUpClass(cls):
        try:
            out = subprocess.run(['git', 'ls-files'], cwd=ROOT, capture_output=True, text=True, check=True)
        except (OSError, subprocess.CalledProcessError):
            raise unittest.SkipTest('not a git checkout')
        cls.tracked = [p for p in out.stdout.splitlines() if p]
        if not (ROOT / 'cli/src/main.rs').exists():
            raise unittest.SkipTest('sparse checkout')

    def test_every_tracked_file_is_classified(self):
        missing = [p for p in self.tracked if not impact.classify(p)[1]]
        self.assertEqual(missing, [], 'add rules to .github/ci/impact.py for these paths')

    def test_template_links_reach_the_scaffolding_jobs(self):
        # Files outside cli/ that cli/src/templates links to are part of the
        # generated projects, so changing them must run E2E and the CLI tests.
        templates = ROOT / 'cli/src/templates'
        for dirpath, dirnames, filenames in os.walk(templates):
            for name in dirnames + filenames:
                link = Path(dirpath) / name
                if not link.is_symlink():
                    continue
                target = os.path.relpath(os.path.normpath(link.parent / os.readlink(link)), ROOT)
                # Documentation copied into generated projects needs no run.
                if target.startswith('cli/') or target.endswith('.md'):
                    continue
                probe = target + '/x.ts' if (ROOT / target).is_dir() else target
                found, _ = impact.classify(probe)
                found = impact.expand(found)
                rust_only = link.relative_to(templates).parts[0] == 'rust'
                self.assertIn('cli-tests', found, target)
                self.assertIn('e2e-rust' if rust_only else 'e2e-ts', found, target)

    def test_every_shard_has_a_job(self):
        layout = json.loads((ROOT / '.github/ci/e2e-suites.json').read_text())
        listed = {s for kind in ('typescript', 'rust') for e in layout[kind] for s in e['shards'].split()}
        self.assertEqual(listed, set(impact.load_shards()))


if __name__ == '__main__':
    unittest.main(verbosity=2)
