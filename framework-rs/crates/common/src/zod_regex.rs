//! Checked ECMAScript regex admission for the pinned Node 24.2 / Unicode 16 target.
//!
//! Only bounded entry points are exposed. The engine operates on UTF-16 units;
//! Rust `str` callers cannot supply lone surrogates, while schema pattern units
//! can preserve them. Limits count conservative work/requested storage, not RSS
//! or wall-clock time. Each call resets RegExp state as Zod v3 does.
use std::fmt;

pub use regress::limits::{
    CompileLimits as RegexCompileLimits, MatchLimits as RegexMatchLimits,
    ResourceLimit as RegexResourceLimit,
};

/// Invalid registration and operational resource exhaustion are distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexCompileError {
    InvalidFlags,
    Syntax(String),
    Resource(RegexResourceLimit),
}
impl fmt::Display for RegexCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFlags => {
                formatter.write_str("invalid ECMAScript regular expression flags")
            }
            Self::Syntax(message) => write!(formatter, "invalid regular expression: {message}"),
            Self::Resource(limit) => {
                write!(formatter, "regular expression compilation limit: {limit:?}")
            }
        }
    }
}
impl std::error::Error for RegexCompileError {}

/// An immutable compiled regex; no unrestricted engine accessor is provided.
pub struct ZodRegex {
    compiled: regress::Regex,
    sticky: bool,
}
impl fmt::Debug for ZodRegex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ZodRegex")
            .field("sticky", &self.sticky)
            .finish_non_exhaustive()
    }
}
impl ZodRegex {
    /// Compile a raw UTF-16 pattern after checking flags, before engine allocation.
    /// `g`/`d` do not change boolean validity; `y` requires a match at offset zero.
    /// # Errors
    /// Returns a syntax/flag error or a distinct resource failure.
    pub fn compile(
        source_units: &[u16],
        flags: &str,
        limits: RegexCompileLimits,
    ) -> Result<Self, RegexCompileError> {
        validate_flags(flags)?;
        let compiled =
            regress::Regex::from_utf16_limited(source_units, regress::Flags::from(flags), limits)
                .map_err(|error| match error {
                regress::limits::CompileFailure::Syntax(error) => {
                    RegexCompileError::Syntax(error.text)
                }
                regress::limits::CompileFailure::Resource(limit) => {
                    RegexCompileError::Resource(limit)
                }
            })?;
        Ok(Self {
            compiled,
            sticky: flags.contains('y'),
        })
    }

    /// Test the original UTF-16 representation with fresh matching state.
    /// # Errors
    /// Resource exhaustion is never a false match result.
    pub fn test_utf16(
        &self,
        value: &[u16],
        limits: RegexMatchLimits,
    ) -> Result<bool, RegexResourceLimit> {
        self.compiled.test_utf16_limited(value, self.sticky, limits)
    }

    /// Test a Rust string without allocating beyond the admitted input-unit limit.
    /// # Errors
    /// Returns input/storage/work exhaustion independently of ordinary no-match.
    pub fn test(&self, value: &str, limits: RegexMatchLimits) -> Result<bool, RegexResourceLimit> {
        let count = value
            .encode_utf16()
            .take(limits.input_units.saturating_add(1))
            .count();
        if count > limits.input_units {
            return Err(RegexResourceLimit::Input);
        }
        let mut units = Vec::new();
        units
            .try_reserve_exact(count)
            .map_err(|_| RegexResourceLimit::Input)?;
        units.extend(value.encode_utf16());
        self.test_utf16(&units, limits)
    }
}

fn validate_flags(flags: &str) -> Result<(), RegexCompileError> {
    let mut seen = 0_u16;
    for flag in flags.bytes() {
        let bit = match flag {
            b'd' => 1,
            b'g' => 2,
            b'i' => 4,
            b'm' => 8,
            b's' => 16,
            b'u' => 32,
            b'v' => 64,
            b'y' => 128,
            _ => return Err(RegexCompileError::InvalidFlags),
        };
        if seen & bit != 0 {
            return Err(RegexCompileError::InvalidFlags);
        }
        seen |= bit;
    }
    if seen & 32 != 0 && seen & 64 != 0 {
        return Err(RegexCompileError::InvalidFlags);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compile(pattern: &str, flags: &str) -> ZodRegex {
        ZodRegex::compile(
            &pattern.encode_utf16().collect::<Vec<_>>(),
            flags,
            RegexCompileLimits::default(),
        )
        .unwrap()
    }
    #[test]
    fn invalid_flags_precede_engine_admission() {
        for flags in ["gg", "uv", "ii", "z", "é"] {
            assert!(matches!(
                ZodRegex::compile(&[], flags, RegexCompileLimits::default()),
                Err(RegexCompileError::InvalidFlags)
            ));
        }
    }
    #[test]
    fn resets_global_and_respects_sticky() {
        let global = compile("a", "g");
        for _ in 0..3 {
            assert_eq!(global.test("ba", RegexMatchLimits::default()), Ok(true));
        }
        assert_eq!(
            compile("a", "y").test("ba", RegexMatchLimits::default()),
            Ok(false)
        );
    }
    #[test]
    fn retains_utf16_and_legacy_case_semantics() {
        assert_eq!(
            compile("^.$", "").test("😀", RegexMatchLimits::default()),
            Ok(false)
        );
        assert_eq!(
            compile("^.$", "u").test("😀", RegexMatchLimits::default()),
            Ok(true)
        );
        assert_eq!(
            compile("k", "i").test("\u{212a}", RegexMatchLimits::default()),
            Ok(false)
        );
        assert_eq!(
            compile("k", "iu").test("\u{212a}", RegexMatchLimits::default()),
            Ok(true)
        );
        assert_eq!(
            compile("^.$", "u").test_utf16(&[0xd800], RegexMatchLimits::default()),
            Ok(true)
        );
    }
    #[test]
    fn exhaustion_is_not_false() {
        let regex = compile("^(a+)+$", "");
        assert_eq!(
            regex.test(
                &format!("{}!", "a".repeat(40)),
                RegexMatchLimits {
                    work: 1000,
                    ..RegexMatchLimits::default()
                }
            ),
            Err(RegexResourceLimit::Work)
        );
    }
}
