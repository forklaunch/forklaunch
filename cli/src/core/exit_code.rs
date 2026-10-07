//! Exit codes for outcomes a script or agent needs to tell apart.
//!
//! `anyhow` collapses every error into exit 1, which is right for "the command
//! could not do what you asked". A deploy that is parked behind an approval
//! gate, or one the CLI stopped waiting on, is not that: the deploy itself is
//! fine. A wrapper that treats every non-zero as "the deploy failed" would
//! retry or roll back a deploy that is merely waiting.
//!
//! `main` downcasts the top-level error to `ExitWith` and uses its code.

use std::fmt;

/// Exit 2: the deploy is parked awaiting approval; nothing failed.
pub(crate) const EXIT_AWAITING_APPROVAL: i32 = 2;
/// Exit 3: the CLI stopped waiting before the deploy reached a terminal state.
pub(crate) const EXIT_STILL_RUNNING: i32 = 3;

#[derive(Debug)]
pub(crate) struct ExitWith {
    pub(crate) code: i32,
    pub(crate) message: String,
}

impl fmt::Display for ExitWith {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExitWith {}

impl ExitWith {
    pub(crate) fn new(code: i32, message: impl Into<String>) -> anyhow::Error {
        anyhow::Error::new(ExitWith {
            code,
            message: message.into(),
        })
    }
}

/// The process exit code an error should produce: the `ExitWith` code when
/// the error carries one, else 1.
pub(crate) fn exit_code_for(error: &anyhow::Error) -> i32 {
    error
        .downcast_ref::<ExitWith>()
        .map(|e| e.code)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_error_exits_one() {
        assert_eq!(exit_code_for(&anyhow::anyhow!("boom")), 1);
    }

    #[test]
    fn an_exit_with_carries_its_code_through_context() {
        let err = ExitWith::new(EXIT_AWAITING_APPROVAL, "parked");
        assert_eq!(exit_code_for(&err), EXIT_AWAITING_APPROVAL);
        // Wrapping with context must not hide the code; main sees the outer error.
        let wrapped = err.context("while deploying");
        assert_eq!(exit_code_for(&wrapped), EXIT_AWAITING_APPROVAL);
        assert_eq!(wrapped.root_cause().to_string(), "parked");
    }
}
