#!/usr/bin/env python3
"""Check every publishable crate of a workspace with its default features and
with each optional feature alone. An all-features build hides code that needs a
dependency only some feature enables."""
import argparse
import json
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("workspace", type=Path, help="Directory holding the workspace Cargo.toml")
    parser.add_argument("--target-dir", type=Path)
    args = parser.parse_args()
    manifest = args.workspace.resolve() / "Cargo.toml"
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path", str(manifest)]))
    target = ["--target-dir", str(args.target_dir)] if args.target_dir else []
    failures = []
    for package in metadata["packages"]:
        if package.get("publish") == []:
            continue
        cases = [[]] + [["--no-default-features", "--features", feature]
                        for feature in sorted(package["features"]) if feature != "default"]
        for case in cases:
            command = ["cargo", "check", "--quiet", "--manifest-path", str(manifest), "-p", package["name"], *case, *target]
            label = f"{package['name']} [{case[-1] if case else 'default'}]"
            print("Checking", label, flush=True)
            if subprocess.run(command).returncode != 0:
                failures.append(label)
    if failures:
        sys.exit("Feature combinations failed:\n" + "\n".join(failures))
    print("Every publishable crate builds with default features and each feature alone")


if __name__ == "__main__":
    main()
