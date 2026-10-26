#!/usr/bin/env python3
"""Prepare reviewable ECS requests offline. Never contacts AWS or runs commands.

Usage: python3 scripts/rust_ecs_smoke_plan.py --config target.json --output plan-dir
A supplied registered_task_definition_arn binds the service to an exact revision;
otherwise service-request.template.json deliberately contains an invalid placeholder.
The output directory must not exist. See --example for the complete input schema.
"""
import argparse
import json
import re
from pathlib import Path

PLACEHOLDER = "REPLACE_WITH_EXACT_REGISTERED_TASK_DEFINITION_ARN"
EXAMPLE = {
    "name": "fl-rust-smoke-example",
    "region": "us-west-2",
    "image": "123456789012.dkr.ecr.us-west-2.amazonaws.com/rust-smoke@sha256:" + "a" * 64,
    "execution_role_arn": "arn:aws:iam::123456789012:role/rust-smoke-execution",
    "log_group": "/forklaunch/rust-smoke",
    "cluster_arn": "arn:aws:ecs:us-west-2:123456789012:cluster/dedicated-rust-smoke",
    "subnets": ["subnet-0123456789abcdef0"],
    "security_groups": ["sg-0123456789abcdef0"],
    "assign_public_ip": "DISABLED",
    "binary": "api",
    "port": 3000,
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def matches(pattern, value):
    return isinstance(value, str) and re.fullmatch(pattern, value) is not None


def ids(value, prefix, maximum):
    require(isinstance(value, list) and 1 <= len(value) <= maximum,
            f"{prefix} requires 1..{maximum} identifiers")
    require(all(matches(prefix + r"-(?:[0-9a-f]{8}|[0-9a-f]{17})", item) for item in value),
            f"Invalid {prefix} identifier")
    require(len(set(value)) == len(value), f"Duplicate {prefix} identifier")
    return value.copy()


def generate(config):
    """Validate a target and return AWS request objects without any side effects."""
    require(isinstance(config, dict), "Configuration must be an object")
    required = set(EXAMPLE)
    optional = {"registered_task_definition_arn"}
    require(required <= config.keys(), "Missing fields: " + ", ".join(sorted(required - config.keys())))
    require(config.keys() <= required | optional, "Unknown configuration fields")
    name, region = config["name"], config["region"]
    require(matches(r"fl-rust-smoke-[a-z0-9][a-z0-9-]{0,79}", name), "Name must use fl-rust-smoke- prefix and lowercase letters, digits or hyphens")
    require(matches(r"(?:us|eu|ap|sa|ca|me|af|il|mx)-(?:[a-z]+-)+[0-9]+", region), "Invalid commercial AWS region")
    # This deliberately supports only commercial AWS, rather than incorrectly
    # constructing GovCloud/China endpoints or ARNs from a commercial template.
    require(not region.startswith("us-gov-"), "Only commercial AWS partitions are supported")
    cluster = config["cluster_arn"]
    match = re.fullmatch(r"arn:aws:ecs:([^:]+):([0-9]{12}):cluster/([A-Za-z0-9_-]{1,255})", cluster) if isinstance(cluster, str) else None
    require(match is not None, "Cluster must be a full commercial ECS cluster ARN")
    require(match[1] == region, "Cluster region does not match requested region")
    account = match[2]
    require(matches(r"arn:aws:iam::" + account + r":role/(?:[A-Za-z0-9+=,.@_-]+/)*[A-Za-z0-9+=,.@_-]{1,64}", config["execution_role_arn"]),
            "Execution role must be an IAM role in the cluster account")
    image = config["image"]
    # ECR-only keeps account/region selection unambiguous and uses the supplied
    # execution role for pulls; cross-account repositories require a separate plan.
    require(matches(account + r"\.dkr\.ecr\." + re.escape(region) + r"\.amazonaws\.com/[a-z0-9]+(?:[._/-][a-z0-9]+)*@sha256:[0-9a-f]{64}", image),
            "Image must be digest-pinned ECR in the cluster account and region")
    require(matches(r"[A-Za-z_][A-Za-z0-9_-]{0,127}", config["binary"]), "Invalid native binary name")
    require(matches(r"[A-Za-z0-9_./#-]{1,512}", config["log_group"]) and not config["log_group"].startswith("aws/"), "Invalid pre-existing log group name")
    port = config["port"]
    require(type(port) is int and 1024 <= port <= 65535, "Port must be an unprivileged integer 1024..65535")
    require(config["assign_public_ip"] in ("ENABLED", "DISABLED"), "assign_public_ip must explicitly be ENABLED or DISABLED")
    subnets = ids(config["subnets"], "subnet", 16)
    groups = ids(config["security_groups"], "sg", 5)
    task_arn = config.get("registered_task_definition_arn", PLACEHOLDER)
    if "registered_task_definition_arn" in config:
        require(matches(r"arn:aws:ecs:" + re.escape(region) + ":" + account + ":task-definition/" + re.escape(name) + r":[1-9][0-9]*", task_arn),
                "Task definition must be an exact revision ARN in the target account/region and smoke family")
    tags = [{"key": "ManagedBy", "value": "ForkLaunchRustSmoke"}, {"key": "SmokeRun", "value": name}]
    task = {
        "family": name, "networkMode": "awsvpc", "requiresCompatibilities": ["FARGATE"],
        "cpu": "256", "memory": "512",
        "runtimePlatform": {"cpuArchitecture": "ARM64", "operatingSystemFamily": "LINUX"},
        "executionRoleArn": config["execution_role_arn"], "tags": tags,
        "containerDefinitions": [{
            "name": name, "image": image, "essential": True,
            "entryPoint": [], "command": ["/app/target/release/" + config["binary"]],
            "portMappings": [{"containerPort": port, "hostPort": port, "protocol": "tcp"}],
            "environment": [{"name": "HOST", "value": "0.0.0.0"}, {"name": "PORT", "value": str(port)}],
            "healthCheck": {"command": ["CMD-SHELL", f"curl -f http://127.0.0.1:{port}/health || exit 1"],
                            "interval": 30, "retries": 5, "startPeriod": 120, "timeout": 10},
            "logConfiguration": {"logDriver": "awslogs", "options": {
                "awslogs-group": config["log_group"], "awslogs-region": region, "awslogs-stream-prefix": name}},
        }],
    }
    service = {
        "cluster": cluster, "serviceName": name, "taskDefinition": task_arn,
        "desiredCount": 1, "launchType": "FARGATE", "platformVersion": "1.4.0",
        "networkConfiguration": {"awsvpcConfiguration": {"subnets": subnets, "securityGroups": groups,
                                                           "assignPublicIp": config["assign_public_ip"]}},
        "deploymentConfiguration": {"deploymentCircuitBreaker": {"enable": True, "rollback": False},
                                    "maximumPercent": 100, "minimumHealthyPercent": 0},
        "enableECSManagedTags": True, "propagateTags": "SERVICE", "tags": tags,
    }
    return task, service


def reject_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate JSON configuration key: " + key)
        result[key] = value
    return result


def write_plan(config, output):
    task, service = generate(config)
    output = Path(output)
    # Refuse collisions, including symlinks, instead of replacing a prior review.
    output.mkdir(parents=False, exist_ok=False)
    service_name = "service-request.json" if service["taskDefinition"] != PLACEHOLDER else "service-request.template.json"
    for name, value in (("task-definition.json", task), (service_name, service)):
        (output / name).write_text(json.dumps(value, indent=2) + "\n")
    (output / "REVIEW.txt").write_text(
        "OFFLINE PLAN ONLY: no AWS calls were made. This is not ECS runtime validation.\n"
        "The named cluster, subnets, groups, execution role and log group must already exist.\n"
        "Verify the cluster is dedicated to this smoke run and that all subnets/groups share its intended VPC.\n"
        "Verify account/region, ECR image ARM64 architecture, execution-role pull/log permissions,\n"
        "network reachability to ECR/CloudWatch, and existing log group before any cloud action.\n"
        "This minimal service has no database, secrets, task IAM role, load balancer or auto-expiry.\n"
        "Use a dependency-free generated service image containing the named binary and curl.\n"
        "After approved registration, use its exact task-definition ARN in registered_task_definition_arn\n"
        "and regenerate into a new directory. Verify that registered revision matches this task JSON.\n"
        "Do not submit the service template placeholder or resolve an unpinned latest family revision.\n"
        "A service remains billable until explicitly deleted: after evidence capture, scale to zero,\n"
        "delete only this named service, wait for its tasks to stop, and deregister only its exact revision.\n"
        "Do not delete the supplied shared resources. Health alone does not prove HTTP/MCP parity;\n"
        "capture task health/logs and explicit HTTP/MCP probes through approved network access.\n"
    )
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--example", action="store_true")
    args = parser.parse_args()
    if args.example:
        print(json.dumps(EXAMPLE, indent=2))
        return
    if args.config is None or args.output is None:
        parser.error("--config and --output are required")
    try:
        config = json.loads(args.config.read_text(), object_pairs_hook=reject_duplicate_keys)
        result = write_plan(config, args.output)
    except (ValueError, OSError) as error:
        parser.exit(2, f"Invalid smoke plan: {error}\n")
    print(f"Offline review artifacts: {result}")


if __name__ == "__main__":
    main()
