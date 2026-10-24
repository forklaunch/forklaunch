//! Scratch bounded execution API. No existing unlimited API is implicitly limited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceLimit {
    Input,
    Work,
    Stack,
    Lookaround,
    CompileInput,
    CompileDepth,
    CompileExpansion,
    CompileArithmetic,
}
#[derive(Debug, Clone, Copy)]
pub struct MatchLimits {
    pub input_units: usize,
    pub work: usize,
    pub stack_bytes: usize,
    pub lookaround_depth: usize,
}
impl Default for MatchLimits {
    fn default() -> Self {
        Self {
            input_units: 1 << 20,
            work: 10_000_000,
            stack_bytes: 8 << 20,
            lookaround_depth: 64,
        }
    }
}
#[derive(Debug)]
pub(crate) struct MatchBudget {
    pub remaining: usize,
    pub stack_bytes: usize,
    pub depth_limit: usize,
    pub depth: usize,
    pub parent_bytes: usize,
    pub cost: usize,
    pub exhausted: Option<ResourceLimit>,
}
impl MatchBudget {
    pub fn fail(&mut self, limit: ResourceLimit) -> bool {
        self.exhausted.get_or_insert(limit);
        false
    }
    pub fn charge(&mut self, bytes: usize) -> bool {
        if self.exhausted.is_some() {
            return false;
        }
        if bytes > self.stack_bytes {
            return self.fail(ResourceLimit::Stack);
        }
        match self.remaining.checked_sub(self.cost) {
            Some(left) => {
                self.remaining = left;
                true
            }
            None => self.fail(ResourceLimit::Work),
        }
    }
}

/// Conservative admission budgets, not a wall-clock timeout or allocator quota.
#[derive(Debug, Clone, Copy)]
pub struct CompileLimits {
    pub pattern_units: usize,
    pub nesting: usize,
    pub groups: usize,
    pub allocation_units: u64,
    pub work: u64,
}
impl Default for CompileLimits {
    fn default() -> Self {
        Self {
            pattern_units: 4096,
            nesting: 32,
            groups: 128,
            allocation_units: 128 << 20,
            work: 10_000_000,
        }
    }
}
#[derive(Debug)]
pub enum CompileFailure {
    Syntax(crate::Error),
    Resource(ResourceLimit),
}
impl From<ResourceLimit> for CompileFailure {
    fn from(value: ResourceLimit) -> Self {
        Self::Resource(value)
    }
}

// Structural preflight only. Allocation/work uses the scoped in-path ledger.
#[cfg(feature = "utf16")]
pub(crate) fn admit_compile(
    pattern: &[u16],
    flags: crate::Flags,
    limits: CompileLimits,
) -> Result<(), ResourceLimit> {
    if pattern.len() > limits.pattern_units {
        return Err(ResourceLimit::CompileInput);
    }
    if !crate::compile_ledger::charge(0, pattern.len()) {
        return Err(ResourceLimit::CompileExpansion);
    }
    let mut groups = 0usize;
    let mut nesting = 0usize;
    let mut classes = 0usize;
    let mut alternatives = 0usize;
    let mut i = 0;
    while i < pattern.len() {
        let c = pattern[i];
        i += 1;
        if c == 92 {
            i = i.saturating_add(1);
            continue;
        }
        if c == 91 {
            if classes == 0 || flags.unicode_sets {
                classes = classes
                    .checked_add(1)
                    .ok_or(ResourceLimit::CompileArithmetic)?;
            }
        } else if c == 93 && classes > 0 {
            classes -= 1;
        } else if classes == 0 {
            match c {
                40 => {
                    groups = groups
                        .checked_add(1)
                        .ok_or(ResourceLimit::CompileArithmetic)?;
                    nesting = nesting
                        .checked_add(1)
                        .ok_or(ResourceLimit::CompileArithmetic)?;
                }
                41 => nesting = nesting.saturating_sub(1),
                124 => {
                    alternatives = alternatives
                        .checked_add(1)
                        .ok_or(ResourceLimit::CompileArithmetic)?;
                }
                123 => {
                    let mut cursor = i;
                    let mut number = 0usize;
                    while let Some(&digit) = pattern.get(cursor) {
                        if (48..=57).contains(&digit) {
                            number = number
                                .checked_mul(10)
                                .and_then(|n| n.checked_add(usize::from(digit - 48)))
                                .ok_or(ResourceLimit::CompileArithmetic)?;
                        } else if digit == 44 {
                            number = 0;
                        } else {
                            break;
                        }
                        cursor += 1;
                    }
                }
                _ => {}
            }
        }
        if nesting.saturating_add(classes) > limits.nesting.min(48) {
            return Err(ResourceLimit::CompileDepth);
        }
        if groups > limits.groups || alternatives > 128 {
            return Err(ResourceLimit::CompileExpansion);
        }
    }
    Ok(())
}

/// Exact element-count admission for the unoptimized emitter. The parser's
/// independent admission still needs its own allocation proof.
#[derive(Clone, Copy)]
pub(crate) struct EmitPlan {
    pub instructions: usize,
    pub brackets: usize,
    pub groups: usize,
    pub pending: usize,
    pub allocation_bytes: usize,
}
pub(crate) fn admit_ir(
    root: &crate::ir::Node,
    limits: CompileLimits,
) -> Result<EmitPlan, ResourceLimit> {
    use crate::{codepointset::Interval, insn::Insn, ir::Node, types::BracketContents};
    let fail = ResourceLimit::CompileExpansion;
    let mut pending = Vec::new();
    if core::mem::size_of::<(&Node, usize)>() as u64 > limits.allocation_units {
        return Err(fail);
    }
    pending.push((root, 0usize));
    let mut plan = EmitPlan {
        instructions: 0,
        brackets: 0,
        groups: 0,
        pending: 1,
        allocation_bytes: 0,
    };
    let mut nodes = 0usize;
    let mut intervals = 0usize;
    let mut names = 0usize;
    let mut payload = 0usize;
    let mut work = limits.work;
    while let Some((node, depth)) = pending.pop() {
        work = work.checked_sub(1).ok_or(fail)?;
        if depth > 256 {
            return Err(ResourceLimit::CompileDepth);
        }
        nodes = nodes.checked_add(1).ok_or(fail)?;
        let (insns, children) = match node {
            Node::Empty => (0, 0),
            Node::Cat(children) => {
                payload = payload
                    .checked_add(
                        children
                            .capacity()
                            .checked_sub(children.len())
                            .ok_or(fail)?
                            .checked_mul(core::mem::size_of::<Node>())
                            .ok_or(fail)?,
                    )
                    .ok_or(fail)?;
                (0, children.len())
            }
            Node::Alt(..) => (2, 2),
            Node::CaptureGroup { name, .. } => {
                plan.groups = plan.groups.checked_add(1).ok_or(fail)?;
                names = names
                    .checked_add(name.as_ref().map_or(0, |s| s.len()))
                    .ok_or(fail)?;
                (2, 1)
            }
            Node::LookaroundAssertion { .. } => (2, 1),
            Node::Loop {
                enclosed_groups, ..
            } => (
                2 + usize::from(
                    enclosed_groups
                        .end
                        .checked_sub(enclosed_groups.start)
                        .ok_or(ResourceLimit::CompileArithmetic)?,
                ),
                1,
            ),
            Node::Loop1CharBody { .. } => (1, 1),
            Node::Bracket(contents) => {
                plan.brackets = plan.brackets.checked_add(1).ok_or(fail)?;
                intervals = intervals
                    .checked_add(contents.cps.interval_capacity())
                    .and_then(|n| n.checked_add(contents.cps.intervals().len()))
                    .ok_or(fail)?;
                (1, 0)
            }
            Node::ByteSequence(bytes) => {
                payload = payload.checked_add(bytes.capacity()).ok_or(fail)?;
                (bytes.len().div_ceil(crate::insn::MAX_BYTE_SEQ_LENGTH), 0)
            }
            Node::ByteSet(bytes) => {
                payload = payload.checked_add(bytes.capacity()).ok_or(fail)?;
                (1, 0)
            }
            Node::CharSet(chars) => {
                payload = payload
                    .checked_add(
                        chars
                            .capacity()
                            .checked_mul(core::mem::size_of::<u32>())
                            .ok_or(fail)?,
                    )
                    .ok_or(fail)?;
                (1, 0)
            }
            _ => (1, 0),
        };
        plan.instructions = plan.instructions.checked_add(insns).ok_or(fail)?;
        if plan.instructions > u32::MAX as usize {
            return Err(ResourceLimit::CompileArithmetic);
        }
        let count = pending.len().checked_add(children).ok_or(fail)?;
        plan.pending = plan.pending.max(count);
        let bytes = nodes
            .checked_mul(core::mem::size_of::<Node>())
            .and_then(|n| n.checked_add(payload))
            .and_then(|n| {
                n.checked_add(
                    plan.instructions
                        .checked_mul(core::mem::size_of::<Insn>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    plan.brackets
                        .checked_mul(core::mem::size_of::<BracketContents>())?,
                )
            })
            .and_then(|n| n.checked_add(intervals.checked_mul(core::mem::size_of::<Interval>())?))
            .and_then(|n| n.checked_add(plan.groups.checked_mul(core::mem::size_of::<Box<str>>())?))
            .and_then(|n| n.checked_add(names.checked_mul(2)?))
            .and_then(|n| {
                n.checked_add(
                    count
                        .checked_mul(core::mem::size_of::<(&Node, usize)>())?
                        .checked_mul(4)?,
                )
            })
            .ok_or(fail)?;
        if bytes as u64 > limits.allocation_units {
            return Err(fail);
        }
        plan.allocation_bytes = plan.allocation_bytes.max(bytes);
        pending.try_reserve_exact(children).map_err(|_| fail)?;
        match node {
            Node::Cat(children) => pending.extend(children.iter().map(|child| (child, depth + 1))),
            Node::Alt(left, right) => {
                pending.push((left, depth + 1));
                pending.push((right, depth + 1));
            }
            Node::CaptureGroup { contents, .. } | Node::LookaroundAssertion { contents, .. } => {
                pending.push((contents, depth + 1))
            }
            Node::Loop { loopee, .. } | Node::Loop1CharBody { loopee, .. } => {
                pending.push((loopee, depth + 1))
            }
            _ => {}
        }
    }
    // One finish marker per branch/group can coexist with each pending node.
    plan.pending = nodes
        .checked_mul(2)
        .and_then(|n| n.checked_add(1))
        .ok_or(fail)?;
    // Stack dispatch, instruction emission, copied bracket intervals and names.
    let emission_work = plan
        .pending
        .checked_add(plan.instructions)
        .and_then(|n| n.checked_add(intervals))
        .and_then(|n| n.checked_add(names))
        .and_then(|n| n.checked_add(plan.groups))
        .ok_or(fail)?;
    work.checked_sub(emission_work as u64).ok_or(fail)?;
    Ok(plan)
}

#[cfg(all(test, feature = "utf16"))]
mod compile_admission_tests {
    use super::*;
    use crate::ir::Node;
    #[test]
    fn counts_retained_payload_capacity_even_when_empty() {
        let limit = CompileLimits {
            allocation_units: 512,
            ..CompileLimits::default()
        };
        for node in [
            Node::ByteSet(Vec::with_capacity(4096)),
            Node::CharSet(Vec::with_capacity(1024)),
            Node::ByteSequence(Vec::with_capacity(4096)),
            Node::Cat(Vec::with_capacity(128)),
        ] {
            assert!(matches!(
                admit_ir(&node, limit),
                Err(ResourceLimit::CompileExpansion)
            ));
        }
    }
    #[test]
    fn rejects_invalid_internal_capture_range_without_overflow() {
        let node = Node::Loop {
            loopee: Box::new(Node::Empty),
            quant: crate::ir::Quantifier {
                min: 0,
                max: None,
                greedy: true,
            },
            enclosed_groups: 2..1,
        };
        assert!(matches!(
            admit_ir(&node, CompileLimits::default()),
            Err(ResourceLimit::CompileArithmetic)
        ));
    }
}
