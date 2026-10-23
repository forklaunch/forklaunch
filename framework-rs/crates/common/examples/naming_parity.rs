//! JSON output for hex-encoded UTF-8 input lines; no JavaScript runtime coercions.
use forklaunch_common::{
    JsonField, capitalize, get_transformation_info, hash_string, is_valid_identifier, json_object,
    json_string, json_string_array, remove_leading_numbers, remove_leading_separators,
    remove_non_alphanumeric, split_by_separators, to_camel_case_identifier, to_pretty_camel_case,
    to_valid_identifier, uncapitalize,
};
use forklaunch_common::{
    open_api_compliant_path, remove_double_leading_slash, remove_trailing_slash,
    sanitize_path_slashes,
};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let bytes = (0..line.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&line[index..index + 2], 16).unwrap())
            .collect();
        let input = String::from_utf8(bytes).unwrap();
        let info = get_transformation_info(&input);
        let details = json_object([
            JsonField::new("original", json_string(&info.original)),
            JsonField::new(
                "withoutLeadingSeparators",
                json_string(&info.without_leading_separators),
            ),
            JsonField::new("parts", json_string_array(&info.parts)),
            JsonField::new("camelCase", json_string(&info.camel_case)),
            JsonField::new("prettyCamelCase", json_string(&info.pretty_camel_case)),
            JsonField::new(
                "withoutLeadingNumbers",
                json_string(&info.without_leading_numbers),
            ),
            JsonField::new(
                "prettyCamelCaseWithoutLeadingNumbers",
                json_string(&info.pretty_camel_case_without_leading_numbers),
            ),
            JsonField::new("isValid", info.is_valid.to_string()),
        ]);
        println!(
            "{}",
            json_object([
                JsonField::new(
                    "removeLeadingSeparators",
                    json_string(&remove_leading_separators(&input))
                ),
                JsonField::new(
                    "splitBySeparators",
                    json_string_array(&split_by_separators(&input))
                ),
                JsonField::new(
                    "removeNonAlphanumeric",
                    json_string(&remove_non_alphanumeric(&input))
                ),
                JsonField::new(
                    "removeLeadingNumbers",
                    json_string(&remove_leading_numbers(&input))
                ),
                JsonField::new("capitalize", json_string(&capitalize(&input))),
                JsonField::new("uncapitalize", json_string(&uncapitalize(&input))),
                JsonField::new(
                    "toCamelCaseIdentifier",
                    json_string(&to_camel_case_identifier(&input))
                ),
                JsonField::new(
                    "toPrettyCamelCase",
                    json_string(&to_pretty_camel_case(&input))
                ),
                JsonField::new(
                    "toValidIdentifier",
                    json_string(&to_valid_identifier(&input))
                ),
                JsonField::new("isValidIdentifier", is_valid_identifier(&input).to_string()),
                JsonField::new("getTransformationInfo", details),
                JsonField::new("hashString", hash_string(&input).to_string()),
                JsonField::new(
                    "openApiCompliantPath",
                    json_string(&open_api_compliant_path(&input))
                ),
                JsonField::new(
                    "removeTrailingSlash",
                    json_string(&remove_trailing_slash(&input))
                ),
                JsonField::new(
                    "removeDoubleLeadingSlash",
                    json_string(&remove_double_leading_slash(&input))
                ),
                JsonField::new(
                    "sanitizePathSlashes",
                    json_string(&sanitize_path_slashes(&input))
                ),
            ])
        );
    }
}
