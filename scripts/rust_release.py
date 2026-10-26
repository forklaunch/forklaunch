#!/usr/bin/env python3
"""Prepare and validate ForkLaunch crate releases without publishing implicitly."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tempfile
import time
import tomllib
from urllib.error import HTTPError
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = "https://github.com/rust-lang/crates.io-index"
# Each publishable scope is one workspace with its own version and tag. The
# blueprint follows the framework: it pins already-published framework releases.
SCOPES = {"framework": "framework-rs", "blueprint": "blueprint-rs"}
TAG_PREFIX = {"framework": "rust-v", "blueprint": "rust-blueprint-v"}


def scope_version(scope: str, root: Path | None = None) -> str:
    return tomllib.loads(((root or ROOT) / SCOPES[scope] / "Cargo.toml").read_text())["workspace"]["package"]["version"]


def cargo_manifests(root: Path, scope: str = "framework") -> dict[str, tuple[Path, dict]]:
    result = {}
    if scope not in (*SCOPES, "all"):
        raise ValueError(f"Unsupported release scope: {scope}")
    workspaces = tuple(SCOPES.values()) if scope == "all" else (SCOPES[scope],)
    for workspace in workspaces:
        inherited = tomllib.loads((root / workspace / "Cargo.toml").read_text())["workspace"]["package"]
        for path in sorted((root / workspace / "crates").rglob("Cargo.toml")):
            manifest = tomllib.loads(path.read_text())
            package = manifest.get("package", {})
            name = package.get("name", "")
            if not name.startswith("forklaunch-"):
                continue
            if not re.fullmatch(r"forklaunch-[a-z0-9-]+", name) or name in result:
                raise ValueError(f"Invalid or duplicate package name: {name}")
            for key, value in list(package.items()):
                if isinstance(value, dict) and value.get("workspace"):
                    package[key] = inherited[key]
            result[name] = (path, manifest)
    return result


def dependency_tables(manifest: dict):
    for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
        yield kind, None, manifest.get(kind, {})
    for target, values in manifest.get("target", {}).items():
        for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
            yield kind, target, values.get(kind, {})


def release_plan(root: Path = ROOT, scope: str = "framework") -> list[dict]:
    manifests = cargo_manifests(root, scope)
    paths = {path.resolve(): name for name, (path, _) in manifests.items()}
    # Blueprint crates may depend on framework crates by path for development;
    # those must be exact pins of framework versions published beforehand.
    released = cargo_manifests(root, "framework") if scope == "blueprint" else {}
    released_paths = {path.resolve(): name for name, (path, _) in released.items()}
    packages = {}
    for name, (path, manifest) in manifests.items():
        package = manifest["package"]
        if package.get("publish") is False:
            continue
        if package.get("publish") not in (None, ["crates-io"]):
            raise ValueError(f"Unsupported publication registry: {name}")
        for key in ("version", "description", "license", "repository"):
            if not isinstance(package.get(key), str) or not package[key]:
                raise ValueError(f"Missing {key}: {name}")
        dependencies = set()
        external = set()
        for kind, _, table in dependency_tables(manifest):
            for alias, spec in table.items():
                if not isinstance(spec, dict):
                    continue
                if "git" in spec:
                    raise ValueError(f"Unpublishable Git dependency: {name}/{alias}")
                if "path" not in spec:
                    continue
                target = (path.parent / spec["path"] / "Cargo.toml").resolve()
                if target in released_paths:
                    released_name = released_paths[target]
                    released_version = released[released_name][1]["package"]["version"]
                    if spec.get("version") != "=" + released_version:
                        raise ValueError(f"Framework dependency must pin the exact framework version {released_version}: {name}/{alias}")
                    if kind != "dev-dependencies":
                        external.add(f"{released_name}@{released_version}")
                    continue
                if target not in paths:
                    raise ValueError(f"Uninventoried path dependency outside {scope} release scope: {name}/{alias}")
                target_name = paths[target]
                target_package = manifests[target_name][1]["package"]
                if target_package.get("publish") is False:
                    raise ValueError(f"Public crate depends on private sample: {name}/{alias}")
                if spec.get("package", alias) != target_name:
                    raise ValueError(f"Dependency package name mismatch: {name}/{alias}")
                local_dev = kind == "dev-dependencies" and "version" not in spec
                if not local_dev and spec.get("version", "").removeprefix("=") != target_package["version"]:
                    raise ValueError(f"Missing or stale path dependency version: {name}/{alias}")
                # Cargo omits unversioned dev paths when packaging. Versioned
                # dev dependencies must exist when `publish` resolves its lock.
                if not local_dev:
                    dependencies.add(target_name)
        packages[name] = {
            "name": name, "version": package["version"],
            "manifest": str(path.relative_to(root)),
            "dependencies": sorted(dependencies),
            **({"externalDependencies": sorted(external)} if scope == "blueprint" else {}),
        }
    pending = {name: set(package["dependencies"]) for name, package in packages.items()}
    order = []
    while pending:
        ready = sorted(name for name, dependencies in pending.items() if not dependencies)
        if not ready:
            raise ValueError(f"Publication dependency cycle: {sorted(pending)}")
        order.extend(packages[name] for name in ready)
        for name in ready:
            del pending[name]
        for dependencies in pending.values():
            dependencies.difference_update(ready)
    return order


def run(command: list[str], **kwargs):
    subprocess.run(command, check=True, **kwargs)


def hydrate_verified_release(output: Path, repository: str, tag: str):
    """Retrieve exact prior archives from an explicitly selected public release."""
    from rust_registry import checked_archive
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Expected an explicit GitHub owner/repository")
    if not re.fullmatch(r"rust-v\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?", tag):
        raise ValueError("Expected an explicit rust-vVERSION release tag")
    metadata = json.loads(subprocess.check_output([
        "gh", "release", "view", tag, "--repo", repository, "--json", "tagName,isDraft,url"], text=True))
    url = f"https://github.com/{repository}/releases/tag/{tag}"
    if metadata.get("tagName") != tag or metadata.get("isDraft") is not False or metadata.get("url") != url:
        raise ValueError("Prior release must be the requested public tagged release")
    commit = json.loads(subprocess.check_output([
        "gh", "api", f"repos/{repository}/commits/{tag}"], text=True)).get("sha")
    output.mkdir(parents=True, exist_ok=False)
    run(["gh", "release", "download", tag, "--repo", repository, "--dir", str(output),
         "--pattern", "release.json", "--pattern", "verification.json", "--pattern", "publication.json"])
    previous = verified_release_archives(output)
    if previous["commit"] != commit:
        raise ValueError("Prior release report must match the selected tag commit")
    with tempfile.TemporaryDirectory(prefix="download-", dir=output) as temporary:
        downloads = Path(temporary)
        for package in previous["packages"].values():
            relative = PurePosixPath(package["archive"])
            asset = f"{package['name']}-{package['version']}.crate"
            if (relative.is_absolute() or ".." in relative.parts or "\\" in str(relative)
                    or relative.name != asset or not re.fullmatch(r"[A-Za-z0-9_.+-]+\.crate", asset)):
                raise ValueError("Prior release has an unsafe or noncanonical package asset path")
            run(["gh", "release", "download", tag, "--repo", repository, "--dir", str(downloads),
                 "--pattern", asset])
            data = checked_archive(downloads, {**package, "archive": asset})
            comparable_package_files(data, package)  # Validate archive identity and members as well.
            destination = output / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
    receipt = {"schemaVersion": 1, "repository": repository, "tag": tag, "url": url,
               "commit": commit, "releaseSha256": previous["reportSha256"],
               "archives": [{"name": package["name"], "version": package["version"], "sha256": package["sha256"]}
                            for package in previous["packages"].values()]}
    (output / "hydration.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"Retrieved {len(receipt['archives'])} exact verified archives from {url}")


def verified_release_archives(directory: Path) -> dict:
    """Load an explicitly selected, completely verified and published release."""
    report_bytes = (directory / "release.json").read_bytes()
    report = json.loads(report_bytes)
    verification = json.loads((directory / "verification.json").read_text())
    publication = json.loads((directory / "publication.json").read_text())
    checksum = hashlib.sha256(report_bytes).hexdigest()
    packages = report.get("packages", [])
    if (report.get("schemaVersion") != 1 or report.get("scope") not in SCOPES
            or report.get("dirty") is not False or not report.get("commit")
            or verification.get("schemaVersion") != 1
            or verification.get("releaseSha256") != checksum
            or verification.get("packages") != len(packages)
            or verification.get("httpMcpStartup") is not True
            or publication.get("schemaVersion") != 1
            or publication.get("scope") != report.get("scope")
            or publication.get("releaseSha256") != checksum
            or publication.get("complete") is not True):
        raise ValueError("Archive reuse requires a clean, verified and completely published prior release")
    if any(package["name"] == "forklaunch-regress" for package in packages) and verification.get("boundedRegex") is not True:
        raise ValueError("Prior release lacks bounded regex verification")
    if any(package["name"] == "forklaunch-worker-interface" for package in packages) and verification.get("nativeHttpWorkerFeatures") is not True:
        raise ValueError("Prior release lacks native HTTP/worker verification")
    entries = {(package["name"], package["version"]): package for package in packages}
    receipts = {(package["name"], package["version"]): package for package in publication.get("packages", [])}
    if (len(entries) != len(packages) or len(receipts) != len(publication.get("packages", []))
            or entries.keys() != receipts.keys()
            or any(not re.fullmatch(r"[0-9a-f]{64}", entry.get("registrySha256", "")) for entry in receipts.values())):
        raise ValueError("Prior release publication inventory does not match its verified packages")
    result = {"directory": directory, "reportSha256": checksum, "commit": report["commit"],
              "packages": entries, "publication": receipts}
    hydration = directory / "hydration.json"
    if hydration.is_file():
        public = json.loads(hydration.read_text())
        if public.get("commit") != report["commit"] or public.get("releaseSha256") != checksum:
            raise ValueError("Public release retrieval receipt does not match verified release")
        result["publicRelease"] = {key: public[key] for key in ("repository", "tag", "url", "commit", "releaseSha256")}
    return result


def comparable_package_files(data: bytes, package: dict) -> dict:
    """Compare packaged contents, excluding only Cargo-generated lock/VCS files."""
    from rust_registry import safe_members
    prefix = f"{package['name']}-{package['version']}"
    files = {}
    seen = set()
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in safe_members(archive):
            path = PurePosixPath(member.name)
            if not path.parts or path.parts[0] != prefix:
                raise ValueError("Package archive has unexpected root")
            relative = str(PurePosixPath(*path.parts[1:]))
            if relative in seen:
                raise ValueError(f"Duplicate package archive member: {member.name}")
            seen.add(relative)
            if member.isfile() and relative not in ("Cargo.lock", ".cargo_vcs_info.json"):
                files[relative] = {"sha256": hashlib.sha256(archive.extractfile(member).read()).hexdigest(),
                                   "mode": member.mode & 0o777}
        manifest = tomllib.loads(archive.extractfile(f"{prefix}/Cargo.toml").read().decode())
        if (manifest["package"]["name"], manifest["package"]["version"]) != (package["name"], package["version"]):
            raise ValueError("Archive package identity mismatch")
    return files


def reuse_verified_archive(output: Path, package: dict, previous: dict):
    """Retain the old exact archive only after fresh packaging proves equality."""
    from rust_registry import checked_archive
    key = (package["name"], package["version"])
    prior = previous["packages"].get(key)
    if prior is None:
        return
    old_bytes = checked_archive(previous["directory"], prior)
    fresh_bytes = checked_archive(output, package)
    fresh_files = comparable_package_files(fresh_bytes, package)
    if fresh_files != comparable_package_files(old_bytes, package):
        raise ValueError(f"Cannot reuse changed package at unchanged version: {package['name']} {package['version']}; bump its version")
    fresh = output / "fresh-packages" / f"{package['name']}-{package['version']}.crate"
    fresh.parent.mkdir(exist_ok=True)
    fresh.write_bytes(fresh_bytes)
    package["verifiedArchiveReuse"] = {
        "sourceRelease": str(previous["directory"]),
        "sourceReleaseSha256": previous["reportSha256"],
        "sourceCommit": previous["commit"],
        "sourceArchiveSha256": prior["sha256"],
        "sourceRegistrySha256": previous["publication"][key]["registrySha256"],
        "freshArchive": str(fresh.relative_to(output)),
        "freshArchiveSha256": package["sha256"],
        "comparedFiles": len(fresh_files),
        "packagedContentSha256": hashlib.sha256(json.dumps(fresh_files, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
        "ignoredGeneratedFiles": ["Cargo.lock", ".cargo_vcs_info.json"],
    }
    if "publicRelease" in previous:
        package["verifiedArchiveReuse"]["sourcePublicRelease"] = previous["publicRelease"]
    (output / package["archive"]).write_bytes(old_bytes)
    package["sha256"] = prior["sha256"]


def validate_archive_reuse(output: Path, package: dict):
    from rust_registry import checked_archive
    provenance = package.get("verifiedArchiveReuse")
    if provenance is None:
        return
    fresh = checked_archive(output, {"archive": provenance["freshArchive"], "sha256": provenance["freshArchiveSha256"]})
    files = comparable_package_files(fresh, package)
    digest = hashlib.sha256(json.dumps(files, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if (files != comparable_package_files(checked_archive(output, package), package)
            or provenance.get("sourceArchiveSha256") != package["sha256"]
            or provenance.get("comparedFiles") != len(files)
            or provenance.get("packagedContentSha256") != digest
            or provenance.get("ignoredGeneratedFiles") != ["Cargo.lock", ".cargo_vcs_info.json"]):
        raise ValueError("Reused archive provenance does not match fresh packaged contents")


def prepare(output: Path, allow_dirty: bool, scope: str = "framework", reuse_verified_release: Path | None = None):
    plan = release_plan(scope=scope)
    previous = verified_release_archives(reuse_verified_release) if reuse_verified_release is not None else None
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT).strip())
    if not allow_dirty and dirty:
        raise ValueError("Release preparation requires a clean checkout (or explicit --allow-dirty for local validation)")
    output.mkdir(parents=True, exist_ok=False)
    target = output / "cargo-target"
    for package in plan:
        command = ["cargo", "package", "--manifest-path", str(ROOT / package["manifest"]),
                   "--no-verify", "--exclude-lockfile", "--target-dir", str(target)]
        if allow_dirty:
            command.append("--allow-dirty")
        run(command, cwd=ROOT)
        archive = target / "package" / f"{package['name']}-{package['version']}.crate"
        package["archive"] = str(archive.relative_to(output))
        package["sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
        with tarfile.open(archive) as contents:
            member = f"{package['name']}-{package['version']}/Cargo.toml"
            normalized = tomllib.loads(contents.extractfile(member).read().decode())
            for _, _, table in dependency_tables(normalized):
                if any(isinstance(spec, dict) and "path" in spec for spec in table.values()):
                    raise ValueError(f"Packaged path dependency remains: {package['name']}")
        if previous is not None:
            reuse_verified_archive(output, package, previous)
    report = {
        "schemaVersion": 1,
        "scope": scope,
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "dirty": dirty,
        "packages": plan,
    }
    (output / "release.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Prepared {len(plan)} registry crates in {output}")


def publication_preflight(output: Path, tag: str):
    from rust_registry import checked_archive, sdk_discovery_supported
    report_bytes = (output / "release.json").read_bytes()
    report = json.loads(report_bytes)
    verified = json.loads((output / "verification.json").read_text())
    if report.get("schemaVersion") != 1 or report.get("dirty") is not False:
        raise ValueError("Publication requires a clean release report")
    # The framework proves HTTP/MCP startup; the blueprint proves an all-features
    # install against the public framework crates it pins.
    if report.get("scope") == "blueprint":
        receipt_complete = verified.get("allFeatures") is True and verified.get("frameworkFromCratesIo") is True
    else:
        receipt_complete = verified.get("httpMcpStartup") is True
    if (verified.get("schemaVersion") != 1 or not receipt_complete
            or verified.get("releaseSha256") != hashlib.sha256(report_bytes).hexdigest()
            or verified.get("packages") != len(report["packages"])):
        raise ValueError("Missing or mismatched registry/startup verification")
    if (any(package["name"] == "forklaunch-regress" for package in report["packages"])
            and verified.get("boundedRegex") is not True):
        raise ValueError("Missing bounded regex registry verification")
    if (any(package["name"] == "forklaunch-worker-interface" for package in report["packages"])
            and verified.get("nativeHttpWorkerFeatures") is not True):
        raise ValueError("Missing native HTTP/worker feature registry verification")
    if (any(package["name"] == "forklaunch-core" and sdk_discovery_supported(package["version"])
            for package in report["packages"])
            and verified.get("installedSdkDiscovery") is not True):
        raise ValueError("Missing installed SDK discovery registry verification")
    scope = report.get("scope")
    if scope not in SCOPES or any(
        not package["manifest"].startswith(f"{SCOPES[scope]}/crates/")
        for package in report["packages"]
    ):
        raise ValueError("A release publishes exactly one scope: framework or blueprint")
    # Compare the same fields on both sides, including the framework pins.
    fields = ("name", "version", "manifest", "dependencies", "externalDependencies")
    plan = [{key: package.get(key, []) for key in fields} for package in release_plan(scope=scope)]
    prepared = [{key: package.get(key, []) for key in fields} for package in report["packages"]]
    if prepared != plan:
        raise ValueError("Prepared package inventory differs from checkout")
    version = scope_version(scope)
    if tag != f"{TAG_PREFIX[scope]}{version}":
        raise ValueError(f"Publication requires tag {TAG_PREFIX[scope]}{version}")
    # The blueprint installs against public framework crates; never publish ahead of them.
    for dependency in sorted({d for package in report["packages"] for d in package.get("externalDependencies", [])}):
        name, released_version = dependency.split("@")
        if published_version({"name": name, "version": released_version}) is None:
            raise ValueError(f"Publish framework {name} {released_version} before the blueprint")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    tagged = subprocess.check_output(["git", "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}"], cwd=ROOT, text=True).strip()
    if head != report["commit"] or tagged != head:
        raise ValueError("Release report, tag and checkout must identify the same commit")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT).strip():
        raise ValueError("Publication requires a clean checkout")
    for package in report["packages"]:
        checked_archive(output, package)
        validate_archive_reuse(output, package)
    return report


def published_version(package: dict):
    name = package["name"]
    url = f"https://index.crates.io/{name[:2]}/{name[2:4]}/{name}"
    try:
        with urlopen(url, timeout=20) as response:
            entries = response.read(8 * 1024 * 1024).decode().splitlines()
    except HTTPError as error:
        if error.code == 404:
            return None
        raise
    for line in entries:
        entry = json.loads(line)
        if entry["vers"] == package["version"]:
            if entry.get("yanked"):
                raise ValueError(f"Refusing to reuse yanked version: {name} {package['version']}")
            return entry
    return None


def wait_for_version(package: dict, timeout: float = 180):
    deadline = time.monotonic() + timeout
    while True:
        entry = published_version(package)
        if entry is not None:
            return entry
        if time.monotonic() >= deadline:
            raise ValueError(f"Published version is not yet available: {package['name']}; rerun to resume")
        time.sleep(2)


def verify_published_archive(output: Path, package: dict, entry: dict) -> str:
    from rust_registry import checked_archive, safe_members
    name, version = package["name"], package["version"]
    with urlopen(f"https://static.crates.io/crates/{name}/{name}-{version}.crate", timeout=30) as response:
        published = response.read(32 * 1024 * 1024)
    checksum = hashlib.sha256(published).hexdigest()
    if checksum != entry["cksum"]:
        raise ValueError(f"Published archive checksum mismatch: {name}")

    def files(data):
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            return {member.name: archive.extractfile(member).read()
                    for member in safe_members(archive)
                    if member.isfile() and member.name != f"{name}-{version}/Cargo.lock"}

    # Cargo may add a resolved lockfile when publishing after dependencies become
    # available. Every source/manifest/VCS-info file must match the verified package.
    if files(published) != files(checked_archive(output, package)):
        raise ValueError(f"Existing registry version differs from verified release: {name} {version}")
    return checksum


def publish(output: Path, tag: str, upload: bool):
    report = publication_preflight(output, tag)
    # Cargo resolves credentials through its configured provider, including
    # cargo login and registry-specific environment variables. Never print them.
    receipts = []
    for package in report["packages"]:
        command = ["cargo", "publish", "--manifest-path", str(ROOT / package["manifest"]),
                   "--registry", "crates-io", "--no-verify"]
        if upload:
            entry = published_version(package)
            reused = entry is not None
            if entry is None:
                print(f"Publishing {package['name']} {package['version']}", flush=True)
                run(command, cwd=ROOT)
                entry = wait_for_version(package)
            checksum = verify_published_archive(output, package, entry)
            receipts.append({"name": package["name"], "version": package["version"],
                             "registrySha256": checksum, "reusedMatchingVersion": reused})
            progress = {"schemaVersion": 1, "scope": report.get("scope", "framework"), "tag": tag,
                        "releaseSha256": hashlib.sha256((output / "release.json").read_bytes()).hexdigest(),
                        "packages": receipts, "complete": len(receipts) == len(report["packages"])}
            temporary = output / "publication.json.tmp"
            temporary.write_text(json.dumps(progress, indent=2) + "\n")
            temporary.replace(output / "publication.json")
            print(f"Verified registry archive {package['name']} ({len(receipts)}/{len(report['packages'])})", flush=True)
        else:
            print(" ".join(command))
    print("Published release in dependency order" if upload else "Preflight passed; commands shown only, no uploads performed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["plan", "prepare", "verify", "publish", "hydrate"])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--allow-dirty", action="store_true")
    parser.add_argument("--reuse-verified-release", type=Path,
                        help="Prepare only: reuse exact prior verified/published archives after fresh packaged-content comparison")
    parser.add_argument("--scope", choices=["framework", "blueprint", "all"], default="framework",
                        help="Package scope; 'all' lists both inventories but cannot be prepared or published")
    parser.add_argument("--tag")
    parser.add_argument("--repository", help="Hydrate only: explicit GitHub owner/repository")
    parser.add_argument("--upload", action="store_true", help="Actually upload crates (publish only)")
    parser.add_argument("--framework-release", type=Path,
                        help="Verify only: serve this prepared framework release beside an unpublished blueprint release")
    args = parser.parse_args()
    if args.reuse_verified_release is not None and args.command != "prepare":
        parser.error("--reuse-verified-release applies only to prepare")
    if args.repository is not None and args.command != "hydrate":
        parser.error("--repository applies only to hydrate")
    if args.command == "plan":
        print(json.dumps(release_plan(scope=args.scope), indent=2))
    elif args.command == "hydrate":
        if args.output is None or args.repository is None or args.tag is None:
            parser.error("hydrate requires --output (a new directory), --repository and --tag")
        hydrate_verified_release(args.output.resolve(), args.repository, args.tag)
    elif args.command == "prepare":
        if args.scope == "all":
            parser.error("prepare one scope at a time: --scope framework or --scope blueprint")
        if args.output is None:
            parser.error("prepare requires --output (a new directory)")
        prepare(args.output.resolve(), args.allow_dirty, args.scope,
                args.reuse_verified_release.resolve() if args.reuse_verified_release is not None else None)
    elif args.command == "verify":
        if args.output is None or args.target_dir is None:
            parser.error("verify requires --output (prepared release) and --target-dir")
        from rust_registry import verify, verify_blueprint
        scope = json.loads((args.output / "release.json").read_text()).get("scope")
        if scope == "blueprint":
            verify_blueprint(args.output.resolve(), args.target_dir.resolve(),
                             args.framework_release.resolve() if args.framework_release else None)
        else:
            verify(args.output.resolve(), args.target_dir.resolve())
    else:
        if args.scope == "all":
            parser.error("publish one scope at a time: --scope framework or --scope blueprint")
        if args.output is None or args.tag is None:
            parser.error("publish requires --output (verified release) and --tag")
        publish(args.output.resolve(), args.tag, args.upload)


if __name__ == "__main__":
    main()
