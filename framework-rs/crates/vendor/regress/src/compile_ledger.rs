//! Private cumulative allocation/work ledger. Only limited compilation installs it.
//! Counts requested collection storage, not allocator bookkeeping or process RSS.
#[cfg(all(feature = "std", feature = "utf16"))]
use crate::limits::{CompileLimits, ResourceLimit};
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};
#[cfg(all(feature = "std", feature = "utf16"))]
#[derive(Clone, Copy)]
struct Ledger {
    bytes: u64,
    work: u64,
    failed: Option<ResourceLimit>,
}
#[cfg(all(feature = "std", feature = "utf16"))]
std::thread_local! { static CURRENT: std::cell::RefCell<Option<Ledger>> = const { std::cell::RefCell::new(None) }; }
#[cfg(all(feature = "std", feature = "utf16"))]
pub(crate) struct Scope(Option<Ledger>);
#[cfg(all(feature = "std", feature = "utf16"))]
impl Scope {
    pub fn enter(limits: CompileLimits) -> Self {
        Self(CURRENT.with(|slot| {
            slot.replace(Some(Ledger {
                bytes: limits.allocation_units,
                work: limits.work,
                failed: None,
            }))
        }))
    }
    pub fn outcome(&self) -> Result<(), ResourceLimit> {
        CURRENT.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|l| l.failed)
                .map_or(Ok(()), Err)
        })
    }
    pub fn remaining(&self, mut limits: CompileLimits) -> CompileLimits {
        CURRENT.with(|slot| {
            if let Some(l) = slot.borrow().as_ref() {
                limits.allocation_units = l.bytes;
                limits.work = l.work;
            }
        });
        limits
    }
}
#[cfg(all(feature = "std", feature = "utf16"))]
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}
pub(crate) fn charge(bytes: usize, work: usize) -> bool {
    #[cfg(all(feature = "std", feature = "utf16"))]
    {
        CURRENT.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(l) = slot.as_mut() else { return true };
            // Saturating callers use MAX as a fail-closed overflow sentinel,
            // even when a caller supplies the largest possible quota.
            if bytes == usize::MAX || work == usize::MAX {
                l.failed.get_or_insert(ResourceLimit::CompileArithmetic);
                return false;
            }
            if l.failed.is_some() {
                return false;
            }
            match (
                l.bytes.checked_sub(bytes as u64),
                l.work.checked_sub(work as u64),
            ) {
                (Some(b), Some(w)) => {
                    l.bytes = b;
                    l.work = w;
                    true
                }
                _ => {
                    l.failed = Some(ResourceLimit::CompileExpansion);
                    false
                }
            }
        })
    }
    #[cfg(not(all(feature = "std", feature = "utf16")))]
    {
        let _ = (bytes, work);
        true
    }
}
pub(crate) fn failed() -> bool {
    !charge(0, 0)
}
pub(crate) fn active() -> bool {
    #[cfg(all(feature = "std", feature = "utf16"))]
    {
        CURRENT.with(|s| s.borrow().is_some())
    }
    #[cfg(not(all(feature = "std", feature = "utf16")))]
    {
        false
    }
}
pub(crate) fn arithmetic_failure() -> bool {
    #[cfg(all(feature = "std", feature = "utf16"))]
    CURRENT.with(|slot| {
        if let Some(l) = slot.borrow_mut().as_mut() {
            l.failed.get_or_insert(ResourceLimit::CompileArithmetic);
        }
    });
    false
}
pub(crate) fn reserve<T>(v: &mut Vec<T>, additional: usize) -> bool {
    if !active() {
        v.reserve(additional);
        return true;
    }
    let Some(required) = v.len().checked_add(additional) else {
        return arithmetic_failure();
    };
    if required <= v.capacity() {
        return charge(0, additional);
    }
    let Some(bytes) = required.checked_mul(core::mem::size_of::<T>()) else {
        return arithmetic_failure();
    };
    if !charge(bytes, additional) {
        return false;
    }
    if v.try_reserve_exact(additional).is_err() {
        let _ = arithmetic_failure();
        return false;
    }
    true
}
pub(crate) fn copy<T: Copy>(slice: &[T]) -> Vec<T> {
    let mut result = Vec::new();
    if reserve(&mut result, slice.len()) {
        result.extend_from_slice(slice)
    }
    result
}
pub(crate) fn push<T>(v: &mut Vec<T>, item: T) -> bool {
    if !reserve(v, 1) {
        return false;
    }
    v.push(item);
    true
}
pub(crate) fn push_char(s: &mut String, c: char) -> bool {
    if !active() {
        s.push(c);
        return true;
    }
    let extra = c.len_utf8();
    let Some(required) = s.len().checked_add(extra) else {
        return arithmetic_failure();
    };
    if !charge(if required > s.capacity() { required } else { 0 }, 1) {
        return false;
    }
    if s.try_reserve_exact(extra).is_err() {
        let _ = arithmetic_failure();
        return false;
    }
    s.push(c);
    true
}
#[cfg(all(test, feature = "std", feature = "utf16"))]
mod tests {
    use super::*;
    #[test]
    fn overflow_sentinel_rejects_even_maximum_quotas() {
        let scope = Scope::enter(CompileLimits {
            allocation_units: u64::MAX,
            work: u64::MAX,
            ..CompileLimits::default()
        });
        assert!(!charge(usize::MAX, 0));
        assert_eq!(scope.outcome(), Err(ResourceLimit::CompileArithmetic));
    }
    #[test]
    fn collection_failure_precedes_growth() {
        let scope = Scope::enter(CompileLimits {
            allocation_units: 8,
            ..CompileLimits::default()
        });
        let mut v = Vec::<u64>::new();
        assert!(!reserve(&mut v, 2));
        assert_eq!(v.capacity(), 0);
        assert_eq!(scope.outcome(), Err(ResourceLimit::CompileExpansion));
    }
    #[test]
    fn set_intersection_work_rejected_before_mutation() {
        let mut a = crate::codepointset::CodePointSet::new();
        let mut b = crate::codepointset::CodePointSet::new();
        for n in 0..100 {
            a.add_one(n * 3);
            b.add_one(n * 3);
        }
        let original = a.clone();
        let scope = Scope::enter(CompileLimits {
            work: 10,
            ..CompileLimits::default()
        });
        a.intersect(b.intervals());
        assert_eq!(a, original);
        assert_eq!(scope.outcome(), Err(ResourceLimit::CompileExpansion));
    }
    #[test]
    fn exhausted_parser_never_becomes_success_or_syntax() {
        for pattern in [
            r"\p{RGI_Emoji}",
            r"(?<name>a)|(?<name>b)",
            r"[[a-z]--[aeiou]]",
            r"[\q{a|abc|xyz}]",
            r"(?<=abc)def",
            r"[a-z]",
        ] {
            let scope = Scope::enter(CompileLimits {
                allocation_units: 512,
                work: 20,
                ..CompileLimits::default()
            });
            let _ =
                crate::parse::try_parse(pattern.chars().map(u32::from), crate::Flags::from("iv"));
            assert_eq!(
                scope.outcome(),
                Err(ResourceLimit::CompileExpansion),
                "{pattern}"
            );
        }
    }
    #[test]
    fn reentrant_scope_restores_parent() {
        let outer = Scope::enter(CompileLimits {
            work: 1,
            ..CompileLimits::default()
        });
        {
            let inner = Scope::enter(CompileLimits {
                work: 0,
                ..CompileLimits::default()
            });
            assert!(!charge(0, 1));
            assert!(inner.outcome().is_err());
        }
        assert!(outer.outcome().is_ok());
        assert!(charge(0, 1));
        assert!(!charge(0, 1));
    }
    #[test]
    fn unwind_restores_scope() {
        let _ = std::panic::catch_unwind(|| {
            let _scope = Scope::enter(CompileLimits::default());
            panic!("test unwind")
        });
        assert!(!active());
    }
    #[test]
    fn concurrent_scopes_are_isolated() {
        let threads: Vec<_> = (0..8)
            .map(|i| {
                std::thread::spawn(move || {
                    let scope = Scope::enter(CompileLimits {
                        work: i,
                        ..CompileLimits::default()
                    });
                    assert_eq!(charge(0, 4), i >= 4);
                    assert_eq!(scope.outcome().is_ok(), i >= 4);
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
    }
}
