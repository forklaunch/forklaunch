use regress::{
    Flags, Regex,
    limits::{MatchLimits, ResourceLimit},
};
fn test(pattern: &str, text: &str, limits: MatchLimits) -> Result<bool, ResourceLimit> {
    let regex = Regex::with_flags(
        pattern,
        Flags {
            no_opt: true,
            ..Flags::default()
        },
    )
    .unwrap();
    regex.test_utf16_limited(&text.encode_utf16().collect::<Vec<_>>(), false, limits)
}
#[test]
fn ordinary_and_failure() {
    let limits = MatchLimits::default();
    assert_eq!(test("^(a|b)+$", "abba", limits), Ok(true));
    assert_eq!(test("^(a|b)+$", "abc", limits), Ok(false));
}
#[test]
fn exhausted_is_not_false() {
    for pattern in ["^(a+)+$", "^(a|aa)+$", "(?!(a+)+$)b"] {
        let limits = MatchLimits {
            work: 10000,
            ..MatchLimits::default()
        };
        assert_eq!(
            test(pattern, &format!("{}!", "a".repeat(40)), limits),
            Err(ResourceLimit::Work)
        );
    }
}
#[test]
fn stack_and_input_before_match() {
    assert_eq!(
        test(
            "a",
            "a",
            MatchLimits {
                stack_bytes: 0,
                ..MatchLimits::default()
            }
        ),
        Err(ResourceLimit::Stack)
    );
    assert_eq!(
        test(
            "a",
            "ab",
            MatchLimits {
                input_units: 1,
                ..MatchLimits::default()
            }
        ),
        Err(ResourceLimit::Input)
    );
}
#[test]
fn lookaround_shared_budget() {
    assert_eq!(
        test(
            "(?=(?=(?=a)))a",
            "a",
            MatchLimits {
                lookaround_depth: 1,
                ..MatchLimits::default()
            }
        ),
        Err(ResourceLimit::Lookaround)
    );
}
fn compile(
    pattern: &str,
    flags: &str,
    limits: regress::limits::CompileLimits,
) -> Result<Regex, regress::limits::CompileFailure> {
    Regex::from_utf16_limited(
        &pattern.encode_utf16().collect::<Vec<_>>(),
        Flags::from(flags),
        limits,
    )
}
#[test]
fn compile_admits_ordinary_and_escaped_nesting() {
    use regress::limits::CompileLimits as L;
    for (pattern, flags) in [
        ("^(a|b)+$", ""),
        ("^[a-z]+$", "i"),
        (r"^\p{Letter}+$", "u"),
        (r"^\(\[\]\)$", ""),
        (r"[(){}]", ""),
        (r"[[a-z]--[aeiou]]", "iv"),
    ] {
        assert!(
            compile(pattern, flags, L::default()).is_ok(),
            "{pattern}/{flags}"
        );
    }
}
#[test]
fn compile_rejects_depth_input_and_expansion() {
    use regress::limits::{CompileFailure as F, CompileLimits as L};
    assert!(matches!(
        compile(
            "aaaa",
            "",
            L {
                pattern_units: 3,
                ..L::default()
            }
        ),
        Err(F::Resource(ResourceLimit::CompileInput))
    ));
    assert!(matches!(
        compile(
            &format!("{}a{}", "(".repeat(49), ")".repeat(49)),
            "",
            L::default()
        ),
        Err(F::Resource(ResourceLimit::CompileDepth))
    ));
    assert!(matches!(
        compile(
            r"\p{RGI_Emoji}",
            "v",
            L {
                allocation_units: 4096,
                ..L::default()
            }
        ),
        Err(F::Resource(ResourceLimit::CompileExpansion))
    ));
}
#[test]
fn compile_quantifier_overflow_typed() {
    use regress::limits::{CompileFailure as F, CompileLimits as L};
    for pattern in [
        "a{999999999999999999999999999999}",
        "a{1,99999999999999999999999999999}",
    ] {
        assert!(matches!(
            compile(pattern, "", L::default()),
            Err(F::Resource(ResourceLimit::CompileArithmetic))
        ));
    }
}
#[test]
fn compile_syntax_distinct() {
    assert!(matches!(
        compile("(", "", regress::limits::CompileLimits::default()),
        Err(regress::limits::CompileFailure::Syntax(_))
    ));
}
#[test]
fn aggregate_property_admission() {
    use regress::limits::{CompileFailure as F, CompileLimits as L};
    let pattern = r"\p{Letter}".repeat(100);
    assert!(compile(&pattern, "u", L::default()).is_ok());
    assert!(matches!(
        compile(
            &pattern,
            "u",
            L {
                allocation_units: 1 << 20,
                ..L::default()
            }
        ),
        Err(F::Resource(ResourceLimit::CompileExpansion))
    ));
}
#[test]
fn emoji_balanced_compilation() {
    use regress::limits::CompileLimits as L;
    let r = compile(
        r"\p{RGI_Emoji}",
        "v",
        L {
            allocation_units: 256 << 20,
            work: 1_000_000_000_000,
            ..L::default()
        },
    )
    .unwrap();
    assert_eq!(
        r.test_utf16_limited(
            &"👨‍👩‍👦".encode_utf16().collect::<Vec<_>>(),
            false,
            MatchLimits {
                work: 1_000_000_000,
                ..MatchLimits::default()
            }
        ),
        Ok(true)
    );
}
