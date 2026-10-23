//! Unicode16 default case mapping, pinned independently of Rust's Unicode data.
use super::{Budget, ZodJsonUnsupported};
#[path = "zod_string_case_tables.rs"]
mod tables;

fn contains(value: char, ranges: &[(u32, u32)]) -> bool {
    let value = u32::from(value);
    let index = ranges.partition_point(|&(lo, _)| lo <= value);
    index > 0 && value <= ranges[index - 1].1
}

#[derive(Clone, Copy)]
enum Mapping {
    Original(char),
    Mapped(&'static str),
}
impl Mapping {
    fn len(self) -> usize {
        match self {
            Self::Original(value) => value.len_utf8(),
            Self::Mapped(value) => value.len(),
        }
    }
    fn append(self, output: &mut String) {
        match self {
            Self::Original(value) => output.push(value),
            Self::Mapped(value) => output.push_str(value),
        }
    }
}
struct Mappings<'a> {
    input: std::str::Chars<'a>,
    preceding_cased: bool,
    upper: bool,
}
impl<'a> Mappings<'a> {
    fn new(input: &'a str, upper: bool) -> Self {
        Self {
            input: input.chars(),
            preceding_cased: false,
            upper,
        }
    }
}
impl Iterator for Mappings<'_> {
    type Item = Mapping;
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.input.next()?;
        let final_sigma = !self.upper
            && value == '\u{3a3}'
            && self.preceding_cased
            && !self
                .input
                .clone()
                .find(|&ch| !contains(ch, tables::CASE_IGNORABLE))
                .is_some_and(|ch| contains(ch, tables::CASED));
        if !contains(value, tables::CASE_IGNORABLE) {
            self.preceding_cased = contains(value, tables::CASED);
        }
        if final_sigma {
            return Some(Mapping::Mapped("\u{3c2}"));
        }
        let table = if self.upper {
            tables::UPPER
        } else {
            tables::LOWER
        };
        Some(
            table
                .binary_search_by_key(&u32::from(value), |&(cp, _)| cp)
                .map_or(Mapping::Original(value), |index| {
                    Mapping::Mapped(table[index].1)
                }),
        )
    }
}

pub(super) fn convert(
    input: &str,
    upper: bool,
    budget: &mut Budget,
) -> Result<String, ZodJsonUnsupported> {
    // Two mapping passes plus sigma lookahead. Every case-ignorable run can
    // be visited by at most one preceding sigma in each pass (sigma itself is
    // not ignorable). Fixed Unicode tables make property lookup bounded.
    budget.charge(0, input.len().saturating_mul(8))?;
    let length = Mappings::new(input, upper).try_fold(0usize, |length, mapping| {
        length
            .checked_add(mapping.len())
            .ok_or(ZodJsonUnsupported::EvaluationBytes)
    })?;
    budget.charge(0, length)?;
    let mut output = String::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| ZodJsonUnsupported::EvaluationBytes)?;
    for mapping in Mappings::new(input, upper) {
        mapping.append(&mut output);
    }
    debug_assert_eq!(length, output.len());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode16_expansions_context_and_no_normalization() {
        for (input, upper, expected) in [
            ("ß", true, "SS"),
            ("İ", false, "i\u{307}"),
            ("ΟΣ", false, "ος"),
            ("ΟΣΑ", false, "οσα"),
            ("AΣ\u{345}", false, "aς\u{345}"),
            ("AΣ\u{345}A", false, "aσ\u{345}a"),
            ("e\u{301}", true, "E\u{301}"),
            ("\u{1c89}", false, "\u{1c8a}"),
        ] {
            assert_eq!(convert(input, upper, &mut Budget::new()).unwrap(), expected);
        }
    }
    #[test]
    fn repeated_transform_work_is_bounded_before_output_allocation() {
        let input = "ß".repeat(100_000);
        let mut budget = Budget::new();
        let mut rejected = false;
        for _ in 0..100 {
            if matches!(
                convert(&input, true, &mut budget),
                Err(ZodJsonUnsupported::EvaluationBytes)
            ) {
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        // Long ignorable runs around sigma are linear and produce exact context.
        let input = format!("AΣ{}A", "\u{345}".repeat(100_000));
        assert_eq!(
            convert(&input, false, &mut Budget::new()).unwrap(),
            format!("aσ{}a", "\u{345}".repeat(100_000))
        );
    }
}
