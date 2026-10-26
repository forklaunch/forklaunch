//! Zod v3 ordered cardinality comparisons over JavaScript numeric bounds.

/// A numeric bound, retaining every IEEE value including inert NaN.
/// This is an explicit Zod capability, not a standard JSON Schema bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZodCardinality(u64);
impl ZodCardinality {
    #[must_use]
    pub const fn new(bound: f64) -> Self {
        Self(bound.to_bits())
    }
    /// Length is compared as a JavaScript Number without capping the bound.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn accepts_minimum(self, length: u64) -> bool {
        (length as f64).partial_cmp(&f64::from_bits(self.0)) != Some(std::cmp::Ordering::Less)
    }
    /// NaN and positive infinity impose no upper bound.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn accepts_maximum(self, length: u64) -> bool {
        (length as f64).partial_cmp(&f64::from_bits(self.0)) != Some(std::cmp::Ordering::Greater)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ieee_cardinalities_are_not_capped_or_rounded_as_bounds() {
        for bound in [f64::NAN, 0.0, -0.0] {
            assert!(ZodCardinality::new(bound).accepts_minimum(0));
            assert!(ZodCardinality::new(bound).accepts_maximum(0));
        }
        assert!(!ZodCardinality::new(1.5).accepts_minimum(1));
        assert!(!ZodCardinality::new(1.5).accepts_maximum(2));
        assert!(!ZodCardinality::new(f64::MAX).accepts_minimum(u64::MAX));
        assert!(ZodCardinality::new(f64::MAX).accepts_maximum(u64::MAX));
        assert!(!ZodCardinality::new(f64::NEG_INFINITY).accepts_maximum(0));
        assert!(!ZodCardinality::new(f64::INFINITY).accepts_minimum(u64::MAX));
    }
}
