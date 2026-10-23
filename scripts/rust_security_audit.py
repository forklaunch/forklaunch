#!/usr/bin/env python3
"""Scan all three Rust lockfiles, retaining narrowly reviewed advisory exceptions."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SCOPES = {"framework": "framework-rs", "blueprint": "blueprint-rs", "cli": "cli"}
FIELDS = ("scope", "kind", "id", "package", "version")
EXCEPTIONS = ROOT / "cli/tests/fixtures/rust_security_advisories.json"


def finding(scope, kind, value):
    return dict(zip(FIELDS, (scope, kind, value["advisory"]["id"],
                            value["package"]["name"], value["package"]["version"])))


def main():
    allowed = json.loads(EXCEPTIONS.read_text())
    allowed_keys = {tuple(row[field] for field in FIELDS) for row in allowed}
    assert len(allowed_keys) == len(allowed), "duplicate advisory exceptions"
    assert all(row.get("review") for row in allowed), "exceptions require a review"
    auditor = os.environ.get("CARGO_AUDIT_BIN", "cargo-audit")
    version = subprocess.check_output([auditor, "--version"], text=True).strip()
    assert version == "cargo-audit 0.22.2", f"review a scanner version change: {version}"
    database = os.environ.get("FORKLAUNCH_RUSTSEC_DB", str(Path(tempfile.gettempdir()) / "forklaunch-rustsec-advisory-db"))
    reports = {}
    unexpected = []
    for index, (scope, workspace) in enumerate(SCOPES.items()):
        lockfile = ROOT / workspace / "Cargo.lock"
        assert lockfile.is_file(), f"generate {lockfile} before scanning"
        command = [auditor, "audit", "--db", database, "--file", str(lockfile), "--json"]
        if index:
            command.append("--no-fetch")
        result = subprocess.run(command, text=True, capture_output=True, timeout=300)
        assert result.returncode in (0, 1), f"scanner failed: {result.stderr}"
        report = json.loads(result.stdout)
        assert report["database"]["advisory-count"] > 0, "empty advisory database"
        assert report["lockfile"]["dependency-count"] > 0, "empty dependency scan"
        assert report["settings"]["ignore"] == [], "scanner ignores are forbidden"
        assert report["settings"]["severity"] is None, "severity filtering is forbidden"
        assert not report["settings"]["target_arch"] and not report["settings"]["target_os"], "platform filtering is forbidden"
        assert report["vulnerabilities"]["count"] == len(report["vulnerabilities"]["list"]), "incomplete vulnerability report"
        entries = [finding(scope, "vulnerability", item) for item in report["vulnerabilities"]["list"]]
        for kind, values in report["warnings"].items():
            for item in values:
                # Unknown warning formats must fail rather than disappear.
                entries.append(finding(scope, kind, item))
        if result.returncode == 1:
            assert entries, "scanner failed without reportable findings"
        for entry in entries:
            if tuple(entry[field] for field in FIELDS) not in allowed_keys:
                unexpected.append(entry)
        reports[scope] = {"database": report["database"], "lockfile": report["lockfile"], "findings": entries}
        print(f"{scope}: {len(entries)} advisory findings", file=sys.stderr)
    output = {"scanner": version, "reports": reports, "unexpected": unexpected}
    if destination := os.environ.get("FORKLAUNCH_SECURITY_REPORT"):
        Path(destination).write_text(json.dumps(output, indent=2) + "\n")
    print(json.dumps(output, indent=2))
    if unexpected:
        raise SystemExit("Unreviewed Rust security advisories require remediation/review")
    print("PASS no unreviewed advisories; reviewed findings remain visible", file=sys.stderr)


if __name__ == "__main__":
    main()
