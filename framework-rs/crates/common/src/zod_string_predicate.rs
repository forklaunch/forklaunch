//! Literal UTF-16 predicates, including surrogate fragments and IEEE positions.
//! Compilation admits at most 4096 needle units. KMP uses bounded needle storage
//! and scans input once without allocating an encoded copy. Input remains UTF-8;
//! a needle may match one half of an astral scalar's UTF-16 representation.

/// Literal predicate selected by the trusted schema profile.
#[derive(Clone, Copy, Debug)]
pub enum PredicateKind {
    StartsWith,
    EndsWith,
    Includes,
}

/// Immutable admitted literal and its linear-time search prefix table.
pub struct StringPredicate {
    kind: PredicateKind,
    units: Vec<u16>,
    prefix: Vec<usize>,
    position: f64,
}
impl StringPredicate {
    /// Returns `None` before allocation if the needle exceeds 4096 UTF-16 units.
    #[must_use]
    pub fn new(kind: PredicateKind, units: &[u16], position: f64) -> Option<Self> {
        if units.len() > 4096 {
            return None;
        }
        let mut prefix = vec![0; units.len()];
        let mut matched = 0;
        for index in 1..units.len() {
            while matched > 0 && units[index] != units[matched] {
                matched = prefix[matched - 1];
            }
            if units[index] == units[matched] {
                matched += 1;
            }
            prefix[index] = matched;
        }
        Some(Self {
            kind,
            units: units.to_vec(),
            prefix,
            position,
        })
    }

    /// JavaScript code-unit comparisons over the representable UTF-8 input domain.
    /// Work is linear in input units per rule; callers bound aggregate rule count.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn accepts(&self, input: &str) -> bool {
        if self.units.is_empty() {
            return true;
        }
        if matches!(self.kind, PredicateKind::StartsWith) {
            return input
                .encode_utf16()
                .take(self.units.len())
                .eq(self.units.iter().copied());
        }
        let start = if self.position.is_nan() || self.position < 0.0 {
            0.0
        } else {
            self.position.trunc()
        };
        let mut matched = 0;
        let mut ends_here = false;
        for (index, unit) in input.encode_utf16().enumerate() {
            if matches!(self.kind, PredicateKind::Includes) && (index as f64) < start {
                continue;
            }
            ends_here = false;
            while matched > 0 && unit != self.units[matched] {
                matched = self.prefix[matched - 1];
            }
            if unit == self.units[matched] {
                matched += 1;
            }
            if matched == self.units.len() {
                if matches!(self.kind, PredicateKind::Includes) {
                    return true;
                }
                ends_here = true;
                matched = self.prefix[matched - 1];
            }
        }
        ends_here
    }
}

#[cfg(test)]
mod tests {
    use super::{PredicateKind::*, StringPredicate};
    #[test]
    fn surrogate_fragments_positions_and_empty_needles() {
        assert!(
            StringPredicate::new(StartsWith, &[0xd83d], 0.0)
                .unwrap()
                .accepts("😀")
        );
        assert!(
            StringPredicate::new(EndsWith, &[0xde00], 0.0)
                .unwrap()
                .accepts("😀")
        );
        assert!(
            StringPredicate::new(Includes, &[0xde00], 1.5)
                .unwrap()
                .accepts("😀")
        );
        assert!(
            !StringPredicate::new(Includes, &[0xd83d], 1.0)
                .unwrap()
                .accepts("😀")
        );
        for position in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY, -0.5] {
            assert!(
                StringPredicate::new(Includes, &[], position)
                    .unwrap()
                    .accepts("")
            );
        }
        assert!(
            StringPredicate::new(Includes, &[97], f64::NAN)
                .unwrap()
                .accepts("a")
        );
        assert!(
            !StringPredicate::new(Includes, &[97], f64::INFINITY)
                .unwrap()
                .accepts("a")
        );
    }
    #[test]
    fn overlapping_long_prefixes_and_needle_admission() {
        let mut needle = vec![97; 4095];
        needle.push(98);
        let text = "a".repeat(100_000) + "b";
        assert!(
            StringPredicate::new(Includes, &needle, 0.0)
                .unwrap()
                .accepts(&text)
        );
        assert!(
            StringPredicate::new(EndsWith, &needle, 0.0)
                .unwrap()
                .accepts(&text)
        );
        assert!(
            !StringPredicate::new(EndsWith, &needle, 0.0)
                .unwrap()
                .accepts(&(text + "a"))
        );
        assert!(StringPredicate::new(Includes, &vec![0; 4097], 0.0).is_none());
    }
}
