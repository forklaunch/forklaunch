//! Checked regex keyword. Its boolean adapter is private to a checked closure.
use forklaunch_common::zod_regex::{RegexCompileLimits, RegexMatchLimits, ZodRegex};
use jsonschema::{Keyword, ValidationError, paths::Location};
use serde_json::{Map, Value};
use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

pub(super) const KEYWORD: &str = "x-forklaunch-zod-regex";

/// Logical quotas: conservative requested storage/work, not RSS or a timeout.
/// Each rule debits its entire configured allowance before execution.
#[derive(Clone, Copy, Debug)]
pub struct RegexProfile {
    pub compile: RegexCompileLimits,
    pub matching: RegexMatchLimits,
    pub rules: usize,
    pub source_units: usize,
    pub compile_storage: u64,
    pub compile_work: u64,
    pub evaluation_work: usize,
}
impl Default for RegexProfile {
    fn default() -> Self {
        Self {
            compile: RegexCompileLimits {
                allocation_units: 16 << 20,
                ..RegexCompileLimits::default()
            },
            matching: RegexMatchLimits::default(),
            rules: 64,
            source_units: 64 * 4096,
            compile_storage: 512 << 20,
            compile_work: 320_000_000,
            evaluation_work: 400_000_000,
        }
    }
}

#[derive(Clone, Copy)]
struct Frame {
    remaining: usize,
    failed: bool,
}
thread_local! { static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) }; }
/// Replaces the current frame by value; nested validation and panic unwind restore it.
pub(super) struct Scope(Option<Frame>);
impl Scope {
    pub(super) fn enter(work: usize) -> Self {
        Self(FRAME.with(|frame| {
            frame.replace(Some(Frame {
                remaining: work,
                failed: false,
            }))
        }))
    }
    pub(super) fn failed() -> bool {
        FRAME.with(|frame| frame.borrow().as_ref().is_none_or(|frame| frame.failed))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        FRAME.with(|frame| {
            frame.replace(self.0.take());
        });
    }
}
fn debit(amount: usize) -> bool {
    FRAME.with(|frame| {
        let mut frame = frame.borrow_mut();
        let Some(frame) = frame.as_mut() else {
            return false;
        };
        if frame.failed {
            return false;
        }
        if let Some(left) = frame.remaining.checked_sub(amount) {
            frame.remaining = left;
            true
        } else {
            frame.failed = true;
            false
        }
    })
}
fn fail() {
    FRAME.with(|frame| {
        if let Some(frame) = frame.borrow_mut().as_mut() {
            frame.failed = true;
        }
    });
}

pub(super) struct Compiler {
    profile: RegexProfile,
    budget: Mutex<Admission>,
}
struct Admission {
    rules: usize,
    units: usize,
    bytes: u64,
    work: u64,
}
impl Compiler {
    pub(super) fn new(profile: RegexProfile) -> Arc<Self> {
        Arc::new(Self {
            profile,
            budget: Mutex::new(Admission {
                rules: profile.rules,
                units: profile.source_units,
                bytes: profile.compile_storage,
                work: profile.compile_work,
            }),
        })
    }
    pub(super) fn compile<'a>(
        &self,
        _: &'a Map<String, Value>,
        value: &'a Value,
        _: Location,
    ) -> Result<Box<dyn for<'i> Keyword<'i>>, ValidationError<'a>> {
        let object = value
            .as_object()
            .filter(|v| v.len() == 2)
            .ok_or_else(invalid)?;
        if object.get("version").and_then(Value::as_u64) != Some(1) {
            return Err(invalid());
        }
        let rules = object
            .get("rules")
            .and_then(Value::as_array)
            .filter(|v| v.len() <= self.profile.rules)
            .ok_or_else(invalid)?;
        let result_bytes = rules
            .len()
            .checked_mul(std::mem::size_of::<ZodRegex>())
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or_else(invalid)?;
        {
            let mut budget = self.budget.lock().map_err(|_| invalid())?;
            budget.bytes = budget.bytes.checked_sub(result_bytes).ok_or_else(invalid)?;
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(rules.len())
            .map_err(|_| invalid())?;
        for rule in rules {
            let rule = rule
                .as_object()
                .filter(|v| v.len() == 2)
                .ok_or_else(invalid)?;
            let units = rule
                .get("sourceUnits")
                .and_then(Value::as_array)
                .filter(|v| v.len() <= self.profile.compile.pattern_units)
                .ok_or_else(invalid)?;
            let flags = rule
                .get("flags")
                .and_then(Value::as_str)
                .filter(|v| v.len() <= 8)
                .ok_or_else(invalid)?;
            // Aggregate admission precedes pattern allocation and engine compilation.
            {
                let mut budget = self.budget.lock().map_err(|_| invalid())?;
                budget.rules = budget.rules.checked_sub(1).ok_or_else(invalid)?;
                budget.units = budget.units.checked_sub(units.len()).ok_or_else(invalid)?;
                let source_bytes = units
                    .len()
                    .checked_mul(std::mem::size_of::<u16>())
                    .and_then(|bytes| u64::try_from(bytes).ok())
                    .ok_or_else(invalid)?;
                let storage = self
                    .profile
                    .compile
                    .allocation_units
                    .checked_add(source_bytes)
                    .ok_or_else(invalid)?;
                budget.bytes = budget.bytes.checked_sub(storage).ok_or_else(invalid)?;
                budget.work = budget
                    .work
                    .checked_sub(self.profile.compile.work)
                    .ok_or_else(invalid)?;
            }
            let mut source = Vec::new();
            source
                .try_reserve_exact(units.len())
                .map_err(|_| invalid())?;
            for unit in units {
                source.push(
                    unit.as_u64()
                        .and_then(|v| u16::try_from(v).ok())
                        .ok_or_else(invalid)?,
                );
            }
            result.push(
                ZodRegex::compile(&source, flags, self.profile.compile)
                    .map_err(|error| ValidationError::custom(error.to_string()))?,
            );
        }
        Ok(Box::new(Rules {
            rules: result,
            limits: self.profile.matching,
        }))
    }
}
fn invalid<'a>() -> ValidationError<'a> {
    ValidationError::custom("invalid or over-budget x-forklaunch-zod-regex profile")
}
struct Rules {
    rules: Vec<ZodRegex>,
    limits: RegexMatchLimits,
}
impl Rules {
    fn accepts(&self, value: &Value) -> bool {
        let Some(value) = value.as_str() else {
            return true;
        };
        for rule in &self.rules {
            // Include the bounded UTF16 admission scan and encoding in logical work.
            let Some(cost) = self
                .limits
                .input_units
                .checked_mul(2)
                .and_then(|v| v.checked_add(1))
                .and_then(|v| v.checked_add(self.limits.work))
            else {
                fail();
                return false;
            };
            if !debit(cost) {
                return false;
            }
            match rule.test(value, self.limits) {
                Ok(true) => {}
                Ok(false) => return false,
                Err(_) => {
                    fail();
                    return false;
                }
            }
        }
        true
    }
}
impl<'i> Keyword<'i> for Rules {
    fn is_valid(&self, value: &'i Value) -> bool {
        self.accepts(value)
    }
    fn validate(&self, value: &'i Value) -> Result<(), ValidationError<'i>> {
        if self.accepts(value) {
            Ok(())
        } else {
            Err(ValidationError::custom(
                "string does not satisfy regex checks",
            ))
        }
    }
}
