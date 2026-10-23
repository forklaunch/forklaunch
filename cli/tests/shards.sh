#!/usr/bin/env bash
# Shard entrypoints for the end-to-end scripts in cli/tests.
#
#   bash cli/tests/shards.sh list                 # shards and their scripts
#   bash cli/tests/shards.sh run SHARD [SHARD..]  # run them, report timings
#   bash cli/tests/shards.sh check                # every CI script is in exactly one shard
#
# Every script scaffolds the applications it tests.
#
# Scripts run from cli/tests. Every script in a shard runs even after a failure;
# the shard fails if any did. FORKLAUNCH_SHARD_FAIL_FAST=1 stops at the first.
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

shard_scripts() {
  case "$1" in
    # TypeScript: scaffold, install and build applications and their projects.
    ts-build) echo init_compile.sh init_service.sh init_worker.sh init_library.sh init_router.sh depcheck.sh eject.sh ;;
    # TypeScript command tests: change, delete and sync on small fresh apps.
    ts-change) echo change_application.sh change_library.sh change_router.sh change_router_add_mappers.sh change_sdk_mode.sh change_service.sh change_service_worker_migration.sh change_worker.sh ts_change_round_trips.sh ;;
    ts-delete-sync) echo delete_library.sh delete_router.sh delete_service.sh delete_worker.sh sync_add_remove_flow.sh sync_application_add.sh sync_application_remove.sh sync_library_subcommand.sh sync_service_subcommand.sh sync_worker_subcommand.sh ;;
    ts-init) echo dryrun.sh existing_docker_compose.sh init_application.sh init_application_custom_path.sh init_billing_stripe.sh init_cac.sh init_iam_better_auth.sh init_messaging_twilio.sh init_module.sh init_module_src_path.sh init_relay.sh init_relay_drift.sh init_service_mappers.sh init_worker_mappers.sh init_worker_workspace_and_typecheck.sh ;;
    ts-env-release) echo environment_sync.sh environment_validate.sh precheck_version_match_allows_run.sh precheck_version_mismatch_decline_fails.sh release_non_tty.sh ;;
    # Rust and mixed scaffolding and code-editing commands.
    rust-commands) echo rust_init_commands.sh init_rust_projects.sh rust_sync_commands.sh rust_delete_commands.sh rust_change_commands.sh rust_conversion_preservation.sh rust_package_sources.sh rust_infrastructure_sources.sh ;;
    rust-sdk) echo rust_sdk_openapi_commands.sh rust_sdk_workspace.sh rust_bootstrap_lifecycle.sh rust_checked_mapper.sh rust_local_sdk.sh rust_static_router_lifecycle.sh ;;
    # The change/delete sweep reports a result per command case (see
    # fixtures/rust_command_sweep.py), in two halves that run in parallel.
    rust-sweep-change) echo rust_command_sweep_change.sh rust_name_prompt.sh ;;
    rust-sweep-delete) echo rust_command_sweep_delete.sh rust_generated_projects.sh ;;
    mixed-tooling) echo mixed_application_tooling.sh mixed_runtime_migration.sh neutral_framework_dependencies.sh raw_mixed_workspace.sh ;;
    rust-mixed) echo rust_mixed_sync.sh rust_typescript_attach.sh rust_mixed_attach.sh rust_mixed_lifecycle.sh ;;
    rust-modules) echo rust_module_lifecycle.sh ;;
    # Slow: Docker images of generated apps, and a build against the
    # published framework crates. Full runs only.
    rust-images) echo rust_container_runtime.sh rust_registry_scaffold.sh ;;
    *) return 1 ;;
  esac
}
SHARDS="ts-build ts-change ts-delete-sync ts-init ts-env-release rust-commands rust-sdk rust-sweep-change rust-sweep-delete rust-mixed mixed-tooling rust-modules rust-images"

run_one() {
  local script="$1" started status
  started=$(date +%s)
  echo "[shard] >>> $script"
  case "$script" in
    *.py) (cd "$REPO_ROOT" && python3 "cli/tests/$script") ;;
    *) (cd "$SCRIPT_DIR" && bash "$script") ;;
  esac
  status=$?
  echo "[shard] <<< $script: $([ $status = 0 ] && echo PASS || echo "FAIL ($status)") in $(( $(date +%s) - started ))s"
  return $status
}

command="${1:-list}"; shift || true
case "$command" in
  list)
    for shard in $SHARDS; do echo "$shard: $(shard_scripts "$shard")"; done ;;
  check)
    # Each script named by a shard exists, and appears in only one shard.
    seen=$(for shard in $SHARDS; do shard_scripts "$shard" | tr ' ' '\n'; done)
    dupes=$(echo "$seen" | sort | uniq -d)
    missing=$(for s in $seen; do [ -f "$SCRIPT_DIR/$s" ] || echo "$s"; done)
    [ -z "$dupes$missing" ] || { echo "duplicated: $dupes"; echo "missing: $missing"; exit 1; }
    echo "$(echo "$seen" | wc -l | tr -d ' ') scripts in $(echo $SHARDS | wc -w | tr -d ' ') shards" ;;
  run)
    [ $# -gt 0 ] || { echo "usage: shards.sh run SHARD..." >&2; exit 2; }
    failed=() total=0 started=$(date +%s) rows=
    for shard in "$@"; do
      scripts=$(shard_scripts "$shard") || { echo "unknown shard $shard; known: $SHARDS" >&2; exit 2; }
      for script in $scripts; do
        total=$((total + 1))
        script_started=$(date +%s)
        if run_one "$script"; then
          rows+="| $shard/$script | ✅ | $(( $(date +%s) - script_started ))s |"$'\n'
        else
          rows+="| $shard/$script | ❌ | $(( $(date +%s) - script_started ))s |"$'\n'
          failed+=("$shard/$script")
          [ "${FORKLAUNCH_SHARD_FAIL_FAST:-0}" = 1 ] && break 2
        fi
      done
    done
    echo "[shard] $((total - ${#failed[@]}))/$total passed in $(( $(date +%s) - started ))s"
    for f in ${failed[@]+"${failed[@]}"}; do
      echo "[shard] FAIL $f"
      echo "::error title=${f##*/} failed::$f failed; see the job log for its output"
    done
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
      {
        echo "### Shards $*: $((total - ${#failed[@]}))/$total scripts passed"
        echo
        echo "| Script | Result | Time |"
        echo "| --- | --- | --- |"
        printf '%s' "$rows"
      } >> "$GITHUB_STEP_SUMMARY"
    fi
    [ ${#failed[@]} = 0 ] ;;
  *) echo "usage: shards.sh list | check | run SHARD..." >&2; exit 2 ;;
esac
