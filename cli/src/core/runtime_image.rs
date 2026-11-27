//! The base-image stages of generated production images.
//!
//! Every generated Dockerfile builds on a ForkLaunch `-dev` image and runs on
//! the matching ForkLaunch runtime image (github.com/forklaunch/base-images),
//! each pinned by digest (see the `*_IMAGE` constants). The runtime images
//! already carry the users the old Debian images had (`node` and `bun` are
//! uid/gid 1000, Rust's default is `nonroot` 65532), curl for the generated
//! Compose health checks, and CA roots.
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::{
    BASE_IMAGE_REGISTRY, BUN_DEV_IMAGE, BUN_RUNTIME_IMAGE, NODE_DEV_IMAGE, NODE_RUNTIME_IMAGE,
    RUST_DEV_IMAGE, RUST_RUNTIME_IMAGE,
};

/// Where the builder installs the application's pinned pnpm.
const PNPM_PREFIX: &str = "/opt/pnpm";

/// The release of a `packageManager` pin such as `pnpm@12.23.0`.
fn pnpm_release(pin: &str) -> &str {
    pin.strip_prefix("pnpm@").unwrap_or(pin)
}

/// pnpm 12 and later ship as one native executable; earlier releases are a
/// JavaScript package that runs on Node.
fn native_pnpm(release: &str) -> bool {
    release
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 12)
}

/// Builder stage of the shared Node image: the dev image (Node, npm, git and
/// the native-addon toolchain) plus exactly the pnpm the workspace pins,
/// installed with npm because Node 25 and later ship no corepack. It lives under
/// /opt/pnpm so it is found before the image's own pnpm and can be copied into
/// the runtime stage on its own.
pub(crate) fn node_builder_stage(pnpm_pin: &str) -> String {
    let release = pnpm_release(pnpm_pin);
    format!(
        "FROM {NODE_DEV_IMAGE} AS builder\nRUN npm install -g --prefix {PNPM_PREFIX} pnpm@{release} && [ \"$({PNPM_PREFIX}/bin/pnpm --version)\" = \"{release}\" ]\nENV PATH={PNPM_PREFIX}/bin:$PATH\nWORKDIR /app\n"
    )
}

/// Builder stage of the shared Bun image (runs as root, with git and the
/// native-addon toolchain).
pub(crate) fn bun_builder_stage() -> String {
    format!("FROM {BUN_DEV_IMAGE} AS builder\nWORKDIR /app\n")
}

/// Builder stage of Rust images: the toolchain, pkg-config, OpenSSL 3.6
/// headers, cmake, perl and a C/C++ compiler.
pub(crate) fn rust_builder_from(stage: &str) -> String {
    format!("FROM {RUST_DEV_IMAGE} AS {stage}")
}

/// Runtime stage of the shared Node image, up to and including `USER node`.
/// ForkLaunch starts processes with `pnpm run start:*`, so the pinned pnpm
/// travels with the application: pnpm 12's native executable alone (glibc,
/// libgcc and libstdc++ are in the image), or an earlier release's package.
/// pnpm 11 and later verify dependencies before `pnpm run` and reinstall when
/// they look stale; a `pnpm deploy` tree always looks stale to them (its store
/// is gone), and the reinstall deletes it. The image's tree is final, so that
/// check is off.
pub(crate) fn node_runtime_stage(pnpm_pin: &str) -> String {
    let release = pnpm_release(pnpm_pin);
    let pnpm = if native_pnpm(release) {
        format!(
            "COPY --from=builder {PNPM_PREFIX}/lib/node_modules/pnpm/pnpm /usr/local/bin/pnpm\n"
        )
    } else {
        format!(
            "COPY --from=builder {PNPM_PREFIX} {PNPM_PREFIX}\nENV PATH={PNPM_PREFIX}/bin:$PATH\n"
        )
    };
    format!(
        "FROM {NODE_RUNTIME_IMAGE} AS runtime\nWORKDIR /app\n{pnpm}RUN [ \"$(pnpm --version)\" = \"{release}\" ]\nENV pnpm_config_verify_deps_before_run=false\nCOPY --from=builder --chown=node:node /out/ /app/\nENV NODE_ENV=production\nUSER node\n"
    )
}

/// Runtime stage of the shared Bun image, up to and including `USER bun`. The
/// image's entrypoint is `bun`; it is cleared so Compose and platform commands
/// (`/bin/sh -c ...`, `bun run ...`) run as written.
pub(crate) fn bun_runtime_stage() -> String {
    format!(
        "FROM {BUN_RUNTIME_IMAGE} AS runtime\nENTRYPOINT []\nCMD [\"bun\"]\nWORKDIR /app\nCOPY --from=builder --chown=bun:bun /out/ /app/\nENV NODE_ENV=production\nUSER bun\n"
    )
}

/// Runtime stage header of the shared (mixed application) Rust image. It
/// prepares /app as root; the image switches to 65532 after copying binaries.
pub(crate) fn rust_runtime_stage() -> String {
    format!("FROM {RUST_RUNTIME_IMAGE} AS runtime\nUSER root\nWORKDIR /app\n")
}

/// Builder stage header of the single-application Rust Dockerfile template.
pub(crate) fn rust_template_builder_stage() -> String {
    format!(
        "{}\n\nARG FRAMEWORK_ROOT=/forklaunch\nWORKDIR /app\n",
        rust_builder_from("builder")
    )
}

/// Runtime stage header of the single-application Rust Dockerfile template.
/// That image has always run as root; the runtime image's default is 65532.
pub(crate) fn rust_template_runtime_stage() -> String {
    format!("FROM {RUST_RUNTIME_IMAGE} AS runtime\n\nUSER root\nWORKDIR /app\n")
}

static PINNED_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"({}/[a-z0-9-]+):[A-Za-z0-9._-]+@sha256:[0-9a-f]{{64}}",
        regex::escape(BASE_IMAGE_REGISTRY)
    ))
    .unwrap()
});

/// `content` with every ForkLaunch base-image tag and digest blanked, so a file
/// generated under older pins still compares equal to the current rendering.
pub(crate) fn without_pins(content: &str) -> String {
    PINNED_IMAGE
        .replace_all(content, "$1:<tag>@sha256:<pin>")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{BUN_VERSION, NODE_MAJOR, PNPM_VERSION};

    fn tag(image: &str) -> &str {
        image
            .rsplit_once('@')
            .unwrap()
            .0
            .rsplit_once(':')
            .unwrap()
            .1
    }

    #[test]
    fn every_pin_is_a_forklaunch_image_with_a_versioned_tag_and_an_index_digest() {
        for image in [
            NODE_RUNTIME_IMAGE,
            NODE_DEV_IMAGE,
            BUN_RUNTIME_IMAGE,
            BUN_DEV_IMAGE,
            RUST_RUNTIME_IMAGE,
            RUST_DEV_IMAGE,
        ] {
            assert!(
                image.starts_with(&format!("{BASE_IMAGE_REGISTRY}/")),
                "{image}"
            );
            let (reference, digest) = image.split_once('@').unwrap();
            assert!(
                digest.starts_with("sha256:") && digest.len() == 71,
                "{image}"
            );
            assert!(digest[7..].bytes().all(|b| b.is_ascii_hexdigit()));
            let tag = reference.rsplit_once(':').unwrap().1;
            assert!(!tag.is_empty() && !tag.contains("latest"), "{image}");
            assert_eq!(without_pins(image).matches("<pin>").count(), 1, "{image}");
        }
        assert_eq!(tag(NODE_RUNTIME_IMAGE), NODE_MAJOR);
        assert_eq!(tag(NODE_DEV_IMAGE), format!("{NODE_MAJOR}-dev"));
        assert_eq!(tag(BUN_RUNTIME_IMAGE), BUN_VERSION);
        assert_eq!(tag(BUN_DEV_IMAGE), format!("{BUN_VERSION}-dev"));
        assert!(RUST_DEV_IMAGE.contains("/rust:") && tag(RUST_DEV_IMAGE).ends_with("-dev"));
        assert!(RUST_RUNTIME_IMAGE.contains("/rust:") && !tag(RUST_RUNTIME_IMAGE).contains("dev"));
    }

    #[test]
    fn node_stages_install_and_carry_the_pinned_pnpm() {
        let pin = format!("pnpm@{PNPM_VERSION}");
        let builder = node_builder_stage(&pin);
        assert!(builder.starts_with(&format!("FROM {NODE_DEV_IMAGE} AS builder\n")));
        assert!(builder.contains(&format!(
            "npm install -g --prefix /opt/pnpm pnpm@{PNPM_VERSION} && [ \"$(/opt/pnpm/bin/pnpm --version)\" = \"{PNPM_VERSION}\" ]"
        )));
        assert!(!builder.contains("corepack"));
        let runtime = node_runtime_stage(&pin);
        assert!(runtime.starts_with(&format!("FROM {NODE_RUNTIME_IMAGE} AS runtime\n")));
        assert!(runtime.contains(
            "COPY --from=builder /opt/pnpm/lib/node_modules/pnpm/pnpm /usr/local/bin/pnpm\n"
        ));
        assert!(runtime.ends_with(
            "COPY --from=builder --chown=node:node /out/ /app/\nENV NODE_ENV=production\nUSER node\n"
        ));
        // A JavaScript pnpm (before 12) travels as its package.
        let legacy = node_runtime_stage("pnpm@10.33.2");
        assert!(
            legacy.contains(
                "COPY --from=builder /opt/pnpm /opt/pnpm\nENV PATH=/opt/pnpm/bin:$PATH\n"
            )
        );
        assert!(legacy.contains("RUN [ \"$(pnpm --version)\" = \"10.33.2\" ]"));
        for stage in [&runtime, &legacy] {
            assert!(stage.contains("ENV pnpm_config_verify_deps_before_run=false\n"));
        }
    }

    #[test]
    fn bun_and_rust_stages_use_their_images() {
        let bun = bun_runtime_stage();
        assert!(bun.starts_with(&format!(
            "FROM {BUN_RUNTIME_IMAGE} AS runtime\nENTRYPOINT []\nCMD [\"bun\"]\n"
        )));
        assert!(bun.ends_with(
            "COPY --from=builder --chown=bun:bun /out/ /app/\nENV NODE_ENV=production\nUSER bun\n"
        ));
        assert!(bun_builder_stage().starts_with(&format!("FROM {BUN_DEV_IMAGE} AS builder\n")));
        assert!(rust_runtime_stage().starts_with(&format!(
            "FROM {RUST_RUNTIME_IMAGE} AS runtime\nUSER root\n"
        )));
        assert!(rust_template_runtime_stage().contains("USER root"));
        assert!(
            rust_template_builder_stage()
                .starts_with(&format!("FROM {RUST_DEV_IMAGE} AS builder\n"))
        );
        for stage in [
            node_builder_stage("pnpm@12.23.0"),
            node_runtime_stage("pnpm@12.23.0"),
            bun_builder_stage(),
            bun_runtime_stage(),
            rust_runtime_stage(),
            rust_template_builder_stage(),
            rust_template_runtime_stage(),
        ] {
            assert!(
                !stage.contains("apt-get") && !stage.contains("apk add"),
                "{stage}"
            );
        }
    }

    #[test]
    fn pins_are_ignored_only_where_they_are_pins() {
        let older = node_runtime_stage("pnpm@12.23.0").replace(
            NODE_RUNTIME_IMAGE,
            &format!("{BASE_IMAGE_REGISTRY}/node:26@sha256:{}", "0".repeat(64)),
        );
        assert_ne!(older, node_runtime_stage("pnpm@12.23.0"));
        assert_eq!(
            without_pins(&older),
            without_pins(&node_runtime_stage("pnpm@12.23.0"))
        );
        let edited = node_runtime_stage("pnpm@12.23.0").replace("USER node", "USER root");
        assert_ne!(
            without_pins(&edited),
            without_pins(&node_runtime_stage("pnpm@12.23.0"))
        );
        assert_eq!(
            without_pins("FROM node:26\nENV PORT=8000"),
            "FROM node:26\nENV PORT=8000"
        );
    }
}
