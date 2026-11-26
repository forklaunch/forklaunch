#!/usr/bin/env bash
# Print the current pins for the Wolfi runtime stage of generated Dockerfiles:
# the multi-arch digest of cgr.dev/chainguard/wolfi-base:latest and the newest
# version of each package the runtime stage installs, on both architectures.
#
# Copy the output into the WOLFI_* constants in cli/src/constants.rs, then run
# `cargo test` (the Dockerfile tests read the constants, so only snapshots of
# whole files need updating). A package must exist at the same version on
# x86_64 and aarch64; the script flags any mismatch.
#
# Usage: cli/scripts/wolfi-runtime-pins.sh [bun-version]
#   bun-version  the BUN_VERSION constant (default: read from constants.rs);
#                the newest Wolfi build of that exact release is printed.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
constants="$here/../src/constants.rs"
bun_version="${1:-$(sed -n 's/^pub(crate) const BUN_VERSION: &str = "\(.*\)";/\1/p' "$constants")}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The index digest, not a per-architecture manifest digest, so one pin serves
# linux/amd64 and linux/arm64.
token="$(curl -fsSL 'https://cgr.dev/token?scope=repository:chainguard/wolfi-base:pull' | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')"
digest="$(curl -fsSI \
  -H "Authorization: Bearer $token" \
  -H 'Accept: application/vnd.oci.image.index.v1+json' \
  -H 'Accept: application/vnd.docker.distribution.manifest.list.v2+json' \
  https://cgr.dev/v2/chainguard/wolfi-base/manifests/latest |
  tr -d '\r' | awk -F': ' 'tolower($1)=="docker-content-digest"{print $2}')"
echo "WOLFI_BASE_DIGEST = \"$digest\""

for arch in x86_64 aarch64; do
  mkdir -p "$work/$arch"
  curl -fsSL "https://packages.wolfi.dev/os/$arch/APKINDEX.tar.gz" | tar -xzf - -C "$work/$arch" APKINDEX
done

# Newest "<version>-r<n>" of a package, optionally limited to one upstream release.
newest() {
  local arch="$1" package="$2" release="${3:-}"
  awk -v p="$package" '/^P:/{n=substr($0,3)} /^V:/{if(n==p)print substr($0,3)}' "$work/$arch/APKINDEX" |
    awk -v r="$release" 'r=="" || index($0, r "-r")==1' |
    sort -V | tail -1
}

report() {
  local constant="$1" package="$2" release="${3:-}"
  local x86 arm
  x86="$(newest x86_64 "$package" "$release" || true)"
  arm="$(newest aarch64 "$package" "$release" || true)"
  if [ -z "$x86" ] || [ -z "$arm" ]; then
    echo "$constant: $package${release:+ $release} not found (x86_64='$x86' aarch64='$arm')" >&2
    return 1
  elif [ "$x86" != "$arm" ]; then
    echo "$constant = \"$x86\"  # WARNING: aarch64 has $arm; pick a version both carry" >&2
  else
    echo "$constant = \"$x86\""
  fi
}

status=0
report WOLFI_NODEJS_22 nodejs-22 || status=1
report WOLFI_BUN bun "$bun_version" || status=1
report WOLFI_CA_CERTIFICATES ca-certificates-bundle || status=1
report WOLFI_CURL curl || status=1
report WOLFI_LIBSTDCXX libstdc++ || status=1
report WOLFI_OPENSSL_3_LIBS openssl-3.6-libssl || status=1
exit "$status"
