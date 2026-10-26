use super::super::ZodStringRule;
use super::{MAX_NODES, ZodJsonSchema, ZodJsonUnsupported};
use serde_json::Value;

pub(super) const MAX_ADMISSION_BYTES: usize = 16 * 1024 * 1024;
const MAX_WORK_BYTES: usize = 32 * 1024 * 1024;

pub(super) struct Budget {
    pub track_origins: bool,
    pub nodes: usize,
    #[cfg(feature = "zod-regex")]
    pub regex_work: usize,
    bytes: usize,
}
impl Budget {
    pub fn new() -> Self {
        Self {
            track_origins: false,
            nodes: MAX_NODES,
            #[cfg(feature = "zod-regex")]
            regex_work: 400_000_000,
            bytes: MAX_WORK_BYTES,
        }
    }
    pub fn charge(&mut self, nodes: usize, bytes: usize) -> Result<(), ZodJsonUnsupported> {
        let next_nodes = self
            .nodes
            .checked_sub(nodes)
            .ok_or(ZodJsonUnsupported::EvaluationNodes)?;
        let next_bytes = self
            .bytes
            .checked_sub(bytes)
            .ok_or(ZodJsonUnsupported::EvaluationBytes)?;
        self.nodes = next_nodes;
        self.bytes = next_bytes;
        Ok(())
    }
}

pub(super) fn own_schema_bytes(schema: &ZodJsonSchema<'_>) -> usize {
    match schema {
        ZodJsonSchema::Enum(values) => values
            .iter()
            .fold(0usize, |sum, value| sum.saturating_add(value.len())),
        ZodJsonSchema::NativeEnum(entries) => entries.iter().fold(0usize, |sum, entry| {
            sum.saturating_add(entry.name.len())
                .saturating_add(match entry.value {
                    super::ZodJsonEnumValue::String(value) => value.len(),
                    super::ZodJsonEnumValue::Number(_) => 32,
                })
        }),
        ZodJsonSchema::DiscriminatedUnion { discriminator, .. } => discriminator.len(),
        ZodJsonSchema::Literal(literal) => literal.bytes(),
        ZodJsonSchema::CheckedString(checks) => checks
            .iter()
            .fold(0usize, |bytes, check| bytes.saturating_add(check.bytes())),
        ZodJsonSchema::String(checks) => checks.iter().fold(0usize, |bytes, check| {
            bytes
                .saturating_add(check.message.map_or(0, str::len))
                .saturating_add(match check.rule {
                    ZodStringRule::StartsWith(value)
                    | ZodStringRule::EndsWith(value)
                    | ZodStringRule::Includes { needle: value, .. } => value.len(),
                    ZodStringRule::Jwt { algorithm } => algorithm.map_or(0, str::len),
                    _ => 0,
                })
        }),
        ZodJsonSchema::CheckedArray { checks, .. } => checks.iter().fold(0usize, |bytes, check| {
            bytes.saturating_add(check.message.map_or(0, str::len))
        }),
        ZodJsonSchema::Number(checks) | ZodJsonSchema::StrictNumber(checks) => {
            checks.iter().fold(0usize, |bytes, check| {
                bytes.saturating_add(check.message.map_or(0, str::len))
            })
        }
        ZodJsonSchema::Object(fields)
        | ZodJsonSchema::StrictObject(fields)
        | ZodJsonSchema::ObjectWithPolicy { fields, .. } => {
            fields.iter().fold(0usize, |bytes, field| {
                bytes.saturating_add(field.name.len())
            })
        }
        _ => 0,
    }
}

pub(super) fn evaluation_bytes(
    input: Option<&Value>,
    schema: &ZodJsonSchema<'_>,
    budget: &mut Budget,
) -> Result<usize, ZodJsonUnsupported> {
    let mut input_bytes = || {
        input
            .map(|value| super::admit_value_counted(value, Some(&mut *budget)))
            .transpose()
            .map(|size| size.map_or(0, |(_, bytes)| bytes))
    };
    Ok(match schema {
        // Escaping may expand each input byte sixfold. Both the intermediate
        // JSON rendering and final diagnostic are charged before allocation.
        ZodJsonSchema::Literal(literal) => input_bytes()?
            .saturating_mul(3)
            .saturating_add(literal.bytes().saturating_mul(12))
            .saturating_add(128),
        ZodJsonSchema::CheckedString(checks) => input_bytes()?
            .saturating_add(own_schema_bytes(schema))
            .saturating_add(checks.len().saturating_add(1).saturating_mul(128)),
        ZodJsonSchema::String(checks) => input_bytes()?
            .saturating_mul(checks.len().saturating_add(2))
            .saturating_add(own_schema_bytes(schema))
            .saturating_add(checks.len().saturating_add(1).saturating_mul(128)),
        ZodJsonSchema::Number(checks) | ZodJsonSchema::StrictNumber(checks) => input_bytes()?
            .saturating_mul(3)
            .saturating_add(own_schema_bytes(schema))
            .saturating_add(checks.len().saturating_add(1).saturating_mul(128)),
        ZodJsonSchema::Boolean | ZodJsonSchema::StrictBoolean => {
            input_bytes()?.saturating_mul(4).saturating_add(128)
        }
        ZodJsonSchema::Tuple { .. }
        | ZodJsonSchema::Record { .. }
        | ZodJsonSchema::Null
        | ZodJsonSchema::Undefined
        | ZodJsonSchema::Void
        | ZodJsonSchema::Never => 128,
        ZodJsonSchema::Unknown => input_bytes()?.saturating_mul(3),
        ZodJsonSchema::Array { checks, .. } => checks.len().saturating_add(1).saturating_mul(128),
        ZodJsonSchema::CheckedArray { checks, .. } => own_schema_bytes(schema)
            .saturating_add(checks.len().saturating_add(1).saturating_mul(128)),
        ZodJsonSchema::Object(fields)
        | ZodJsonSchema::StrictObject(fields)
        | ZodJsonSchema::ObjectWithPolicy { fields, .. } => own_schema_bytes(schema)
            .saturating_mul(
                usize::try_from(fields.len().max(1).ilog2())
                    .unwrap_or(usize::MAX)
                    .saturating_add(2),
            ),
        _ => 0,
    })
}

pub(super) fn number_bytes(number: &serde_json::Number) -> Result<usize, ZodJsonUnsupported> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let next = self.0.saturating_add(bytes.len());
            if next > MAX_ADMISSION_BYTES {
                return Err(std::io::Error::other("numeric admission byte limit"));
            }
            self.0 = next;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, number).map_err(|_| ZodJsonUnsupported::InputBytes)?;
    if !number.as_f64().is_some_and(f64::is_finite) {
        return Err(ZodJsonUnsupported::NonFiniteNumber);
    }
    Ok(counter.0)
}

pub(super) fn clone_json(value: &Value) -> Result<Value, ZodJsonUnsupported> {
    Ok(match value {
        Value::Number(number) => Value::Number(
            serde_json::Number::from_f64(
                number.as_f64().ok_or(ZodJsonUnsupported::NonFiniteNumber)?,
            )
            .ok_or(ZodJsonUnsupported::NonFiniteNumber)?,
        ),
        Value::Array(values) => {
            Value::Array(values.iter().map(clone_json).collect::<Result<_, _>>()?)
        }
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), clone_json(value)?)))
                .collect::<Result<_, ZodJsonUnsupported>>()?,
        ),
        other => other.clone(),
    })
}
