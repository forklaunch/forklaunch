"""Release graph, archive integrity and staging-source regression checks."""
import hashlib
import io
import json
import subprocess
from pathlib import Path
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch
from urllib.request import urlopen

import rust_release as release
import rust_registry as registry


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for workspace in ("framework-rs", "blueprint-rs"):
            directory = self.root / workspace
            (directory / "crates").mkdir(parents=True)
            (directory / "Cargo.toml").write_text('[workspace.package]\nversion="0.1.0"\nlicense="MIT"\nrepository="https://example.test/repo"\n')

    def crate(self, name, dependencies=None, **package):
        path = self.root / "framework-rs/crates" / name / "Cargo.toml"
        path.parent.mkdir(parents=True, exist_ok=True)
        manifest = {"package": {"name": name, "version": {"workspace": True}, "license": {"workspace": True},
                    "repository": {"workspace": True}, "description": "fixture", **package}, "dependencies": dependencies or {}}
        path.write_bytes(registry.manifest_text(manifest))
        return path

    def test_graph_versions_and_private_samples(self):
        self.crate("forklaunch-base")
        self.crate("forklaunch-app", {"base": {"package": "forklaunch-base", "path": "../forklaunch-base", "version": "=0.1.0"}})
        self.crate("forklaunch-sample", publish=False)
        self.assertEqual([p["name"] for p in release.release_plan(self.root)], ["forklaunch-base", "forklaunch-app"])

    def test_framework_default_excludes_blueprint_and_all_is_explicit(self):
        self.crate("forklaunch-base")
        blueprint = self.root / "blueprint-rs/crates/worker/Cargo.toml"
        blueprint.parent.mkdir()
        blueprint.write_text('[package]\nname="forklaunch-worker"\nversion="0.1.0"\nlicense="MIT"\nrepository="https://example.test/repo"\ndescription="worker"\n')
        self.assertEqual([p["name"] for p in release.release_plan(self.root)], ["forklaunch-base"])
        self.assertEqual([p["name"] for p in release.release_plan(self.root, "all")], ["forklaunch-base", "forklaunch-worker"])
        self.crate("forklaunch-base", {"forklaunch-worker": {"path": "../../../blueprint-rs/crates/worker", "version": "0.1.0"}})
        with self.assertRaisesRegex(ValueError, "outside framework release scope"):
            release.release_plan(self.root)

    def test_blueprint_scope_pins_published_framework_and_publishes_alone(self):
        self.crate("forklaunch-core")
        blueprint = self.root / "blueprint-rs/crates/worker/Cargo.toml"
        blueprint.parent.mkdir()
        def worker(version):
            blueprint.write_bytes(registry.manifest_text({"package": {"name": "forklaunch-worker", "version": "0.1.0",
                "license": "MIT", "repository": "https://example.test/repo", "description": "worker"},
                "dependencies": {"forklaunch-core": {"path": "../../../framework-rs/crates/forklaunch-core", "version": version}}}))
        worker("=0.1.0")
        plan = release.release_plan(self.root, "blueprint")
        self.assertEqual([p["name"] for p in plan], ["forklaunch-worker"])
        self.assertEqual(plan[0]["externalDependencies"], ["forklaunch-core@0.1.0"])
        self.assertNotIn("externalDependencies", release.release_plan(self.root)[0])
        for loose in ["0.1.0", "=0.2.0"]:
            worker(loose)
            with self.assertRaisesRegex(ValueError, "exact framework version"):
                release.release_plan(self.root, "blueprint")
        with patch.object(release, "ROOT", self.root):
            self.assertEqual(release.scope_version("blueprint"), "0.1.0")

    def test_rejects_unpublishable_dependencies(self):
        self.crate("forklaunch-base")
        invalid = [
            {"path": "../forklaunch-base"},
            {"path": "../forklaunch-base", "version": "0.2.0"},
            {"path": "../missing", "version": "0.1.0"},
            {"git": "https://example.test/dependency"},
        ]
        for dependency in invalid:
            with self.subTest(dependency=dependency):
                self.crate("forklaunch-app", {"forklaunch-base": dependency})
                with self.assertRaises(ValueError):
                    release.release_plan(self.root)
        self.crate("forklaunch-base", publish=False)
        self.crate("forklaunch-app", {"forklaunch-base": {"path": "../forklaunch-base", "version": "0.1.0"}})
        with self.assertRaisesRegex(ValueError, "private sample"):
            release.release_plan(self.root)

    def test_dev_dependency_backedge_does_not_create_publication_cycle(self):
        macro = self.crate("forklaunch-macros")
        self.crate("forklaunch-core", {"forklaunch-macros": {"path": "../forklaunch-macros", "version": "0.1.0"}})
        manifest = tomllib.loads(macro.read_text())
        manifest["dev-dependencies"] = {"forklaunch-core": {"path": "../forklaunch-core", "version": "0.1.0"}}
        macro.write_bytes(registry.manifest_text(manifest))
        with self.assertRaisesRegex(ValueError, "cycle"):
            release.release_plan(self.root)
        del manifest["dev-dependencies"]["forklaunch-core"]["version"]
        macro.write_bytes(registry.manifest_text(manifest))
        self.assertEqual([p["name"] for p in release.release_plan(self.root)],
                         ["forklaunch-macros", "forklaunch-core"])

    def test_cycle_fails(self):
        self.crate("forklaunch-a", {"forklaunch-b": {"path": "../forklaunch-b", "version": "0.1.0"}})
        self.crate("forklaunch-b", {"forklaunch-a": {"path": "../forklaunch-a", "version": "0.1.0"}})
        with self.assertRaisesRegex(ValueError, "cycle"):
            release.release_plan(self.root)

    def archive(self, manifest, extra=None):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data in {"forklaunch-a-0.1.0/Cargo.toml": registry.manifest_text(manifest),
                               "forklaunch-a-0.1.0/src/lib.rs": b"pub fn answer() -> u8 { 42 }", **(extra or {})}.items():
                member = tarfile.TarInfo(name)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        return buffer.getvalue()

    def manifest(self):
        return {"package": {"name": "forklaunch-a", "version": "0.1.0", "edition": "2024", "rust-version": "1.85"},
                "features": {"base": ["dep:owned", "owned?/extra"]},
                "dependencies": {"owned": {"package": "forklaunch-b", "version": "0.1", "optional": True},
                                 "serde": {"version": "1", "default-features": False, "features": ["derive"]}},
                "target": {"cfg(unix)": {"build-dependencies": {"cc": "1"}}}}

    def test_stage_preserves_source_and_dependency_semantics(self):
        data, index = registry.stage_package(self.archive(self.manifest()), {"name": "forklaunch-a", "version": "0.1.0"}, {"forklaunch-a", "forklaunch-b"})
        self.assertEqual(index["rust_version"], "1.85")
        self.assertEqual(index["features2"], self.manifest()["features"])
        owned, serde, cc = index["deps"]
        self.assertEqual((owned["name"], owned["package"], owned["registry"], owned["optional"]), ("owned", "forklaunch-b", None, True))
        self.assertEqual((serde["registry"], serde["default_features"]), (release.REGISTRY, False))
        self.assertEqual((cc["kind"], cc["target"]), ("build", "cfg(unix)"))
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            self.assertEqual(archive.extractfile("forklaunch-a-0.1.0/src/lib.rs").read(), b"pub fn answer() -> u8 { 42 }")
            manifest = tomllib.loads(archive.extractfile("forklaunch-a-0.1.0/Cargo.toml").read().decode())
        self.assertEqual(manifest["dependencies"]["owned"]["registry"], "forklaunch-staging")
        self.assertNotIn("registry", manifest["dependencies"]["serde"])

    def test_archive_path_and_source_rejections(self):
        for member in ("../escape", "/escape", "other-root/file"):
            with self.subTest(member=member), self.assertRaises(ValueError):
                registry.stage_package(self.archive(self.manifest(), {member: b"bad"}), {"name": "forklaunch-a", "version": "0.1.0"}, set())
        for source in ("path", "git", "registry-index", "registry"):
            manifest = self.manifest()
            manifest["dependencies"]["serde"][source] = "unexpected"
            with self.subTest(source=source), self.assertRaises(ValueError):
                registry.stage_package(self.archive(manifest), {"name": "forklaunch-a", "version": "0.1.0"}, set())

    def test_checksum_and_live_sparse_index(self):
        contents = self.archive(self.manifest())
        (self.root / "package.crate").write_bytes(contents)
        package = {"name": "forklaunch-a", "version": "0.1.0", "archive": "package.crate", "sha256": hashlib.sha256(contents).hexdigest()}
        with registry.registry(self.root, {"packages": [package]}) as index:
            base = index.removeprefix("sparse+")
            with urlopen(base + "config.json") as response:
                self.assertIn("{crate}", json.load(response)["dl"])
            with urlopen(base + "fo/rk/forklaunch-a") as response:
                entry = json.load(response)
            with urlopen(base + "api/v1/crates/forklaunch-a/0.1.0/download") as response:
                self.assertEqual(hashlib.sha256(response.read()).hexdigest(), entry["cksum"])
        with self.assertRaisesRegex(ValueError, "checksum"):
            registry.checked_archive(self.root, {**package, "sha256": "wrong"})
        with self.assertRaisesRegex(ValueError, "escapes"):
            registry.checked_archive(self.root, {**package, "archive": "../outside"})

    def test_publish_defaults_to_no_upload(self):
        report = {"packages": [{"name": "forklaunch-a", "version": "0.1.0", "manifest": "framework-rs/crates/forklaunch-a/Cargo.toml"}]}
        with patch.object(release, "publication_preflight", return_value=report), patch.object(release, "run") as run:
            release.publish(self.root, "rust-v0.1.0", False)
            run.assert_not_called()
        (self.root / "release.json").write_text(json.dumps(report))
        with patch.object(release, "publication_preflight", return_value=report), patch.dict(release.os.environ, {}, clear=True), patch.object(release, "run") as run, patch.object(release, "published_version", return_value=None), patch.object(release, "wait_for_version", return_value={}), patch.object(release, "verify_published_archive", return_value="checked"):
            release.publish(self.root, "rust-v0.1.0", True)
            run.assert_called_once()  # Cargo's credential provider resolves cargo-login credentials.
        progress = json.loads((self.root / "publication.json").read_text())
        self.assertTrue(progress["complete"])
        self.assertFalse(progress["packages"][0]["reusedMatchingVersion"])

    def test_resume_only_reuses_verified_existing_archives(self):
        report = {"packages": [{"name": "forklaunch-a", "version": "0.1.0", "manifest": "framework-rs/crates/forklaunch-a/Cargo.toml"}]}
        (self.root / "release.json").write_text(json.dumps(report))
        with patch.object(release, "publication_preflight", return_value=report), patch.object(release, "run") as run, patch.object(release, "published_version", return_value={"cksum": "checked"}), patch.object(release, "verify_published_archive", return_value="checked") as verify:
            release.publish(self.root, "rust-v0.1.0", True)
            run.assert_not_called()
            verify.assert_called_once()
        progress = json.loads((self.root / "publication.json").read_text())
        self.assertTrue(progress["packages"][0]["reusedMatchingVersion"])
        with patch.object(release, "publication_preflight", return_value=report), patch.object(release, "run") as run, patch.object(release, "published_version", return_value={}), patch.object(release, "verify_published_archive", side_effect=ValueError("different sources")):
            with self.assertRaisesRegex(ValueError, "different sources"):
                release.publish(self.root, "rust-v0.1.0", True)
            run.assert_not_called()

    def test_registry_archive_allows_only_generated_lockfile_difference(self):
        original = self.archive(self.manifest())
        (self.root / "a.crate").write_bytes(original)
        package = {"name": "forklaunch-a", "version": "0.1.0", "archive": "a.crate", "sha256": hashlib.sha256(original).hexdigest()}
        published = self.archive(self.manifest(), {"forklaunch-a-0.1.0/Cargo.lock": b"generated lock"})
        checksum = hashlib.sha256(published).hexdigest()
        with patch.object(release, "urlopen", return_value=io.BytesIO(published)):
            self.assertEqual(release.verify_published_archive(self.root, package, {"cksum": checksum}), checksum)
        changed = self.archive(self.manifest(), {"forklaunch-a-0.1.0/src/lib.rs": b"different source"})
        with patch.object(release, "urlopen", return_value=io.BytesIO(changed)):
            with self.assertRaisesRegex(ValueError, "differs from verified"):
                release.verify_published_archive(self.root, package, {"cksum": hashlib.sha256(changed).hexdigest()})
        changed_vcs = self.archive(self.manifest(), {"forklaunch-a-0.1.0/.cargo_vcs_info.json": b'{"git":{"sha1":"new"}}'})
        with patch.object(release, "urlopen", return_value=io.BytesIO(changed_vcs)):
            with self.assertRaisesRegex(ValueError, "differs from verified"):
                release.verify_published_archive(self.root, package, {"cksum": hashlib.sha256(changed_vcs).hexdigest()})

    def previous_release(self, archive, archive_name="a.crate"):
        previous = self.root / "previous"
        previous.mkdir()
        (previous / archive_name).parent.mkdir(parents=True, exist_ok=True)
        (previous / archive_name).write_bytes(archive)
        package = {"name": "forklaunch-a", "version": "0.1.0", "archive": archive_name,
                   "sha256": hashlib.sha256(archive).hexdigest(), "manifest": "framework-rs/crates/forklaunch-a/Cargo.toml", "dependencies": []}
        report = {"schemaVersion": 1, "scope": "framework", "dirty": False, "commit": "a" * 40, "packages": [package]}
        contents = json.dumps(report).encode()
        (previous / "release.json").write_bytes(contents)
        checksum = hashlib.sha256(contents).hexdigest()
        (previous / "verification.json").write_text(json.dumps({"schemaVersion": 1, "releaseSha256": checksum,
            "packages": 1, "httpMcpStartup": True}))
        (previous / "publication.json").write_text(json.dumps({"schemaVersion": 1, "scope": "framework",
            "releaseSha256": checksum, "complete": True, "packages": [{"name": "forklaunch-a", "version": "0.1.0", "registrySha256": package["sha256"]}]}))
        return previous, package

    def public_download_fixture(self, previous, package, archive):
        def download(command, **kwargs):
            self.assertEqual(command[:4], ["gh", "release", "download", "rust-v0.1.0"])
            self.assertEqual(command[command.index("--repo") + 1], "owner/repository")
            directory = Path(command[command.index("--dir") + 1])
            patterns = [command[index + 1] for index, value in enumerate(command) if value == "--pattern"]
            for pattern in patterns:
                data = archive if pattern.endswith(".crate") else (previous / pattern).read_bytes()
                (directory / pattern).write_bytes(data)
        metadata = json.dumps({"tagName": "rust-v0.1.0", "isDraft": False,
            "url": "https://github.com/owner/repository/releases/tag/rust-v0.1.0"})
        return download, [metadata, json.dumps({"sha": "a" * 40})]

    def test_hydration_restores_exact_public_assets_and_binds_tag_commit(self):
        archive = self.archive(self.manifest())
        previous, package = self.previous_release(archive, "cargo-target/package/forklaunch-a-0.1.0.crate")
        download, metadata = self.public_download_fixture(previous, package, archive)
        output = self.root / "hydrated"
        with patch.object(release, "run", side_effect=download), \
                patch.object(release.subprocess, "check_output", side_effect=metadata):
            release.hydrate_verified_release(output, "owner/repository", "rust-v0.1.0")
        self.assertEqual((output / package["archive"]).read_bytes(), archive)
        hydrated = release.verified_release_archives(output)
        self.assertEqual(hydrated["publicRelease"]["tag"], "rust-v0.1.0")
        self.assertEqual(hydrated["publicRelease"]["commit"], "a" * 40)
        next_output = self.root / "next"
        destination = next_output / package["archive"]
        destination.parent.mkdir(parents=True)
        destination.write_bytes(archive)
        release.reuse_verified_archive(next_output, package, hydrated)
        self.assertEqual(package["verifiedArchiveReuse"]["sourcePublicRelease"], hydrated["publicRelease"])
        release.validate_archive_reuse(next_output, package)

    def test_hydration_rejects_corrupt_assets_wrong_commit_and_nonpublic_release(self):
        archive = self.archive(self.manifest())
        previous, package = self.previous_release(archive, "cargo-target/package/forklaunch-a-0.1.0.crate")
        for case in ("checksum", "commit", "draft", "tag"):
            with self.subTest(case=case):
                download, metadata = self.public_download_fixture(previous, package,
                    b"tampered archive" if case == "checksum" else archive)
                if case == "commit":
                    metadata[1] = json.dumps({"sha": "b" * 40})
                elif case in ("draft", "tag"):
                    detail = json.loads(metadata[0])
                    detail["isDraft" if case == "draft" else "tagName"] = True if case == "draft" else "rust-v9.0.0"
                    metadata[0] = json.dumps(detail)
                with patch.object(release, "run", side_effect=download), \
                        patch.object(release.subprocess, "check_output", side_effect=metadata), self.assertRaises(ValueError):
                    release.hydrate_verified_release(self.root / case, "owner/repository", "rust-v0.1.0")
        for repository, tag in (("--repo", "rust-v0.1.0"), ("owner/repository", "main"), ("owner/repository", "../tag")):
            with self.subTest(repository=repository, tag=tag), patch.object(release.subprocess, "check_output") as read:
                with self.assertRaises(ValueError):
                    release.hydrate_verified_release(self.root / "invalid", repository, tag)
                read.assert_not_called()

    def test_hydration_rejects_archive_path_escape_before_downloading_package(self):
        archive = self.archive(self.manifest())
        previous, package = self.previous_release(archive)
        report = json.loads((previous / "release.json").read_text())
        report["packages"][0]["archive"] = "../forklaunch-a-0.1.0.crate"
        contents = json.dumps(report).encode()
        (previous / "release.json").write_bytes(contents)
        for name in ("verification.json", "publication.json"):
            receipt = json.loads((previous / name).read_text())
            receipt["releaseSha256"] = hashlib.sha256(contents).hexdigest()
            (previous / name).write_text(json.dumps(receipt))
        download, metadata = self.public_download_fixture(previous, package, archive)
        with patch.object(release, "run", side_effect=download) as run, \
                patch.object(release.subprocess, "check_output", side_effect=metadata), self.assertRaisesRegex(ValueError, "unsafe"):
            release.hydrate_verified_release(self.root / "hydrated", "owner/repository", "rust-v0.1.0")
        self.assertEqual(run.call_count, 1)  # Reports only; never follows the archive path.

    def test_prepare_reuses_exact_verified_archive_only_after_fresh_package_comparison(self):
        old = self.archive(self.manifest(), {"forklaunch-a-0.1.0/.cargo_vcs_info.json": b"old commit"})
        fresh = self.archive(self.manifest(), {"forklaunch-a-0.1.0/.cargo_vcs_info.json": b"new commit",
                                               "forklaunch-a-0.1.0/Cargo.lock": b"generated lock"})
        previous, prior = self.previous_release(old)
        output = self.root / "next"
        package = {key: prior[key] for key in ("name", "version", "manifest", "dependencies")}
        def package_current(command, **kwargs):
            self.assertIn("--exclude-lockfile", command)
            archive = output / "cargo-target/package/forklaunch-a-0.1.0.crate"
            archive.parent.mkdir(parents=True)
            archive.write_bytes(fresh)
        with patch.object(release, "ROOT", self.root), patch.object(release, "release_plan", return_value=[package]), \
                patch.object(release, "run", side_effect=package_current) as run, \
                patch.object(release.subprocess, "check_output", side_effect=[b"", "b" * 40]):
            release.prepare(output, False, reuse_verified_release=previous)
            run.assert_called_once()  # Reuse never skips current Cargo packaging.
        entry = json.loads((output / "release.json").read_text())["packages"][0]
        self.assertEqual((output / entry["archive"]).read_bytes(), old)
        self.assertEqual(entry["sha256"], prior["sha256"])
        proof = entry["verifiedArchiveReuse"]
        self.assertEqual((output / proof["freshArchive"]).read_bytes(), fresh)
        self.assertEqual(proof["sourceCommit"], "a" * 40)
        self.assertEqual(proof["sourceRegistrySha256"], prior["sha256"])
        self.assertEqual(proof["comparedFiles"], 2)
        release.validate_archive_reuse(output, entry)
        (output / proof["freshArchive"]).write_bytes(b"tampered comparison evidence")
        with self.assertRaisesRegex(ValueError, "checksum"):
            release.validate_archive_reuse(output, entry)

    def test_reuse_rejects_changed_files_and_requires_bound_complete_prior_receipts(self):
        old = self.archive(self.manifest(), {"forklaunch-a-0.1.0/.cargo_vcs_info.json": b"old"})
        previous, prior = self.previous_release(old)
        trusted = release.verified_release_archives(previous)
        output = self.root / "next"
        output.mkdir()
        for name, content in [("src/lib.rs", b"changed code"), ("Cargo.toml.orig", b"added original manifest"),
                              ("assets/data.json", b"added asset"), ("nested/Cargo.lock", b"nested lock is source"),
                              ("nested/.cargo_vcs_info.json", b"nested VCS file is source")]:
            with self.subTest(file=name):
                fresh = self.archive(self.manifest(), {f"forklaunch-a-0.1.0/{name}": content})
                (output / "a.crate").write_bytes(fresh)
                package = {**prior, "sha256": hashlib.sha256(fresh).hexdigest()}
                with self.assertRaisesRegex(ValueError, "bump its version"):
                    release.reuse_verified_archive(output, package, trusted)
                self.assertEqual((output / "a.crate").read_bytes(), fresh)
        for filename, changes in [("verification.json", {"releaseSha256": "wrong"}),
                                  ("publication.json", {"complete": False}),
                                  ("publication.json", {"packages": []})]:
            path = previous / filename
            original = path.read_text()
            path.write_text(json.dumps({**json.loads(original), **changes}))
            with self.subTest(receipt=filename, changes=changes), self.assertRaises(ValueError):
                release.verified_release_archives(previous)
            path.write_text(original)
        (previous / prior["archive"]).write_bytes(b"tampered prior archive")
        with self.assertRaisesRegex(ValueError, "checksum"):
            release.reuse_verified_archive(output, prior, trusted)

    def test_package_comparison_rejects_unsafe_members_and_compares_modes(self):
        package = {"name": "forklaunch-a", "version": "0.1.0"}
        for member in ("../escape", "other-root/file"):
            with self.subTest(member=member), self.assertRaises(ValueError):
                release.comparable_package_files(self.archive(self.manifest(), {member: b"bad"}), package)
        original = self.archive(self.manifest())
        buffer = io.BytesIO()
        with tarfile.open(fileobj=io.BytesIO(original)) as source, tarfile.open(fileobj=buffer, mode="w:gz") as target:
            for member in source.getmembers():
                content = source.extractfile(member).read()
                if member.name.endswith("src/lib.rs"):
                    member.mode = 0o755
                target.addfile(member, io.BytesIO(content))
        self.assertNotEqual(release.comparable_package_files(original, package),
                            release.comparable_package_files(buffer.getvalue(), package))

    def test_registry_wait_is_bounded(self):
        with patch.object(release, "published_version", return_value=None), patch.object(release.time, "monotonic", side_effect=[0, 181]):
            with self.assertRaisesRegex(ValueError, "rerun to resume"):
                release.wait_for_version({"name": "forklaunch-a", "version": "0.1.0"})

    def test_dirty_or_unverified_release_cannot_publish(self):
        for report in ({"schemaVersion": 1, "dirty": True}, {"schemaVersion": 1, "dirty": False, "packages": []}):
            (self.root / "release.json").write_text(json.dumps(report))
            (self.root / "verification.json").write_text("{}")
            with self.assertRaises(ValueError):
                release.publication_preflight(self.root, "rust-v0.1.0")

    def test_blueprint_release_requires_public_framework_install_proof(self):
        report = {"schemaVersion": 1, "dirty": False, "scope": "blueprint", "packages": []}
        contents = json.dumps(report).encode()
        (self.root / "release.json").write_bytes(contents)
        for proof in ({"httpMcpStartup": True}, {"allFeatures": True, "frameworkFromCratesIo": False},
                      {"allFeatures": False, "frameworkFromCratesIo": True}):
            with self.subTest(proof=proof):
                (self.root / "verification.json").write_text(json.dumps({
                    "schemaVersion": 1, "packages": 0,
                    "releaseSha256": hashlib.sha256(contents).hexdigest(), **proof,
                }))
                with self.assertRaisesRegex(ValueError, "registry/startup verification"):
                    release.publication_preflight(self.root, "rust-blueprint-v0.1.0")
        (self.root / "verification.json").write_text(json.dumps({
            "schemaVersion": 1, "packages": 0, "allFeatures": True, "frameworkFromCratesIo": True,
            "releaseSha256": hashlib.sha256(contents).hexdigest(),
        }))
        # The receipt passes; the next check is the package inventory.
        with self.assertRaises(ValueError) as raised:
            release.publication_preflight(self.root, "rust-blueprint-v0.1.0")
        self.assertNotIn("registry/startup verification", str(raised.exception))

    def test_blueprint_worker_release_requires_native_worker_proof(self):
        report = {"schemaVersion": 1, "dirty": False, "scope": "blueprint",
                  "packages": [{"name": "forklaunch-worker-interface", "manifest": "elsewhere/Cargo.toml"}]}
        contents = json.dumps(report).encode()
        (self.root / "release.json").write_bytes(contents)
        for proof in (None, False, True):
            with self.subTest(proof=proof):
                (self.root / "verification.json").write_text(json.dumps({
                    "schemaVersion": 1, "packages": 1, "allFeatures": True, "frameworkFromCratesIo": True,
                    "releaseSha256": hashlib.sha256(contents).hexdigest(),
                    "nativeHttpWorkerFeatures": proof,
                }))
                with self.assertRaises(ValueError) as raised:
                    release.publication_preflight(self.root, "rust-blueprint-v0.1.0")
                if proof is True:
                    self.assertNotIn("native HTTP/worker", str(raised.exception))
                else:
                    self.assertIn("native HTTP/worker", str(raised.exception))

    def test_blueprint_inventory_check_includes_framework_pins(self):
        package = {"name": "forklaunch-iam-interface", "version": "0.1.0", "dependencies": [],
                   "manifest": "blueprint-rs/crates/interfaces/iam-interface/Cargo.toml",
                   "externalDependencies": ["forklaunch-core@0.1.2"]}
        report = {"schemaVersion": 1, "dirty": False, "scope": "blueprint", "packages": [package]}
        contents = json.dumps(report).encode()
        (self.root / "release.json").write_bytes(contents)
        (self.root / "verification.json").write_text(json.dumps({
            "schemaVersion": 1, "packages": 1, "allFeatures": True, "frameworkFromCratesIo": True,
            "releaseSha256": hashlib.sha256(contents).hexdigest(),
        }))
        for pins, matches in ((["forklaunch-core@0.1.2"], True), (["forklaunch-core@0.1.1"], False)):
            with (self.subTest(pins=pins),
                  patch.object(release, "release_plan", return_value=[{**package, "externalDependencies": pins}]),
                  patch.object(release, "published_version", return_value={}),
                  patch.object(release.subprocess, "check_output", side_effect=ValueError("checkout"))):
                with self.assertRaises(ValueError) as raised:
                    release.publication_preflight(self.root, "rust-blueprint-v0.1.0")
                (self.assertNotIn if matches else self.assertIn)("differs from checkout", str(raised.exception))

    def test_regex_release_requires_optional_feature_proof(self):
        report = {"schemaVersion": 1, "dirty": False,
                  "packages": [{"name": "forklaunch-regress"}]}
        contents = json.dumps(report).encode()
        (self.root / "release.json").write_bytes(contents)
        for proof in (None, False, "true"):
            with self.subTest(proof=proof):
                (self.root / "verification.json").write_text(json.dumps({
                    "schemaVersion": 1, "httpMcpStartup": True, "packages": 1,
                    "releaseSha256": hashlib.sha256(contents).hexdigest(),
                    "boundedRegex": proof,
                }))
                with self.assertRaisesRegex(ValueError, "bounded regex"):
                    release.publication_preflight(self.root, "rust-v0.1.0")

    def test_native_worker_release_requires_optional_feature_proof(self):
        report = {"schemaVersion": 1, "dirty": False,
                  "packages": [{"name": "forklaunch-worker-interface"}]}
        contents = json.dumps(report).encode()
        (self.root / "release.json").write_bytes(contents)
        for proof in (None, False, "true"):
            with self.subTest(proof=proof):
                (self.root / "verification.json").write_text(json.dumps({
                    "schemaVersion": 1, "httpMcpStartup": True, "packages": 1,
                    "releaseSha256": hashlib.sha256(contents).hexdigest(),
                    "nativeHttpWorkerFeatures": proof,
                }))
                with self.assertRaisesRegex(ValueError, "native HTTP/worker"):
                    release.publication_preflight(self.root, "rust-v0.1.0")

    def test_patch_release_requires_installed_sdk_discovery_proof(self):
        report = {"schemaVersion": 1, "dirty": False,
                  "packages": [{"name": "forklaunch-core", "version": "0.1.1"}]}
        contents = json.dumps(report).encode()
        (self.root / "release.json").write_bytes(contents)
        for proof in (None, False, "true"):
            with self.subTest(proof=proof):
                (self.root / "verification.json").write_text(json.dumps({
                    "schemaVersion": 1, "httpMcpStartup": True, "packages": 1,
                    "releaseSha256": hashlib.sha256(contents).hexdigest(),
                    "installedSdkDiscovery": proof,
                }))
                with self.assertRaisesRegex(ValueError, "installed SDK discovery"):
                    release.publication_preflight(self.root, "rust-v0.1.1")

    def test_publication_binds_clean_tag_graph_and_artifacts(self):
        self.crate("forklaunch-a")
        def git(*args):
            return subprocess.check_output(["git", *args], cwd=self.root, stderr=subprocess.DEVNULL, text=True).strip()
        git("init")
        git("add", ".")
        git("-c", "user.name=Release Fixture", "-c", "user.email=release@example.test", "commit", "-m", "fixture")
        commit = git("rev-parse", "HEAD")
        git("tag", "rust-v0.1.0")
        with tempfile.TemporaryDirectory() as artifacts:
            output = Path(artifacts)
            (output / "package.crate").write_bytes(b"fixture archive")
            artifact = {"archive": "package.crate", "sha256": hashlib.sha256(b"fixture archive").hexdigest()}
            report = {"schemaVersion": 1, "scope": "framework", "commit": commit, "dirty": False,
                      "packages": [{**release.release_plan(self.root)[0], **artifact}]}
            def save():
                contents = json.dumps(report).encode()
                (output / "release.json").write_bytes(contents)
                (output / "verification.json").write_text(json.dumps({"schemaVersion": 1, "httpMcpStartup": True,
                    "releaseSha256": hashlib.sha256(contents).hexdigest(), "packages": 1}))
            save()
            with patch.object(release, "ROOT", self.root), patch.object(release, "release_plan", side_effect=lambda **kwargs: original_plan(self.root, **kwargs)):
                self.assertEqual(release.publication_preflight(output, "rust-v0.1.0"), report)
                report["scope"] = "all"
                save()
                with self.assertRaisesRegex(ValueError, "exactly one scope"):
                    release.publication_preflight(output, "rust-v0.1.0")
                report["scope"] = "framework"
                report["packages"][0]["manifest"] = "blueprint-rs/crates/forklaunch-a/Cargo.toml"
                save()
                with self.assertRaisesRegex(ValueError, "exactly one scope"):
                    release.publication_preflight(output, "rust-v0.1.0")
                report["packages"][0]["manifest"] = "framework-rs/crates/forklaunch-a/Cargo.toml"
                save()
                with self.assertRaisesRegex(ValueError, "tag"):
                    release.publication_preflight(output, "rust-v9.0.0")
                (self.root / "untracked").write_text("dirty")
                with self.assertRaisesRegex(ValueError, "clean checkout"):
                    release.publication_preflight(output, "rust-v0.1.0")
                (self.root / "untracked").unlink()
                report["commit"] = "0" * 40
                save()
                with self.assertRaisesRegex(ValueError, "same commit"):
                    release.publication_preflight(output, "rust-v0.1.0")
                report["commit"] = commit
                report["packages"][0]["version"] = "9.0.0"
                save()
                with self.assertRaisesRegex(ValueError, "inventory"):
                    release.publication_preflight(output, "rust-v0.1.0")
                report["packages"][0]["version"] = "0.1.0"
                save()
                (output / "package.crate").write_bytes(b"tampered")
                with self.assertRaisesRegex(ValueError, "checksum"):
                    release.publication_preflight(output, "rust-v0.1.0")


original_plan = release.release_plan


if __name__ == "__main__":
    unittest.main()
