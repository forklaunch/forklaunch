#!/usr/bin/env python3
"""Offline checks that the advisory gate fails closed for new/changed findings."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

GATE = Path(__file__).with_name("rust_security_audit.py")
FAKE = r'''
import json, os, sys
if '--version' in sys.argv:
 print('cargo-audit 0.22.2');raise SystemExit(0)
mode=os.environ['AUDIT_TEST_MODE']
framework='framework-rs' in sys.argv[sys.argv.index('--file')+1]
item={'advisory':{'id':'RUSTSEC-2026-0253'},'package':{'name':'lru','version':'0.16.4'}}
if mode=='new-advisory':item['advisory']['id']='RUSTSEC-2099-9999'
if mode=='new-version':item['package']['version']='0.16.5'
report={'database':{'advisory-count':1},'lockfile':{'dependency-count':1},
 'settings':{'ignore':['hidden'] if mode=='filtered' else [],'severity':None,'target_arch':[],'target_os':[]},
 'vulnerabilities':{'count':0,'list':[]},'warnings':{'unsound':[item]} if framework else {}}
if mode=='scanner-error':print('not a valid report');raise SystemExit(2)
print(json.dumps(report))
'''


class AdvisoryGate(unittest.TestCase):
    def run_gate(self, mode):
        with tempfile.TemporaryDirectory(prefix="fl-audit-gate-test-") as directory:
            tool = Path(directory) / "cargo-audit"
            tool.write_text(f"#!{sys.executable}\n" + FAKE)
            tool.chmod(0o700)
            environment = {**os.environ, "CARGO_AUDIT_BIN": str(tool), "AUDIT_TEST_MODE": mode}
            environment.pop("FORKLAUNCH_SECURITY_REPORT", None)
            return subprocess.run([sys.executable, str(GATE)], env=environment,
                                  text=True, capture_output=True, timeout=30)

    def test_reviewed_finding_stays_visible(self):
        result = self.run_gate("reviewed")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("RUSTSEC-2026-0253", result.stdout)

    def test_new_findings_versions_filters_and_scanner_failures_are_rejected(self):
        for mode in ("new-advisory", "new-version", "filtered", "scanner-error"):
            with self.subTest(mode=mode):
                self.assertNotEqual(self.run_gate(mode).returncode, 0)


if __name__ == "__main__":
    unittest.main()
