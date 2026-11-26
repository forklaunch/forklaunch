//! Node's default query-string value projection; independent of native wire maps.
use serde_json::{Map, Value, map::Entry};
use std::collections::BTreeMap;

/// Parse the default `ForkLaunch` Express (`simple`) query-string profile.
///
/// Consumes at most 1,000 ampersand-delimited segments, including empty segments.
/// Repeated names produce arrays of strings. Brackets, dots, and prototype-named
/// keys remain literal data. Percent escapes decode as UTF-8 with replacement;
/// malformed escapes remain literal. `raw` excludes the leading question mark.
///
/// This pure projection does not mutate native request maps or interpret schemas.
/// Extended/custom Express query parsers, JavaScript object prototypes/key order,
/// and inputs containing unpaired UTF-16 surrogates are outside this string API.
/// [`parse_typescript_simple_query_ordered`] returns the same fields in the
/// order a TypeScript handler enumerates them.
#[must_use]
pub fn parse_typescript_simple_query(raw: Option<&str>) -> BTreeMap<String, Value> {
    parse_typescript_simple_query_ordered(raw)
        .into_iter()
        .collect()
}

/// [`parse_typescript_simple_query`] with the member order a TypeScript handler
/// sees on `req.query`: canonical array-index names ascending, then every other
/// name in first-occurrence order. Values and limits are identical.
#[must_use]
pub fn parse_typescript_simple_query_ordered(raw: Option<&str>) -> Map<String, Value> {
    let mut fields = Map::new();
    for segment in raw.unwrap_or_default().split('&').take(1_000) {
        if segment.is_empty() {
            continue;
        }
        let (key, value) = segment.split_once('=').unwrap_or((segment, ""));
        let value = Value::String(decode(value));
        match fields.entry(decode(key)) {
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                Value::Array(values) => values.push(value),
                previous => {
                    let first = std::mem::replace(previous, Value::Null);
                    *previous = Value::Array(vec![first, value]);
                }
            },
        }
    }
    let index = |key: &str| {
        key.parse::<u32>()
            .ok()
            .filter(|index| *index != u32::MAX && index.to_string() == key)
    };
    if fields.keys().any(|key| index(key).is_some()) {
        let mut ordered: Vec<_> = fields.into_iter().collect();
        ordered.sort_by_key(|(key, _)| index(key).map_or((1, 0), |index| (0, index)));
        fields = ordered.into_iter().collect();
    }
    fields
}
fn decode(value: &str) -> String {
    let value = value.replace('+', " ");
    percent_encoding::percent_decode_str(&value)
        .decode_utf8_lossy()
        .into_owned()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn literal_keys_duplicates_and_invalid_utf8_are_preserved() {
        assert_eq!(
            serde_json::to_value(parse_typescript_simple_query(Some(
                "a[]=1&a[]=2&__proto__=x&=y&&x=%E0%A4%A&z=%ff+%20"
            )))
            .unwrap(),
            json!({"a[]":["1","2"],"__proto__":"x","":"y","x":"�%A","z":"�  "})
        );
        assert!(parse_typescript_simple_query(None).is_empty());
    }
    #[test]
    fn ordered_projection_matches_node_member_order() {
        let raw = Some("b=1&a=2&10=3&2=4&b=5&01=6");
        let ordered = parse_typescript_simple_query_ordered(raw);
        assert_eq!(
            ordered.keys().collect::<Vec<_>>(),
            ["2", "10", "b", "a", "01"]
        );
        assert_eq!(ordered["b"], json!(["1", "5"]));
        assert_eq!(
            ordered.into_iter().collect::<BTreeMap<_, _>>(),
            parse_typescript_simple_query(raw)
        );
    }
    #[test]
    fn empty_segments_count_towards_node_default_limit() {
        assert!(
            parse_typescript_simple_query(Some(&format!("{}last=x", "&".repeat(1_000)))).is_empty()
        );
        assert_eq!(
            parse_typescript_simple_query(Some(&format!("{}last=x", "&".repeat(999))))["last"],
            "x"
        );
    }
}
