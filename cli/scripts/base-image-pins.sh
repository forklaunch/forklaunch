#!/usr/bin/env bash
# Print (or, with --write, apply) the pins of the ForkLaunch base images that
# generated Dockerfiles use: the *_IMAGE constants in cli/src/constants.rs.
#
# The images are built nightly by github.com/forklaunch/base-images and
# published to public ECR only at 0 scan findings. Each pin is
# `<repository>:<tag>@<multi-arch index digest>`: the digest is what Docker
# pulls, the tag documents it and is always a versioned tag (never `latest`).
# Published digests are kept, so older pins keep resolving.
#
# Uses the registry's anonymous pull API; no AWS credentials are needed.
#
# Usage: cli/scripts/base-image-pins.sh [--write]
#   Reads NODE_MAJOR and BUN_VERSION from constants.rs. Run `cargo test` in
#   cli/ after --write.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
constants="$here/../src/constants.rs"
write=false
[ "${1:-}" = "--write" ] && write=true

constant() {
  sed -n "s/^pub(crate) const $1: &str = \"\(.*\)\";/\1/p" "$constants"
}
registry="$(constant BASE_IMAGE_REGISTRY)"
node_major="$(constant NODE_MAJOR)"
bun_version="$(constant BUN_VERSION)"
host="${registry%%/*}"
namespace="${registry#*/}"

# Public ECR rate-limits anonymous pulls (429); curl retries those.
fetch() { curl -fsSL --retry 6 --retry-delay 3 "$@"; }

# One anonymous pull token per repository, fetched once (bash 3 has no
# associative arrays, and command substitutions cannot fill a cache).
pull_token() {
  fetch "https://$host/token/?scope=repository:$namespace/$1:pull" |
    sed -n 's/.*"token":"\([^"]*\)".*/\1/p'
}
token_node="$(pull_token node)"
token_bun="$(pull_token bun)"
token_rust="$(pull_token rust)"
token() {
  case "$1" in
    node) echo "$token_node" ;;
    bun) echo "$token_bun" ;;
    rust) echo "$token_rust" ;;
  esac
}

# Index digest of <repository>:<tag>.
digest() {
  fetch -I \
    -H "Authorization: Bearer $(token "$1")" \
    -H 'Accept: application/vnd.oci.image.index.v1+json' \
    -H 'Accept: application/vnd.docker.distribution.manifest.list.v2+json' \
    "https://$host/v2/$namespace/$1/manifests/$2" |
    tr -d '\r' | awk -F': ' 'tolower($1)=="docker-content-digest"{print $2}'
}

# The rust runtime image is tagged only `latest` and YYYYMMDD: name the newest
# date tag that points at the same digest as `latest`.
rust_runtime_tag() {
  local latest tags tag
  latest="$(digest rust latest)"
  tags="$(fetch -H "Authorization: Bearer $(token rust)" \
    "https://$host/v2/$namespace/rust/tags/list" |
    grep -o '"[0-9]\{8\}"' | tr -d '"' | sort -r)"
  for tag in $tags; do
    if [ "$(digest rust "$tag")" = "$latest" ]; then
      echo "$tag"
      return
    fi
  done
  echo "rust:latest ($latest) has no date tag" >&2
  return 1
}

# The rust dev image's toolchain tag (for example 1.100.0-dev) at `dev`.
rust_dev_tag() {
  local dev tags tag
  dev="$(digest rust dev)"
  tags="$(fetch -H "Authorization: Bearer $(token rust)" \
    "https://$host/v2/$namespace/rust/tags/list" |
    grep -o '"[0-9]*\.[0-9]*\.[0-9]*-dev"' | tr -d '"' | sort -rV)"
  for tag in $tags; do
    if [ "$(digest rust "$tag")" = "$dev" ]; then
      echo "$tag"
      return
    fi
  done
  echo "rust:dev ($dev) has no version tag" >&2
  return 1
}

pins=(
  "NODE_RUNTIME_IMAGE node $node_major"
  "NODE_DEV_IMAGE node $node_major-dev"
  "BUN_RUNTIME_IMAGE bun $bun_version"
  "BUN_DEV_IMAGE bun $bun_version-dev"
  "RUST_RUNTIME_IMAGE rust $(rust_runtime_tag)"
  "RUST_DEV_IMAGE rust $(rust_dev_tag)"
)

for pin in "${pins[@]}"; do
  read -r name repository tag <<<"$pin"
  sha="$(digest "$repository" "$tag")"
  case "$sha" in
    sha256:*) ;;
    *) echo "$repository:$tag not found in $registry" >&2; exit 1 ;;
  esac
  reference="$registry/$repository:$tag@$sha"
  echo "$name = \"$reference\""
  if $write; then
    # rustfmt may put a long value on the line after `=`.
    NAME="$name" REFERENCE="$reference" perl -0pi -e \
      's/(pub\(crate\) const \Q$ENV{NAME}\E: &str =\s*)"[^"]*";/$1"$ENV{REFERENCE}";/' \
      "$constants"
    grep -q "$reference" "$constants" || { echo "could not update $name" >&2; exit 1; }
  fi
done
