#!/usr/bin/env python3
"""Check every workspace member without reformatting excluded vendored sources.

`cargo fmt --all` also traverses local path dependencies, including archived
upstream/generated sources deliberately excluded from these workspaces.
"""

import argparse
import json
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    args = parser.parse_args()
    manifest = args.manifest.resolve(strict=True)
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo", "metadata", "--manifest-path", str(manifest),
                "--format-version", "1", "--no-deps", "--offline", "--locked",
            ],
            text=True,
        )
    )
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    if not packages or len(packages) != len(members):
        raise RuntimeError("Cargo metadata did not resolve every workspace member")
    command = ["cargo", "fmt", "--manifest-path", str(manifest)]
    for package in sorted(packages, key=lambda p: p["name"]):
        command.extend(["--package", package["name"]])
    command.extend(["--", "--check"])
    print(f"Checking formatting for {len(packages)} workspace members", flush=True)
    return subprocess.call(command)


if __name__ == "__main__":
    raise SystemExit(main())
