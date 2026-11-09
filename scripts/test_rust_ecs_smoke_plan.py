#!/usr/bin/env python3
"""Offline request validation. Run: python3 -m unittest discover -s scripts -p test_rust_ecs_smoke_plan.py"""
import copy
import json
import tempfile
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

from rust_ecs_smoke_plan import EXAMPLE, PLACEHOLDER, generate, reject_duplicate_keys, write_plan


class SmokePlanTests(unittest.TestCase):
    def config(self, **changes):
        return copy.deepcopy(EXAMPLE) | changes

    def test_platform_runtime_contract(self):
        task, service = generate(self.config())
        self.assertEqual(task["runtimePlatform"], {"cpuArchitecture": "ARM64", "operatingSystemFamily": "LINUX"})
        self.assertEqual(task["networkMode"], "awsvpc")
        self.assertEqual(task["requiresCompatibilities"], ["FARGATE"])
        container = task["containerDefinitions"][0]
        self.assertEqual(container["entryPoint"], [])
        self.assertEqual(container["command"], ["/app/target/release/api"])
        self.assertEqual(container["healthCheck"], {
            "command": ["CMD-SHELL", "curl -f http://127.0.0.1:3000/health || exit 1"],
            "interval": 30, "retries": 5, "startPeriod": 120, "timeout": 10})
        self.assertEqual(container["logConfiguration"]["options"]["awslogs-region"], EXAMPLE["region"])
        self.assertNotIn("taskRoleArn", task)
        self.assertNotIn("secrets", container)
        self.assertEqual(service["taskDefinition"], PLACEHOLDER)
        self.assertEqual(service["desiredCount"], 1)
        self.assertEqual(service["networkConfiguration"]["awsvpcConfiguration"]["assignPublicIp"], "DISABLED")

    def test_exact_revision_binding(self):
        arn = "arn:aws:ecs:us-west-2:123456789012:task-definition/fl-rust-smoke-example:12"
        _, service = generate(self.config(registered_task_definition_arn=arn))
        self.assertEqual(service["taskDefinition"], arn)
        for invalid in [arn.replace(":12", ":0"), arn.replace(":12", ""), arn.replace("example:12", "other:12"),
                        arn.replace("us-west-2", "us-east-1"), arn.replace("123456789012", "234567890123"), "latest"]:
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                generate(self.config(registered_task_definition_arn=invalid))

    def test_rejects_account_region_and_unpinned_images(self):
        for change in [
            {"image": EXAMPLE["image"].split("@")[0] + ":latest"},
            {"image": EXAMPLE["image"].replace("123456789012", "234567890123")},
            {"image": EXAMPLE["image"].replace("us-west-2", "us-east-1")},
            {"image": EXAMPLE["image"] + "\n"}, {"image": "nginx@sha256:" + "a" * 64},
            {"image": EXAMPLE["image"].replace("sha256:", "sha512:")},
            {"cluster_arn": EXAMPLE["cluster_arn"].replace("us-west-2", "us-east-1")},
            {"cluster_arn": "dedicated-rust-smoke"},
            {"execution_role_arn": EXAMPLE["execution_role_arn"].replace("123456789012", "234567890123")},
            {"execution_role_arn": EXAMPLE["execution_role_arn"].replace(":role/", ":user/")},
            {"region": "us-gov-west-1"}, {"region": "us-west-2; false"},
        ]:
            with self.subTest(change=change), self.assertRaises(ValueError):
                generate(self.config(**change))

    def test_rejects_injection_and_unexpected_inputs(self):
        changes = [
            {"binary": "api;whoami"}, {"binary": "../api"}, {"binary": "$(id)"},
            {"binary": "api\n"}, {"binary": "api --migrate"}, {"binary": "-api"},
            {"name": "production"}, {"name": "fl-rust-smoke-a/b"}, {"log_group": "bad:name"},
            {"log_group": "aws/reserved"}, {"log_group": "bad\n"},
            {"port": True}, {"port": 80}, {"port": 65536}, {"port": "3000"},
            {"assign_public_ip": True}, {"assign_public_ip": "yes"}, {"environment": {"SECRET": "x"}},
        ]
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                generate(self.config(**change))
        for key in EXAMPLE:
            config = self.config()
            del config[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                generate(config)
        for invalid in [None, [], "", 1]:
            with self.subTest(value=invalid), self.assertRaises(ValueError):
                generate(invalid)

    def test_network_fields_fail_closed(self):
        for key, values in {
            "subnets": [[], "subnet-01234567", ["subnet-01234567"] * 2, ["sg-01234567"], [None],
                        ["subnet-" + f"{n:017x}" for n in range(17)]],
            "security_groups": [[], ["sg-anything"], ["sg-01234567"] * 2, ["subnet-01234567"],
                                ["sg-" + f"{n:017x}" for n in range(6)]],
        }.items():
            for value in values:
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    generate(self.config(**{key: value}))
        _, service = generate(self.config(subnets=["subnet-01234567"], security_groups=["sg-01234567"], assign_public_ip="ENABLED"))
        self.assertEqual(service["networkConfiguration"]["awsvpcConfiguration"]["assignPublicIp"], "ENABLED")

    def test_duplicate_json_fields_rejected(self):
        with self.assertRaises(ValueError):
            json.loads('{"region":"us-west-2","region":"us-east-1"}', object_pairs_hook=reject_duplicate_keys)

    def test_cli_roundtrip_and_invalid_input(self):
        script = Path(__file__).with_name("rust_ecs_smoke_plan.py")
        example = subprocess.run([sys.executable, str(script), "--example"], capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(example.stdout), EXAMPLE)
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "config.json"
            config.write_text(example.stdout)
            output = Path(directory) / "review"
            argv = [sys.executable, str(script), "--config", str(config), "--output", str(output)]
            result = subprocess.run(argv, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Offline review artifacts:", result.stdout)
            config.write_text('{"region":"us-west-2","region":"us-east-1"}')
            result = subprocess.run(argv, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("Duplicate JSON", result.stderr)
            self.assertNotIn("Traceback", result.stderr)

    def test_artifacts_do_not_execute_or_overwrite(self):
        with tempfile.TemporaryDirectory() as directory, patch("subprocess.run", side_effect=AssertionError("must not execute")), patch("socket.create_connection", side_effect=AssertionError("must not connect")):
            output = Path(directory) / "plan"
            write_plan(self.config(), output)
            self.assertEqual(set(p.name for p in output.iterdir()), {"task-definition.json", "service-request.template.json", "REVIEW.txt"})
            self.assertEqual(json.loads((output / "task-definition.json").read_text()), generate(self.config())[0])
            self.assertIn("no AWS calls", (output / "REVIEW.txt").read_text())
            with self.assertRaises(FileExistsError):
                write_plan(self.config(), output)
            link = Path(directory) / "link"
            link.symlink_to(output, target_is_directory=True)
            with self.assertRaises(FileExistsError):
                write_plan(self.config(), link)
            invalid = Path(directory) / "invalid"
            with self.assertRaises(ValueError):
                write_plan(self.config(binary="../oops"), invalid)
            self.assertFalse(invalid.exists())
            bound = Path(directory) / "bound"
            arn = "arn:aws:ecs:us-west-2:123456789012:task-definition/fl-rust-smoke-example:12"
            write_plan(self.config(registered_task_definition_arn=arn), bound)
            self.assertTrue((bound / "service-request.json").exists())
            self.assertFalse((bound / "service-request.template.json").exists())


if __name__ == "__main__":
    unittest.main()
