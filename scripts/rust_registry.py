"""Loopback sparse registry for validating prepared crates before publication.

Only owned dependency registry locations change in staged archives. Original
release archives remain intact; external dependencies still use crates.io.
"""
from __future__ import annotations

import contextlib
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path, PurePosixPath
import tarfile
import threading
import tomllib

from rust_release import ROOT, REGISTRY, dependency_tables, run


def toml_value(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        return "[" + ", ".join(map(toml_value, value)) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{toml_value(k)} = {toml_value(v)}" for k, v in value.items()) + " }"
    raise ValueError(f"Unsupported manifest value: {type(value)}")


def manifest_text(manifest):
    text = "\n".join(f"{toml_value(k)} = {toml_value(v)}" for k, v in manifest.items()) + "\n"
    if tomllib.loads(text) != manifest:
        raise ValueError("Manifest serialization changed values")
    return text.encode()


def checked_archive(directory, entry):
    path = (directory / entry["archive"]).resolve()
    if not path.is_relative_to(directory.resolve()):
        raise ValueError("Archive path escapes release directory")
    contents = path.read_bytes()
    if hashlib.sha256(contents).hexdigest() != entry["sha256"]:
        raise ValueError(f"Archive checksum mismatch: {path.name}")
    return contents


def safe_members(archive):
    for member in archive.getmembers():
        path = PurePosixPath(member.name)
        if path.is_absolute() or ".." in path.parts or not (member.isfile() or member.isdir()):
            raise ValueError(f"Unsafe archive member: {member.name}")
        yield member


def stage_package(contents, package, names):
    prefix = f"{package['name']}-{package['version']}/"
    with tarfile.open(fileobj=io.BytesIO(contents), mode="r:gz") as source:
        members = list(safe_members(source))
        if any(not member.name.startswith(prefix) for member in members):
            raise ValueError("Package archive has unexpected root")
        manifest = tomllib.loads(source.extractfile(prefix + "Cargo.toml").read().decode())
        if (manifest["package"]["name"], manifest["package"]["version"]) != (package["name"], package["version"]):
            raise ValueError("Archive package identity mismatch")
        dependencies = []
        for kind, target, table in dependency_tables(manifest):
            for alias, spec in table.items():
                if isinstance(spec, str):
                    spec = table[alias] = {"version": spec}
                name = spec.get("package", alias)
                if any(key in spec for key in ("path", "git", "registry", "registry-index")):
                    raise ValueError(f"Unexpected dependency source in prepared crate: {alias}")
                if name in names:
                    spec["registry"] = "forklaunch-staging"
                dependencies.append({
                    "name": alias, "req": spec["version"], "features": spec.get("features", []),
                    "optional": spec.get("optional", False),
                    "default_features": spec.get("default-features", True), "target": target,
                    "kind": {"dependencies": "normal", "build-dependencies": "build", "dev-dependencies": "dev"}[kind],
                    "registry": None if name in names else REGISTRY,
                    "package": spec.get("package"),
                })
        staged = io.BytesIO()
        with tarfile.open(fileobj=staged, mode="w:gz") as destination:
            for member in members:
                data = source.extractfile(member).read() if member.isfile() else None
                if member.name == prefix + "Cargo.toml":
                    data = manifest_text(manifest)
                    member.size = len(data)
                destination.addfile(member, io.BytesIO(data) if data is not None else None)
    data = staged.getvalue()
    index = {
        "name": package["name"], "vers": package["version"], "deps": dependencies,
        "cksum": hashlib.sha256(data).hexdigest(), "features": {},
        "features2": manifest.get("features", {}), "v": 2, "yanked": False,
    }
    for key in ("links", "rust-version"):
        if key in manifest["package"]:
            index[key.replace("-", "_")] = manifest["package"][key]
    return data, index


@contextlib.contextmanager
def registry(directory, report, *also):
    """Serve one or more prepared releases from one loopback sparse index.

    Extra (directory, report) pairs let an unpublished blueprint release
    install against an unpublished framework release."""
    responses = {}
    releases = [(directory, report), *also]
    packages = [(source, package) for source, release in releases for package in release["packages"]]
    names = {package["name"] for _, package in packages}
    if len(names) != len(packages):
        raise ValueError("Duplicate release package")
    for source, package in packages:
        data, entry = stage_package(checked_archive(source, package), package, names)
        name = package["name"]
        responses[f"/{name[:2]}/{name[2:4]}/{name}"] = (json.dumps(entry) + "\n").encode()
        responses[f"/api/v1/crates/{name}/{package['version']}/download"] = data

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            body = responses.get(self.path)
            if body is None:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Content-Type", "application/octet-stream")
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    base = f"http://127.0.0.1:{server.server_port}"
    responses["/config.json"] = json.dumps({"dl": base + "/api/v1/crates/{crate}/{version}/download"}).encode()
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield "sparse+" + base + "/"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def verify_blueprint(directory: Path, target: Path, framework: Path | None = None):
    """Install every staged blueprint crate with all features against framework crates:
    public ones, or a prepared framework release served beside the blueprint."""
    report = json.loads((directory / "release.json").read_text())
    if report.get("scope") != "blueprint":
        raise ValueError("Blueprint verification requires a blueprint release report")
    consumer = directory / "registry-consumer"
    consumer.mkdir(exist_ok=False)
    dependencies = {}
    for package in report["packages"]:
        with tarfile.open(fileobj=io.BytesIO(checked_archive(directory, package)), mode="r:gz") as archive:
            manifest = tomllib.loads(archive.extractfile(f"{package['name']}-{package['version']}/Cargo.toml").read().decode())
        features = sorted(feature for feature in manifest.get("features", {}) if feature != "default")
        dependencies[package["name"]] = {"version": "=" + package["version"], "registry": "forklaunch-staging", "features": features}
    # Building worker-interface with every feature covers the native worker proof.
    worker = dependencies.get("forklaunch-worker-interface")
    native_worker = worker is not None and {"native", "apalis", "schema", "telemetry"} <= set(worker["features"])
    if worker is not None and not native_worker:
        raise ValueError("forklaunch-worker-interface lacks native/apalis/schema/telemetry features")
    (consumer / "Cargo.toml").write_bytes(manifest_text({
        "package": {"name": "forklaunch-blueprint-registry-smoke", "version": "0.0.0", "edition": "2024", "publish": False},
        "dependencies": dependencies,
    }))
    (consumer / "src").mkdir()
    (consumer / "src/lib.rs").write_text("//! Compiles every published blueprint crate from the registry.\n" + "".join(
        f"pub use {name.replace('-', '_')} as _{index};\n" for index, name in enumerate(dependencies)))
    (consumer / ".cargo").mkdir()
    env = {**os.environ, "CARGO_INCREMENTAL": "0", "CARGO_BUILD_JOBS": os.environ.get("CARGO_BUILD_JOBS", "2")}
    framework_report = json.loads((framework / "release.json").read_text()) if framework else None
    if framework_report is not None and framework_report.get("scope") != "framework":
        raise ValueError("--framework-release must be a prepared framework release")
    extra = [(framework, framework_report)] if framework_report else []
    with registry(directory, report, *extra) as index:
        (consumer / ".cargo/config.toml").write_bytes(manifest_text({"registries": {"forklaunch-staging": {"index": index}}}))
        run(["cargo", "build", "--target-dir", str(target)], cwd=consumer, env=env)
        import subprocess
        metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=consumer, env=env))
    staged_framework = {package["name"] for package in framework_report["packages"]} if framework_report else set()
    resolved = {package["name"]: package for package in metadata["packages"]}
    for package in report["packages"]:
        actual = resolved[package["name"]]
        if actual["version"] != package["version"] or index.removeprefix("sparse+") not in (actual["source"] or ""):
            raise ValueError(f"Crate was not installed from staging registry: {package['name']}")
    for dependency in sorted({d for package in report["packages"] for d in package.get("externalDependencies", [])}):
        name, version = dependency.split("@")
        actual = resolved.get(name)
        expected_source = index.removeprefix("sparse+") if name in staged_framework else "crates.io-index"
        if actual is None or actual["version"] != version or expected_source not in (actual["source"] or ""):
            raise ValueError(f"Framework dependency was not installed from its release: {dependency}")
    for package in metadata["packages"]:
        if package["source"] is None and package["name"] != "forklaunch-blueprint-registry-smoke":
            raise ValueError(f"Unexpected local dependency: {package['name']}")
    (directory / "verification.json").write_text(json.dumps({
        "schemaVersion": 1, "releaseSha256": hashlib.sha256((directory / "release.json").read_bytes()).hexdigest(),
        "packages": len(report["packages"]), "allFeatures": True, "frameworkFromCratesIo": framework_report is None,
        "nativeHttpWorkerFeatures": native_worker,
        "scope": "local sparse registry; every blueprint crate with all non-default features, framework from "
                 + ("the prepared framework release" if framework_report else "crates.io"),
    }, indent=2) + "\n")
    print(f"Verified {len(report['packages'])} registry-installed blueprint crates against "
          + ("the prepared framework release" if framework_report else "published framework crates"))


def sdk_discovery_supported(version: str) -> bool:
    return tuple(map(int, version.split("-")[0].split("."))) >= (0, 1, 1)


def verify(directory: Path, target: Path):
    report = json.loads((directory / "release.json").read_text())
    consumer = directory / "registry-consumer"
    consumer.mkdir(exist_ok=False)
    dependencies = {
        package["name"]: {"version": "=" + package["version"], "registry": "forklaunch-staging"}
        for package in report["packages"]
    }
    for name in ("forklaunch-jsonschema-protocol", "forklaunch-referencing-protocol"):
        if name in dependencies:
            dependencies[name]["default-features"] = False
    dependencies["forklaunch-axum"]["features"] = ["mcp", "native-server", "native-streams"]
    dependencies["forklaunch-core"]["features"] = ["postgres", "postgres09", "encryption", "otlp", "build", "typed-schema", "mappers"]
    native_worker = "forklaunch-worker-interface" in dependencies
    if native_worker:
        dependencies["forklaunch-worker-interface"]["features"] = ["native", "apalis", "schema", "telemetry"]
    bounded_regex = "forklaunch-regress" in dependencies
    if bounded_regex:
        dependencies["forklaunch-common"]["features"] = ["zod-regex"]
    dependencies.update({
        "tokio": {"version": "1", "features": ["rt-multi-thread", "macros", "signal", "net"]},
        "axum": "0.8", "serde_json": "=1.0.151",
    })
    (consumer / "Cargo.toml").write_bytes(manifest_text({
        "package": {"name": "forklaunch-registry-smoke", "version": "0.0.0", "edition": "2024", "publish": False},
        "features": {"bounded-regex": [], "default": ["bounded-regex"] if bounded_regex else []},
        "dependencies": dependencies,
    }))
    (consumer / "src").mkdir()
    (consumer / "src/main.rs").write_text((ROOT / "scripts/rust_registry_smoke.rs").read_text())
    (consumer / ".cargo").mkdir()
    env = {**os.environ, "CARGO_INCREMENTAL": "0", "CARGO_BUILD_JOBS": os.environ.get("CARGO_BUILD_JOBS", "2")}
    with registry(directory, report) as index:
        (consumer / ".cargo/config.toml").write_bytes(manifest_text({"registries": {"forklaunch-staging": {"index": index}}}))
        run(["cargo", "build", "--target-dir", str(target)], cwd=consumer, env=env)
        import subprocess
        metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=consumer, env=env))
        resolved = {p["name"]: p for p in metadata["packages"]}
        for package in report["packages"]:
            actual = resolved[package["name"]]
            if actual["version"] != package["version"] or not actual["source"] or index.removeprefix("sparse+") not in actual["source"]:
                raise ValueError(f"Crate was not installed from staging registry: {package['name']}")
        for package in metadata["packages"]:
            if package["source"] is None and package["name"] != "forklaunch-registry-smoke":
                raise ValueError(f"Unexpected local dependency: {package['name']}")
        # The 0.1.1 adapter publishes checked SDK metadata and its discovery
        # fingerprint. Retain verification compatibility with published 0.1.0.
        core_version = resolved["forklaunch-core"]["version"]
        sdk_discovery = sdk_discovery_supported(core_version)
        run(["node", str(ROOT / "cli/tests/fixtures/rust_mcp_service_startup.mjs"), str(target / "debug/forklaunch-registry-smoke"),
             *(["--verify-sdk-discovery"] if sdk_discovery else [])], cwd=consumer)
    (directory / "verification.json").write_text(json.dumps({
        "schemaVersion": 1, "releaseSha256": hashlib.sha256((directory / "release.json").read_bytes()).hexdigest(),
        "packages": len(report["packages"]), "httpMcpStartup": True,
        "installedSdkDiscovery": sdk_discovery,
        "boundedRegex": bounded_regex,
        "nativeHttpWorkerFeatures": native_worker,
        "scope": "local sparse registry, default features plus core PostgreSQL0.8+0.9/protected disclosure/OTLP, Axum MCP/native-server/native-streams " +
                 (" and native worker lifecycle/Apalis/schema/telemetry" if native_worker else "") +
                 (" and checked common regex" if bounded_regex else "") +
                 "; owned dependency registry metadata rewritten",
    }, indent=2) + "\n")
    print(f"Verified {len(report['packages'])} registry-installed crates and HTTP/MCP startup")
