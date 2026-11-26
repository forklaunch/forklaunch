//! Reserved exact Zod check assertions; never reinterpret ordinary schema keywords.
//! Input strings are Rust UTF-8 strings, so lone JavaScript UTF-16 surrogates are
//! outside this representation domain. Numbers use their finite IEEE-754 value.
use forklaunch_common::zod_string_predicate::{PredicateKind, StringPredicate};
use forklaunch_common::{zod_cardinality::ZodCardinality, zod_multiple_of::ZodMultipleOf};
use jsonschema::{Keyword, ValidationError, paths::Location};
use serde_json::{Map, Value};

pub(super) const KEYWORD: &str = "x-forklaunch-zod-checks";
struct Checks {
    string_predicates: Vec<StringPredicate>,
    utf16_minimum: Vec<ZodCardinality>,
    utf16_maximum: Vec<ZodCardinality>,
    array_minimum: Vec<ZodCardinality>,
    array_maximum: Vec<ZodCardinality>,
    minimum: Option<u64>,
    maximum: Option<u64>,
    divisors: Vec<ZodMultipleOf>,
    formats: Vec<Format>,
    time_precisions: Vec<Option<u16>>,
    date_times: Vec<DateTimeRule>,
    jwt_algorithms: Vec<Option<String>>,
}
struct DateTimeRule {
    precision: Option<u16>,
    local: bool,
    offset: bool,
}
fn jwt_algorithms(value: Option<&Value>) -> Result<Vec<Option<String>>, ValidationError<'static>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .filter(|v| v.len() <= 10_000)
        .ok_or_else(invalid)?
        .iter()
        .map(|value| {
            if value.is_null() {
                Ok(None)
            } else {
                value
                    .as_str()
                    .map(|value| Some(value.to_owned()))
                    .ok_or_else(invalid)
            }
        })
        .collect()
}
fn date_times(value: Option<&Value>) -> Result<Vec<DateTimeRule>, ValidationError<'static>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|v| v.len() <= 10_000)
        .ok_or_else(invalid)?;
    values
        .iter()
        .map(|value| {
            let rule = value
                .as_object()
                .filter(|v| v.len() == 3)
                .ok_or_else(invalid)?;
            let precision = rule.get("precision").ok_or_else(invalid)?;
            let precision = if precision.is_null() {
                None
            } else {
                Some(
                    bound(Some(precision))?
                        .filter(|v| *v <= 1000)
                        .and_then(|v| u16::try_from(v).ok())
                        .ok_or_else(invalid)?,
                )
            };
            Ok(DateTimeRule {
                precision,
                local: rule
                    .get("local")
                    .and_then(Value::as_bool)
                    .ok_or_else(invalid)?,
                offset: rule
                    .get("offset")
                    .and_then(Value::as_bool)
                    .ok_or_else(invalid)?,
            })
        })
        .collect()
}
enum Format {
    Email,
    Uuid,
    Cuid,
    Cuid2,
    Ulid,
    Nanoid,
    Base64,
    Base64Url,
    Date,
    Duration,
    Emoji,
    Ip(Option<u8>),
    Cidr(Option<u8>),
    #[cfg(feature = "zod-uri")]
    Uri,
}
impl Format {
    fn accepts(&self, value: &str) -> bool {
        match self {
            Self::Ip(version) => forklaunch_common::zod_string_format::ip(value, *version),
            Self::Cidr(version) => forklaunch_common::zod_string_format::cidr(value, *version),
            Self::Emoji => forklaunch_common::zod_string_format::emoji(value),
            Self::Email => forklaunch_common::zod_string_format::email(value),
            Self::Uuid => forklaunch_common::zod_string_format::uuid(value),
            Self::Cuid => forklaunch_common::zod_string_format::cuid(value),
            Self::Cuid2 => forklaunch_common::zod_string_format::cuid2(value),
            Self::Ulid => forklaunch_common::zod_string_format::ulid(value),
            Self::Nanoid => forklaunch_common::zod_string_format::nanoid(value),
            Self::Base64 => forklaunch_common::zod_string_format::base64(value),
            Self::Base64Url => forklaunch_common::zod_string_format::base64url(value),
            Self::Duration => forklaunch_common::zod_string_format::duration(value),
            Self::Date => forklaunch_common::zod_string_format::date(value),
            #[cfg(feature = "zod-uri")]
            Self::Uri => ada_url::Url::can_parse(value, None),
        }
    }
}
fn invalid<'a>() -> ValidationError<'a> {
    ValidationError::custom("invalid x-forklaunch-zod-checks version, fields or limits")
}
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn bound(value: Option<&Value>) -> Result<Option<u64>, ValidationError<'static>> {
    value
        .map(|value| {
            value
                .as_f64()
                .filter(|value| {
                    value.is_finite()
                        && *value >= 0.0
                        && *value <= 9_007_199_254_740_991.0
                        && value.fract() == 0.0
                })
                .map(|value| value as u64)
                .ok_or_else(invalid)
        })
        .transpose()
}
fn time_precisions(value: Option<&Value>) -> Result<Vec<Option<u16>>, ValidationError<'static>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|v| v.len() <= 10_000)
        .ok_or_else(invalid)?;
    values
        .iter()
        .map(|value| {
            if value.is_null() {
                return Ok(None);
            }
            bound(Some(value))?
                .filter(|v| *v <= 1000)
                .and_then(|v| u16::try_from(v).ok())
                .map(Some)
                .ok_or_else(invalid)
        })
        .collect()
}
fn formats(raw_formats: Option<&Vec<Value>>) -> Result<Vec<Format>, ValidationError<'static>> {
    let mut formats = Vec::new();
    for value in raw_formats.into_iter().flatten() {
        formats.push(match value.as_str() {
            Some("ip") => Format::Ip(None),
            Some("ipv4") => Format::Ip(Some(4)),
            Some("ipv6") => Format::Ip(Some(6)),
            Some("cidr") => Format::Cidr(None),
            Some("cidrv4") => Format::Cidr(Some(4)),
            Some("cidrv6") => Format::Cidr(Some(6)),
            Some("emoji") => Format::Emoji,
            Some("email") => Format::Email,
            Some("uuid") => Format::Uuid,
            Some("cuid") => Format::Cuid,
            Some("cuid2") => Format::Cuid2,
            Some("ulid") => Format::Ulid,
            Some("nanoid") => Format::Nanoid,
            Some("base64") => Format::Base64,
            Some("base64url") => Format::Base64Url,
            Some("duration") => Format::Duration,
            Some("date") => Format::Date,
            #[cfg(feature = "zod-uri")]
            Some("uri") => Format::Uri,
            #[cfg(not(feature = "zod-uri"))]
            Some("uri") => {
                return Err(ValidationError::custom(
                    "URI checks require the forklaunch-core zod-uri feature",
                ));
            }
            _ => return Err(invalid()),
        });
    }
    Ok(formats)
}
fn numeric_value(value: &Value) -> Result<f64, ValidationError<'static>> {
    match value {
        Value::Number(value) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(invalid),
        Value::String(value) if value == "NaN" => Ok(f64::NAN),
        Value::String(value) if value == "Infinity" => Ok(f64::INFINITY),
        Value::String(value) if value == "-Infinity" => Ok(f64::NEG_INFINITY),
        _ => Err(invalid()),
    }
}
fn raw_rules(value: Option<&Value>) -> Result<Option<&Vec<Value>>, ValidationError<'static>> {
    value
        .map(|value| {
            value
                .as_array()
                .filter(|values| values.len() <= 10_000)
                .ok_or_else(invalid)
        })
        .transpose()
}
fn cardinalities(
    values: Option<&Vec<Value>>,
) -> Result<Vec<ZodCardinality>, ValidationError<'static>> {
    values
        .into_iter()
        .flatten()
        .map(|value| numeric_value(value).map(ZodCardinality::new))
        .collect()
}
fn string_predicates(
    value: Option<&Value>,
) -> Result<Vec<StringPredicate>, ValidationError<'static>> {
    let rules = raw_rules(value)?;
    let mut remaining = 262_144usize;
    // Preflight every source and field before allocating needle/prefix storage.
    for rule in rules.into_iter().flatten() {
        let fields = rule.as_object().ok_or_else(invalid)?;
        let kind = fields
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if !matches!(kind, "startsWith" | "endsWith" | "includes")
            || fields.len() != if kind == "includes" { 3 } else { 2 }
        {
            return Err(invalid());
        }
        let units = fields
            .get("sourceUnits")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if units.len() > 4096 {
            return Err(invalid());
        }
        remaining = remaining.checked_sub(units.len()).ok_or_else(invalid)?;
        for unit in units {
            bound(Some(unit))?
                .filter(|unit| u16::try_from(*unit).is_ok())
                .ok_or_else(invalid)?;
        }
        if kind == "includes" {
            numeric_value(fields.get("position").ok_or_else(invalid)?)?;
        }
    }
    rules
        .into_iter()
        .flatten()
        .map(|rule| {
            let fields = rule.as_object().ok_or_else(invalid)?;
            let kind = match fields.get("kind").and_then(Value::as_str) {
                Some("startsWith") => PredicateKind::StartsWith,
                Some("endsWith") => PredicateKind::EndsWith,
                Some("includes") => PredicateKind::Includes,
                _ => return Err(invalid()),
            };
            let units = fields
                .get("sourceUnits")
                .and_then(Value::as_array)
                .ok_or_else(invalid)?
                .iter()
                .map(|unit| {
                    bound(Some(unit))?
                        .and_then(|unit| u16::try_from(unit).ok())
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let position = fields
                .get("position")
                .map(numeric_value)
                .transpose()?
                .unwrap_or(0.0);
            StringPredicate::new(kind, &units, position).ok_or_else(invalid)
        })
        .collect()
}
pub(super) fn compile<'a>(
    _: &'a Map<String, Value>,
    value: &'a Value,
    _: Location,
) -> Result<Box<dyn for<'i> Keyword<'i>>, ValidationError<'a>> {
    let fields = value.as_object().ok_or_else(invalid)?;
    if fields.get("version").and_then(Value::as_f64) != Some(1.0)
        || fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "version"
                    | "stringPredicates"
                    | "utf16MinLengths"
                    | "utf16MaxLengths"
                    | "utf16MinLength"
                    | "utf16MaxLength"
                    | "multipleOf"
                    | "arrayMinItems"
                    | "arrayMaxItems"
                    | "formats"
                    | "timePrecisions"
                    | "dateTimes"
                    | "jwtAlgorithms"
            )
        })
    {
        return Err(invalid());
    }
    // Validate aggregate rule counts before allocating any per-rule vectors.
    let mut count = 0usize;
    for key in [
        "stringPredicates",
        "utf16MinLengths",
        "utf16MaxLengths",
        "formats",
        "multipleOf",
        "arrayMinItems",
        "arrayMaxItems",
        "timePrecisions",
        "dateTimes",
        "jwtAlgorithms",
    ] {
        count = count
            .checked_add(raw_rules(fields.get(key))?.map_or(0, Vec::len))
            .filter(|count| *count <= 10_000)
            .ok_or_else(invalid)?;
    }
    let string_predicates = string_predicates(fields.get("stringPredicates"))?;
    let jwt_algorithms = jwt_algorithms(fields.get("jwtAlgorithms"))?;
    let date_times = date_times(fields.get("dateTimes"))?;
    let time_precisions = time_precisions(fields.get("timePrecisions"))?;
    let minimum = bound(fields.get("utf16MinLength"))?;
    let maximum = bound(fields.get("utf16MaxLength"))?;
    let raw_formats = raw_rules(fields.get("formats"))?;
    let raw_divisors = raw_rules(fields.get("multipleOf"))?;
    let raw_minimum = raw_rules(fields.get("arrayMinItems"))?;
    let raw_maximum = raw_rules(fields.get("arrayMaxItems"))?;
    let utf16_minimum = cardinalities(raw_rules(fields.get("utf16MinLengths"))?)?;
    let utf16_maximum = cardinalities(raw_rules(fields.get("utf16MaxLengths"))?)?;
    let formats = formats(raw_formats)?;
    let divisors = raw_divisors
        .into_iter()
        .flatten()
        .map(|value| numeric_value(value).map(ZodMultipleOf::new))
        .collect::<Result<_, _>>()?;
    let array_minimum = cardinalities(raw_minimum)?;
    let array_maximum = cardinalities(raw_maximum)?;
    Ok(Box::new(Checks {
        string_predicates,
        utf16_minimum,
        utf16_maximum,
        array_minimum,
        array_maximum,
        minimum,
        maximum,
        divisors,
        formats,
        time_precisions,
        date_times,
        jwt_algorithms,
    }))
}
impl<'i> Keyword<'i> for Checks {
    fn is_valid(&self, instance: &'i Value) -> bool {
        if let Some(values) = instance.as_array() {
            let length = values.len() as u64;
            if !self
                .array_minimum
                .iter()
                .all(|bound| bound.accepts_minimum(length))
                || !self
                    .array_maximum
                    .iter()
                    .all(|bound| bound.accepts_maximum(length))
            {
                return false;
            }
        }
        if let Some(value) = instance.as_str() {
            if !self
                .string_predicates
                .iter()
                .all(|rule| rule.accepts(value))
            {
                return false;
            }
            if !self.jwt_algorithms.iter().all(|algorithm| {
                forklaunch_common::zod_string_format::jwt(value, algorithm.as_deref())
            }) {
                return false;
            }
            if !self.date_times.iter().all(|rule| {
                forklaunch_common::zod_string_format::datetime(
                    value,
                    rule.precision,
                    rule.local,
                    rule.offset,
                )
            }) {
                return false;
            }
            if !self
                .time_precisions
                .iter()
                .all(|p| forklaunch_common::zod_string_format::time(value, *p))
            {
                return false;
            }
            if !self.formats.iter().all(|format| format.accepts(value)) {
                return false;
            }
            let length = value.encode_utf16().count() as u64;
            if !self
                .utf16_minimum
                .iter()
                .all(|bound| bound.accepts_minimum(length))
                || !self
                    .utf16_maximum
                    .iter()
                    .all(|bound| bound.accepts_maximum(length))
                || self.minimum.is_some_and(|minimum| length < minimum)
                || self.maximum.is_some_and(|maximum| length > maximum)
            {
                return false;
            }
        }
        if !self.divisors.is_empty()
            && let Some(value) = instance.as_number()
        {
            return value
                .as_f64()
                .filter(|value| value.is_finite())
                .is_some_and(|value| self.divisors.iter().all(|divisor| divisor.accepts(value)));
        }
        true
    }
    fn validate(&self, instance: &'i Value) -> Result<(), ValidationError<'i>> {
        if self.is_valid(instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(
                "value violates exact Zod check assertions",
            ))
        }
    }
}

#[cfg(test)]
mod predicate_resource_tests {
    use super::string_predicates;
    use serde_json::json;
    #[test]
    fn keyword_source_budget_is_independent_of_outer_schema_node_limit() {
        let rule = json!({"kind":"includes","sourceUnits":vec![97;4096],"position":0});
        assert_eq!(
            string_predicates(Some(&json!(vec![rule.clone(); 64])))
                .unwrap()
                .len(),
            64
        );
        assert!(string_predicates(Some(&json!(vec![rule; 65]))).is_err());
    }
}
