//! The final stage of generated production images.
//!
//! Every generated Dockerfile builds on Debian and runs on Wolfi
//! (`cgr.dev/chainguard/wolfi-base`), pinned by digest with pinned packages
//! (see the `WOLFI_*` constants). Only the runtime stage changes base; the
//! users, paths and binaries the Debian runtime images provided are recreated
//! here so `COPY --chown`, `USER` and compose commands keep working.
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::{
    WOLFI_BASE_DIGEST, WOLFI_BASE_IMAGE, WOLFI_BUN, WOLFI_CA_CERTIFICATES, WOLFI_CURL,
    WOLFI_LIBSTDCXX, WOLFI_NODEJS_22, WOLFI_OPENSSL_3_LIBS,
};

/// `FROM <wolfi-base>@<digest>`, without a stage name.
pub(crate) fn wolfi_from() -> String {
    format!("FROM {WOLFI_BASE_IMAGE}@{WOLFI_BASE_DIGEST}")
}

/// `apk add` of exact package builds. curl stays because generated Compose
/// health checks call it inside the container.
fn apk_add(packages: &[(&str, &str)]) -> String {
    let pins = packages
        .iter()
        .map(|(name, version)| format!("{name}={version}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("apk add --no-cache {pins}")
}

/// The account `node:*` and `oven/bun:*` images ship (uid/gid 1000), so
/// `--chown=<name>:<name>` and `USER <name>` resolve as they did on Debian.
fn add_user(name: &str) -> String {
    format!(
        "addgroup -g 1000 {name} && adduser -D -u 1000 -G {name} -h /home/{name} -s /bin/sh {name}"
    )
}

/// Runtime stage of the shared Node image, up to and including `USER node`.
pub(crate) fn node_runtime_stage() -> String {
    format!(
        "{} AS runtime\nRUN {} && {}\nWORKDIR /app\nCOPY --from=builder --chown=node:node /out/ /app/\nENV NODE_ENV=production\nUSER node\n",
        wolfi_from(),
        apk_add(&[
            ("nodejs-22", WOLFI_NODEJS_22),
            ("ca-certificates-bundle", WOLFI_CA_CERTIFICATES),
            ("curl", WOLFI_CURL),
        ]),
        add_user("node"),
    )
}

/// Runtime stage of the shared Bun image, up to and including `USER bun`.
/// libstdc++ is for prebuilt native addons; Wolfi's bun itself needs only glibc.
pub(crate) fn bun_runtime_stage() -> String {
    format!(
        "{} AS runtime\nRUN {} && {}\nWORKDIR /app\nCOPY --from=builder --chown=bun:bun /out/ /app/\nENV NODE_ENV=production\nUSER bun\n",
        wolfi_from(),
        apk_add(&[
            ("bun", WOLFI_BUN),
            ("libstdc++", WOLFI_LIBSTDCXX),
            ("ca-certificates-bundle", WOLFI_CA_CERTIFICATES),
            ("curl", WOLFI_CURL),
        ]),
        add_user("bun"),
    )
}

/// Packages a Rust binary built on Debian bookworm needs on Wolfi: OpenSSL 3
/// (bookworm's ABI; Wolfi defaults to OpenSSL 4), CA roots and curl. glibc and
/// libgcc are in wolfi-base. The `nonroot` account (65532) is too.
fn rust_packages() -> String {
    apk_add(&[
        ("openssl-3.6-libssl", WOLFI_OPENSSL_3_LIBS),
        ("ca-certificates-bundle", WOLFI_CA_CERTIFICATES),
        ("curl", WOLFI_CURL),
    ])
}

/// Runtime stage header of the shared (mixed application) Rust image.
pub(crate) fn rust_runtime_stage() -> String {
    format!(
        "{} AS runtime\nWORKDIR /app\nRUN {}\n",
        wolfi_from(),
        rust_packages()
    )
}

/// Runtime stage header of the single-application Rust Dockerfile template.
pub(crate) fn rust_template_runtime_stage() -> String {
    format!(
        "{} AS runtime\n\nWORKDIR /app\nRUN {}\n",
        wolfi_from(),
        rust_packages()
    )
}

static PINNED_DIGEST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(wolfi-base)@sha256:[0-9a-f]{64}").unwrap());
static PINNED_PACKAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)(\s[a-z0-9][a-z0-9.+_-]*)=[0-9][0-9A-Za-z.+_~-]*").unwrap());

/// `content` with the Wolfi digest and `apk add` package pins blanked, so a file
/// generated under older pins still compares equal to the current rendering.
/// Only `apk add` lines are touched.
pub(crate) fn without_pins(content: &str) -> String {
    let content = PINNED_DIGEST.replace_all(content, "$1@sha256:<pin>");
    content
        .lines()
        .map(|line| {
            if line.contains("apk add --no-cache ") {
                PINNED_PACKAGE.replace_all(line, "$1=<pin>").into_owned()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Node.js major version a runtime `apk add nodejs-<major>=...` installs.
pub(crate) fn wolfi_node_major(line: &str) -> Option<&str> {
    let rest = line.split_once("nodejs-")?.1;
    let major = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    (!major.is_empty() && line.contains("apk add")).then_some(major)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_stages_use_the_pinned_wolfi_base_and_packages() {
        let from = format!("FROM {WOLFI_BASE_IMAGE}@{WOLFI_BASE_DIGEST} AS runtime\n");
        assert!(WOLFI_BASE_DIGEST.starts_with("sha256:") && WOLFI_BASE_DIGEST.len() == 71);
        for stage in [
            node_runtime_stage(),
            bun_runtime_stage(),
            rust_runtime_stage(),
            rust_template_runtime_stage(),
        ] {
            assert!(stage.starts_with(&from), "{stage}");
            assert!(!stage.contains("apt-get"), "{stage}");
            assert!(stage.contains("apk add --no-cache "), "{stage}");
            assert!(
                stage.contains(&format!("ca-certificates-bundle={WOLFI_CA_CERTIFICATES}")),
                "{stage}"
            );
            assert!(stage.contains(&format!("curl={WOLFI_CURL}")), "{stage}");
        }
        let node = node_runtime_stage();
        assert!(node.contains(&format!("nodejs-22={WOLFI_NODEJS_22}")));
        assert!(node.contains("adduser -D -u 1000 -G node"));
        assert!(node.ends_with("COPY --from=builder --chown=node:node /out/ /app/\nENV NODE_ENV=production\nUSER node\n"));
        let bun = bun_runtime_stage();
        assert!(bun.contains(&format!("bun={WOLFI_BUN}")));
        assert!(bun.contains("adduser -D -u 1000 -G bun"));
        assert!(bun.ends_with(
            "COPY --from=builder --chown=bun:bun /out/ /app/\nENV NODE_ENV=production\nUSER bun\n"
        ));
        assert!(
            rust_runtime_stage().contains(&format!("openssl-3.6-libssl={WOLFI_OPENSSL_3_LIBS}"))
        );
    }

    #[test]
    fn wolfi_bun_is_a_build_of_the_pinned_bun_release() {
        assert!(
            WOLFI_BUN.starts_with(&format!("{}-r", crate::constants::BUN_VERSION)),
            "WOLFI_BUN {WOLFI_BUN} must be a Wolfi build of BUN_VERSION"
        );
    }

    #[test]
    fn pins_are_ignored_only_where_they_are_pins() {
        let older = node_runtime_stage()
            .replace(WOLFI_BASE_DIGEST, &format!("sha256:{}", "0".repeat(64)))
            .replace(WOLFI_NODEJS_22, "22.20.0-r0")
            .replace(WOLFI_CURL, "8.1.0-r3");
        assert_ne!(older, node_runtime_stage());
        assert_eq!(without_pins(&older), without_pins(&node_runtime_stage()));
        // A different package set, or an edit outside the pins, still differs.
        let swapped = node_runtime_stage().replace("nodejs-22=", "nodejs-24=");
        assert_ne!(without_pins(&swapped), without_pins(&node_runtime_stage()));
        let edited = node_runtime_stage().replace("USER node", "USER root");
        assert_ne!(without_pins(&edited), without_pins(&node_runtime_stage()));
        // Other `=` assignments are left alone.
        assert_eq!(without_pins("ENV PORT=8000"), "ENV PORT=8000");
    }

    #[test]
    fn node_major_is_read_from_the_runtime_install() {
        let line = node_runtime_stage().lines().nth(1).unwrap().to_owned();
        assert_eq!(wolfi_node_major(&line), Some("22"));
        assert_eq!(
            wolfi_node_major("RUN apk add --no-cache nodejs-24=24.1.0-r0"),
            Some("24")
        );
        assert_eq!(wolfi_node_major("RUN echo nodejs-24"), None);
        assert_eq!(wolfi_node_major("RUN apk add --no-cache curl"), None);
    }
}
