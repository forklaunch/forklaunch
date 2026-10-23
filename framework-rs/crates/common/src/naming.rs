//! String-input equivalents of the published TypeScript naming utilities.
fn separator(ch: char) -> bool {
    matches!(ch, ' ' | '-' | '_' | '.' | '/' | ':')
}
#[must_use]
pub fn remove_leading_separators(input: &str) -> String {
    input.trim_start_matches(separator).to_owned()
}
#[must_use]
pub fn split_by_separators(input: &str) -> Vec<String> {
    input
        .split(separator)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}
#[must_use]
pub fn remove_non_alphanumeric(input: &str) -> String {
    input.chars().filter(char::is_ascii_alphanumeric).collect()
}
#[must_use]
pub fn remove_leading_numbers(input: &str) -> String {
    input
        .trim_start_matches(|ch: char| ch.is_ascii_digit())
        .to_owned()
}
fn first_case(input: &str, upper: bool) -> String {
    let Some(first) = input.chars().next() else {
        return String::new();
    };
    // JS charAt(0) selects one UTF-16 code unit. A leading surrogate is unchanged.
    if first.len_utf16() == 2 {
        return input.to_owned();
    }
    let mut result: String = if upper {
        first.to_uppercase().collect()
    } else {
        first.to_lowercase().collect()
    };
    result.push_str(&input[first.len_utf8()..]);
    result
}
#[must_use]
pub fn capitalize(input: &str) -> String {
    first_case(input, true)
}
#[must_use]
pub fn uncapitalize(input: &str) -> String {
    first_case(input, false)
}
fn join(parts: &[String], pretty: bool) -> String {
    let mut output = String::new();
    let mut first = true;
    for part in parts {
        let part = remove_non_alphanumeric(&if pretty {
            part.to_lowercase()
        } else {
            part.clone()
        });
        if part.is_empty() {
            continue;
        }
        output.push_str(&if first {
            uncapitalize(&part)
        } else {
            capitalize(&part)
        });
        first = false;
    }
    output
}
fn convert(input: &str, pretty: bool) -> String {
    let result = remove_leading_numbers(&join(&split_by_separators(input), pretty));
    if result.is_empty() {
        input.to_owned()
    } else {
        result
    }
}
#[must_use]
pub fn to_camel_case_identifier(input: &str) -> String {
    convert(input, false)
}
#[must_use]
pub fn to_pretty_camel_case(input: &str) -> String {
    convert(input, true)
}
#[must_use]
pub fn to_valid_identifier(input: &str) -> String {
    to_camel_case_identifier(input)
}
#[must_use]
pub fn is_valid_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || matches!(ch, '_' | '$'))
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'))
}
/// Intermediate results retain empty transformations, unlike the public converters
/// which fall back to their original input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformationInfo {
    pub original: String,
    pub without_leading_separators: String,
    pub parts: Vec<String>,
    pub camel_case: String,
    pub pretty_camel_case: String,
    pub without_leading_numbers: String,
    pub pretty_camel_case_without_leading_numbers: String,
    pub is_valid: bool,
}
#[must_use]
pub fn get_transformation_info(input: &str) -> TransformationInfo {
    let without_leading_separators = remove_leading_separators(input);
    let parts = split_by_separators(&without_leading_separators);
    let camel_case = join(&parts, false);
    let pretty_camel_case = join(&parts, true);
    let without_leading_numbers = remove_leading_numbers(&camel_case);
    let pretty_camel_case_without_leading_numbers = remove_leading_numbers(&pretty_camel_case);
    let is_valid = is_valid_identifier(&without_leading_numbers);
    TransformationInfo {
        original: input.to_owned(),
        without_leading_separators,
        parts,
        camel_case,
        pretty_camel_case,
        without_leading_numbers,
        pretty_camel_case_without_leading_numbers,
        is_valid,
    }
}
/// Matches TypeScript's signed 32-bit, UTF-16 string hash. This is not cryptographic.
#[must_use]
pub fn hash_string(input: &str) -> i32 {
    input.encode_utf16().fold(0_i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}
