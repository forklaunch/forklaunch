//! Bounded, opt-in composition of the supported Zod JSON value operations.
use super::{
    ZodNumberCheck, ZodStringCheck, boolean, number, zod_number_checks, zod_string_checks,
};
use crate::ParseError;
use crate::zod_array::{ZodArrayCheck, ZodArrayStep, zod_array_checks, zod_array_shape};
use serde_json::{Map, Value};

#[path = "zod_json_issues.rs"]
mod issues;
#[path = "zod_json_resources.rs"]
mod resources;
use issues::Issue;
#[path = "zod_json_enum.rs"]
mod enum_leaf;
pub use enum_leaf::{ZodJsonEnumValue, ZodJsonNativeEnumEntry};
#[path = "zod_json_discriminated.rs"]
mod discriminated;
#[path = "zod_json_intersection.rs"]
mod intersection;
use intersection::Origin;
#[path = "zod_json_literal.rs"]
mod literal;
pub use literal::ZodJsonLiteral;
#[path = "zod_json_object.rs"]
mod object_policy;
#[path = "zod_json_ordered.rs"]
mod ordered;
#[path = "zod_json_record.rs"]
mod record;
#[path = "zod_json_regex.rs"]
mod regex;
#[path = "zod_json_tuple.rs"]
mod tuple;
use ordered::Order;
pub use ordered::{OrderedJsonDocument, OrderedJsonError};
pub use regex::ZodJsonStringCheck;
use resources::{
    Budget, MAX_ADMISSION_BYTES, clone_json, evaluation_bytes, number_bytes, own_schema_bytes,
};

const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 100_000;

/// Programmatic schema composition; this does not interpret JSON Schema or Zod.
/// Objects strip unknown fields unless `StrictObject` is explicitly selected.
/// Literal defaults apply only to absent input.
/// Ordered unions choose the first valid branch, otherwise the first dirty
/// branch. Empty and single-branch unions follow the source runtime behavior.
/// Strict unknown-key diagnostics use trusted own-key order; ordinary values
/// need an ordered document only when the selected diagnostic is ambiguous.
/// Record keys use the same finite schema grammar: wrappers/Unknown preserve
/// string inputs; container schemas abort. Arbitrary callbacks, symbols, and
/// non-JSON key outputs are not supported. Output serialization order is separate.
#[derive(Debug, Clone)]
pub enum ZodJsonSchema<'a> {
    String(Vec<ZodStringCheck<'a>>),
    /// Ordered allocating Unicode16 case transforms and scalar checks. Optional
    /// bounded regex assertions share the same sequence. Operational failures
    /// abort all branches; borrowed string helper signatures remain unchanged.
    CheckedString(Vec<ZodJsonStringCheck<'a>>),
    Boolean,
    /// Raw Zod boolean, without the `ForkLaunch` factory preprocessing.
    StrictBoolean,
    Number(Vec<ZodNumberCheck<'a>>),
    /// Raw Zod number; checks run only after a supplied numeric input passes type admission.
    StrictNumber(Vec<ZodNumberCheck<'a>>),
    Unknown,
    /// Accept only a supplied JSON null.
    Null,
    /// Accept only absent input (`None`), preserving omission in objects.
    Undefined,
    /// Accept absent input with the distinct Zod `void` diagnostics.
    Void,
    /// Reject every input, including absence.
    Never,
    Literal(ZodJsonLiteral<'a>),
    /// Raw Zod string enum, distinct from factory enum_ literal-union lowering.
    Enum(Vec<&'a str>),
    /// Own enum entries with source reverse-mapping filtering at parse time.
    NativeEnum(Vec<ZodJsonNativeEnumEntry<'a>>),
    /// Structurally register literal/enum/absence discriminator values and
    /// evaluate only the selected object. Registration never invokes callbacks.
    DiscriminatedUnion {
        discriminator: &'a str,
        options: Vec<Self>,
    },
    Optional(Box<Self>),
    Nullable(Box<Self>),
    Default {
        schema: Box<Self>,
        value: &'a Value,
    },
    Array {
        element: Box<Self>,
        checks: Vec<ZodArrayStep>,
    },
    /// Array checks with general numeric bounds and per-registration messages.
    /// Last registration of each kind wins; element evaluation still runs after
    /// dirty container checks and output is committed atomically.
    CheckedArray {
        element: Box<Self>,
        checks: Vec<ZodArrayCheck<'a>>,
    },
    /// Ordered dense JSON tuple. Every declared slot counts toward minimum
    /// length, including optional/default schemas. An optional rest validates
    /// all trailing values. Sparse/undefined JavaScript elements are excluded.
    Tuple {
        items: Vec<Self>,
        rest: Option<Box<Self>>,
    },
    Object(Vec<ZodJsonField<'a>>),
    /// Reject unknown keys in original ECMAScript own-key order.
    StrictObject(Vec<ZodJsonField<'a>>),
    /// Object unknown-key policy with an optional catchall. Only an unwrapped
    /// `Never` catchall is the source sentinel that restores `unknown` policy.
    ObjectWithPolicy {
        fields: Vec<ZodJsonField<'a>>,
        unknown: ZodJsonUnknownKeys,
        catchall: Option<Box<Self>>,
    },
    Union(Vec<Self>),
    /// Evaluate both branches and merge compatible transformed JSON outputs.
    Intersection {
        left: Box<Self>,
        right: Box<Self>,
    },
    /// Evaluate each own string key then its value. Successful scalar keys use
    /// JavaScript property-key conversion; transformed `__proto__` is omitted.
    /// Ordered documents retain last-wins collisions. Ordinary values defer
    /// unavailable diagnostic order, but ambiguous successful collisions fail.
    Record {
        key: Box<Self>,
        value: Box<Self>,
    },
}

/// The unknown-property policy when no active catchall schema is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZodJsonUnknownKeys {
    Strip,
    Strict,
    Passthrough,
}

/// A declared object property; duplicates and prototype-sensitive names are
/// rejected before evaluation. Integer-index names follow ECMAScript key order.
#[derive(Debug, Clone)]
pub struct ZodJsonField<'a> {
    pub name: &'a str,
    pub schema: ZodJsonSchema<'a>,
}

/// An explicit boundary failure, never disguised as a source validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZodJsonUnsupported {
    SchemaDepth,
    SchemaNodes,
    InputDepth,
    InputNodes,
    EvaluationNodes,
    SchemaBytes,
    InputBytes,
    EvaluationBytes,
    DuplicateField,
    PrototypeField,
    NonFiniteNumber,
    DiagnosticPath,
    ObjectOrderUnavailable,
    RecordKeyOutput,
    DuplicateEnumKey,
    DiscriminatorSchema,
    DuplicateDiscriminator,
    #[cfg(feature = "zod-regex")]
    RegexResource,
}

/// Successful absent input remains distinct from JSON null. Invalid results
/// never expose partially transformed values, including nested defaults.
#[derive(Debug, Clone, PartialEq)]
pub enum ZodJsonOutcome {
    Valid(Option<Value>),
    Invalid(Vec<ParseError>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Valid,
    Dirty,
    Aborted,
}
struct Parsed {
    finite_output: bool,
    merge_order_available: bool,
    origin: Origin,
    value: Option<Value>,
    errors: Vec<Issue>,
    status: Status,
}
impl Parsed {
    fn valid(value: Option<Value>) -> Self {
        Self {
            finite_output: true,
            merge_order_available: true,
            origin: Origin::default(),
            value,
            errors: Vec::new(),
            status: Status::Valid,
        }
    }
    fn invalid(errors: Vec<ParseError>, status: Status) -> Self {
        Self {
            finite_output: true,
            merge_order_available: true,
            origin: Origin::default(),
            value: None,
            errors: errors.into_iter().map(Issue::Leaf).collect(),
            status,
        }
    }
    fn absorb(
        &mut self,
        child: &mut Self,
        key: &str,
        budget: &mut Budget,
    ) -> Result<(), ZodJsonUnsupported> {
        self.finite_output &= child.finite_output;
        self.merge_order_available &= child.merge_order_available;
        if child.status == Status::Aborted {
            self.status = Status::Aborted;
        } else if child.status == Status::Dirty && self.status == Status::Valid {
            self.status = Status::Dirty;
        }
        for error in &mut child.errors {
            error.prepend(key, budget)?;
        }
        self.errors.append(&mut child.errors);
        Ok(())
    }
}

/// Evaluate the finite schema over JSON, with atomic transformed output.
/// Schema and input admission precede evaluation: maximum depth 64 and 100,000
/// nodes (including checks/default values), plus at most 100,000 schema-node
/// and check applications, including revisited input descendants, during evaluation. Aggregate input/schema strings,
/// keys, messages and defaults are admitted up to 16 MiB. Evaluation debits a
/// conservative 32 MiB byte-work allowance before clones, scalar transforms,
/// repeated checks, output keys and diagnostic path/message allocations.
/// These logical budgets do not measure allocator overhead or replace transport
/// body limits. This avoids recursive evaluation of
/// unbounded structures. Unknown fields are checked for resource bounds too.
/// Objects are ordinary JSON own-property values; prototype-sensitive declared
/// names are explicitly unsupported. Missing values can occur at roots/fields,
/// not as sparse array elements. All input/default numbers must project to finite
/// IEEE-754 values, including when arbitrary-precision serde features are unified.
/// Unknown outputs normalize numeric values to f64, matching JavaScript rounding;
/// raw spelling and Rust exact-integer accessors are not preserved in that output.
/// Numeric negative zero retains its IEEE sign (unlike JSON.stringify's text
/// projection); the string "-0" remains a string under Unknown.
/// Original input remains unchanged. Modes remain the enclosing route's policy.
/// # Errors
/// Returns an explicit unsupported boundary, distinct from `Invalid` diagnostics.
pub fn zod_json(
    input: Option<&Value>,
    schema: &ZodJsonSchema<'_>,
) -> Result<ZodJsonOutcome, ZodJsonUnsupported> {
    zod_json_at_path(input, schema, &[])
}

/// Evaluate without cloning input, prefixing raw issue paths before the actual
/// one-level union diagnostic formatter runs. Successful output is unchanged.
/// The trusted diagnostic path is admitted before evaluation: at most 64 segments
/// and 16 MiB, sharing the aggregate schema/input byte allowance. Repeated path
/// copies also consume the evaluation work budgets. This preserves the source
/// adapter's duplicated outer paths for union diagnostics.
/// # Errors
/// Returns [`ZodJsonUnsupported::DiagnosticPath`] for an oversized path, or the
/// same explicit unsupported boundaries as [`zod_json`].
pub fn zod_json_at_path(
    input: Option<&Value>,
    schema: &ZodJsonSchema<'_>,
    path: &[&str],
) -> Result<ZodJsonOutcome, ZodJsonUnsupported> {
    evaluate_document(input, schema, path, None, 0)
}

/// Evaluate immutable ordered JSON without reparsing original transport bytes.
/// Defaults supplied as ordinary values do not gain invented property order.
/// # Errors
/// Returns the same boundaries as [`zod_json_at_path`], including unavailable
/// order when strict unknown-key diagnostics require it.
pub fn zod_json_ordered_at_path(
    input: &OrderedJsonDocument,
    schema: &ZodJsonSchema<'_>,
    path: &[&str],
) -> Result<ZodJsonOutcome, ZodJsonUnsupported> {
    evaluate_document(
        Some(&input.value),
        schema,
        path,
        Some(&input.order),
        input.bytes,
    )
}

/// Evaluate an ordered document with an empty diagnostic path.
/// # Errors
/// See [`zod_json_ordered_at_path`].
pub fn zod_json_ordered(
    input: &OrderedJsonDocument,
    schema: &ZodJsonSchema<'_>,
) -> Result<ZodJsonOutcome, ZodJsonUnsupported> {
    zod_json_ordered_at_path(input, schema, &[])
}

fn evaluate_document(
    input: Option<&Value>,
    schema: &ZodJsonSchema<'_>,
    path: &[&str],
    order: Option<&Order>,
    metadata_bytes: usize,
) -> Result<ZodJsonOutcome, ZodJsonUnsupported> {
    if path.len() > MAX_DEPTH {
        return Err(ZodJsonUnsupported::DiagnosticPath);
    }
    let path_bytes = path
        .iter()
        .fold(0usize, |sum, part| sum.saturating_add(part.len()));
    if path_bytes > MAX_ADMISSION_BYTES {
        return Err(ZodJsonUnsupported::DiagnosticPath);
    }
    let (schema_bytes, track_origins, registrations) = admit_schema(schema)?;
    let schema_bytes = schema_bytes.saturating_add(path_bytes);
    if schema_bytes > MAX_ADMISSION_BYTES {
        return Err(ZodJsonUnsupported::SchemaBytes);
    }
    if let Some(input) = input
        && schema_bytes.saturating_add(admit_value(input)?.1.max(metadata_bytes))
            > MAX_ADMISSION_BYTES
    {
        return Err(ZodJsonUnsupported::InputBytes);
    }
    let mut remaining = Budget::new();
    remaining.track_origins = track_origins;
    remaining.charge(path.len(), path_bytes)?;
    if registrations {
        discriminated::admit(schema, &mut remaining)?;
    }
    let mut parsed = evaluate_ordered(input, schema, &mut remaining, order)?;
    if let Some(value) = &parsed.value {
        admit_value(value)?;
    }
    Ok(if parsed.status == Status::Valid {
        ZodJsonOutcome::Valid(parsed.value)
    } else {
        for segment in path.iter().rev() {
            for issue in &mut parsed.errors {
                issue.prepend(segment, &mut remaining)?;
            }
        }
        ZodJsonOutcome::Invalid(issues::format(parsed.errors, &mut remaining)?)
    })
}

fn admit_value(value: &Value) -> Result<(usize, usize), ZodJsonUnsupported> {
    admit_value_counted(value, None)
}

fn admit_value_counted(
    value: &Value,
    mut budget: Option<&mut Budget>,
) -> Result<(usize, usize), ZodJsonUnsupported> {
    let mut stack = vec![(value, 0)];
    let mut nodes = 0;
    let mut bytes = 0usize;
    while let Some((value, depth)) = stack.pop() {
        // The evaluator has already charged the root application. Every
        // revisited descendant is charged before inspection or child expansion.
        if depth > 0
            && let Some(budget) = &mut budget
        {
            budget.charge(1, 0)?;
        }
        if depth > MAX_DEPTH {
            return Err(ZodJsonUnsupported::InputDepth);
        }
        nodes += 1;
        bytes = bytes.saturating_add(match value {
            Value::String(value) => value.len(),
            Value::Number(value) => number_bytes(value)?,
            Value::Object(values) => values
                .keys()
                .fold(0usize, |sum, key| sum.saturating_add(key.len())),
            _ => 0,
        });
        if bytes > MAX_ADMISSION_BYTES {
            return Err(ZodJsonUnsupported::InputBytes);
        }
        let children = match value {
            Value::Array(v) => v.len(),
            Value::Object(v) => v.len(),
            _ => 0,
        };
        if nodes + stack.len() + children > MAX_NODES {
            return Err(ZodJsonUnsupported::InputNodes);
        }
        match value {
            Value::Array(values) => stack.extend(values.iter().map(|v| (v, depth + 1))),
            Value::Object(values) => stack.extend(values.values().map(|v| (v, depth + 1))),
            _ => {}
        }
    }
    Ok((nodes, bytes))
}

fn admit_schema(schema: &ZodJsonSchema<'_>) -> Result<(usize, bool, bool), ZodJsonUnsupported> {
    let mut intersections = false;
    let mut registrations = false;
    let mut stack = vec![(schema, 0)];
    let mut nodes = 0;
    let mut bytes = 0usize;
    while let Some((schema, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(ZodJsonUnsupported::SchemaDepth);
        }
        nodes += 1;
        registrations |= matches!(
            schema,
            ZodJsonSchema::NativeEnum(_) | ZodJsonSchema::DiscriminatedUnion { .. }
        );
        bytes = bytes.saturating_add(own_schema_bytes(schema));
        if bytes > MAX_ADMISSION_BYTES {
            return Err(ZodJsonUnsupported::SchemaBytes);
        }
        let extra = schema_children(schema);
        if nodes + stack.len() + extra > MAX_NODES {
            return Err(ZodJsonUnsupported::SchemaNodes);
        }
        match schema {
            ZodJsonSchema::Tuple { items, rest } => {
                stack.extend(items.iter().map(|item| (item, depth + 1)));
                if let Some(rest) = rest {
                    stack.push((rest, depth + 1));
                }
            }
            ZodJsonSchema::Intersection {
                left: key,
                right: value,
            }
            | ZodJsonSchema::Record { key, value } => {
                intersections |= matches!(schema, ZodJsonSchema::Intersection { .. });
                stack.push((key, depth + 1));
                stack.push((value, depth + 1));
            }
            ZodJsonSchema::Union(branches)
            | ZodJsonSchema::DiscriminatedUnion {
                options: branches, ..
            } => {
                stack.extend(branches.iter().map(|branch| (branch, depth + 1)));
            }
            ZodJsonSchema::Enum(values) => nodes += values.len(),
            ZodJsonSchema::NativeEnum(values) => nodes += values.len(),
            ZodJsonSchema::String(checks) => nodes += checks.len(),
            ZodJsonSchema::CheckedString(checks) => nodes += checks.len(),
            ZodJsonSchema::Number(checks) | ZodJsonSchema::StrictNumber(checks) => {
                nodes += checks.len();
            }
            ZodJsonSchema::Optional(inner) | ZodJsonSchema::Nullable(inner) => {
                stack.push((inner, depth + 1));
            }
            ZodJsonSchema::Default { schema, value } => {
                let size = admit_value(value)?;
                nodes += size.0;
                bytes = bytes.saturating_add(size.1);
                if bytes > MAX_ADMISSION_BYTES {
                    return Err(ZodJsonUnsupported::SchemaBytes);
                }
                if nodes > MAX_NODES {
                    return Err(ZodJsonUnsupported::SchemaNodes);
                }
                stack.push((schema, depth + 1));
            }
            ZodJsonSchema::Array { element, checks } => {
                nodes += checks.len();
                stack.push((element, depth + 1));
            }
            ZodJsonSchema::CheckedArray { element, checks } => {
                nodes += checks.len();
                stack.push((element, depth + 1));
            }
            ZodJsonSchema::Object(fields)
            | ZodJsonSchema::StrictObject(fields)
            | ZodJsonSchema::ObjectWithPolicy { fields, .. } => {
                if let ZodJsonSchema::ObjectWithPolicy {
                    catchall: Some(catchall),
                    ..
                } = schema
                {
                    stack.push((catchall, depth + 1));
                }
                let mut names = std::collections::BTreeSet::new();
                for field in fields {
                    if !names.insert(field.name) {
                        return Err(ZodJsonUnsupported::DuplicateField);
                    }
                    if prototype_name(field.name) {
                        return Err(ZodJsonUnsupported::PrototypeField);
                    }
                    stack.push((&field.schema, depth + 1));
                }
            }
            _ => {}
        }
    }
    Ok((bytes, intersections, registrations))
}

fn schema_children(schema: &ZodJsonSchema<'_>) -> usize {
    match schema {
        ZodJsonSchema::String(checks) => checks.len(),
        ZodJsonSchema::CheckedString(checks) => checks.len(),
        ZodJsonSchema::Number(checks) | ZodJsonSchema::StrictNumber(checks) => checks.len(),
        ZodJsonSchema::Array { checks, .. } => checks.len() + 1,
        ZodJsonSchema::CheckedArray { checks, .. } => checks.len() + 1,
        ZodJsonSchema::Object(fields) | ZodJsonSchema::StrictObject(fields) => fields.len(),
        ZodJsonSchema::ObjectWithPolicy {
            fields, catchall, ..
        } => fields.len() + usize::from(catchall.is_some()),
        ZodJsonSchema::Union(branches)
        | ZodJsonSchema::DiscriminatedUnion {
            options: branches, ..
        } => branches.len(),
        ZodJsonSchema::Enum(values) => values.len(),
        ZodJsonSchema::NativeEnum(values) => values.len(),
        ZodJsonSchema::Tuple { items, rest } => items.len() + usize::from(rest.is_some()),
        ZodJsonSchema::Record { .. } | ZodJsonSchema::Intersection { .. } => 2,
        ZodJsonSchema::Optional(_) | ZodJsonSchema::Nullable(_) | ZodJsonSchema::Default { .. } => {
            1
        }
        _ => 0,
    }
}

fn prototype_name(name: &str) -> bool {
    matches!(
        name,
        "__proto__"
            | "constructor"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
    )
}

#[cfg(test)]
fn evaluate(
    input: Option<&Value>,
    schema: &ZodJsonSchema<'_>,
    remaining: &mut Budget,
) -> Result<Parsed, ZodJsonUnsupported> {
    evaluate_ordered(input, schema, remaining, None)
}

fn schema_check_count(schema: &ZodJsonSchema<'_>) -> usize {
    match schema {
        ZodJsonSchema::String(checks) => checks.len(),
        ZodJsonSchema::CheckedString(checks) => checks.len(),
        ZodJsonSchema::Number(checks) | ZodJsonSchema::StrictNumber(checks) => checks.len(),
        ZodJsonSchema::Array { checks, .. } => checks.len(),
        ZodJsonSchema::CheckedArray { checks, .. } => checks.len(),
        _ => 0,
    }
}

fn evaluate_ordered(
    input: Option<&Value>,
    schema: &ZodJsonSchema<'_>,
    remaining: &mut Budget,
    order: Option<&Order>,
) -> Result<Parsed, ZodJsonUnsupported> {
    let checks = schema_check_count(schema);
    remaining.charge(1 + checks, 0)?;
    let bytes = evaluation_bytes(input, schema, remaining)?;
    remaining.charge(0, bytes)?;
    Ok(match schema {
        ZodJsonSchema::Enum(values) => enum_leaf::strings(input, values, remaining)?,
        ZodJsonSchema::NativeEnum(entries) => {
            let values = enum_leaf::native_values(entries, true, remaining)?;
            enum_leaf::evaluate(input, &values, true, remaining)?
        }
        ZodJsonSchema::DiscriminatedUnion {
            discriminator,
            options,
        } => discriminated::evaluate(input, discriminator, options, order, remaining)?,
        ZodJsonSchema::Literal(literal) => parsed_literal(input, literal)?,
        ZodJsonSchema::Null => terminal(input, "null"),
        ZodJsonSchema::Undefined => terminal(input, "undefined"),
        ZodJsonSchema::Void => terminal(input, "void"),
        ZodJsonSchema::Never => terminal(input, "never"),
        ZodJsonSchema::Unknown => {
            let mut parsed = Parsed::valid(input.map(clone_json).transpose()?);
            if remaining.track_origins
                && let Some(input) = input
            {
                parsed.origin = Origin::borrowed(input, remaining)?;
            }
            parsed
        }
        ZodJsonSchema::Intersection { left, right } => {
            intersect(input, left, right, remaining, order)?
        }
        ZodJsonSchema::Optional(_) if input.is_none() => Parsed::valid(None),
        ZodJsonSchema::Nullable(_) if input == Some(&Value::Null) => {
            Parsed::valid(Some(Value::Null))
        }
        ZodJsonSchema::Optional(inner) | ZodJsonSchema::Nullable(inner) => {
            evaluate_ordered(input, inner, remaining, order)?
        }
        ZodJsonSchema::Default { schema, value } => {
            evaluate_ordered(input.or(Some(value)), schema, remaining, input.and(order))?
        }
        ZodJsonSchema::CheckedString(checks) => regex::evaluate(input, checks, remaining)?,
        ZodJsonSchema::String(checks) => parsed_string(input, checks, remaining),
        ZodJsonSchema::StrictBoolean if !matches!(input, Some(Value::Bool(_))) => {
            terminal(input, "boolean")
        }
        ZodJsonSchema::Boolean | ZodJsonSchema::StrictBoolean => match boolean(input) {
            Ok(value) => Parsed::valid(Some(Value::Bool(value))),
            Err(error) => Parsed::invalid(vec![error], Status::Aborted),
        },
        ZodJsonSchema::StrictNumber(_) if !matches!(input, Some(Value::Number(_))) => {
            terminal(input, "number")
        }
        ZodJsonSchema::Number(checks) | ZodJsonSchema::StrictNumber(checks) => {
            parsed_number(input, checks)?
        }
        ZodJsonSchema::Array { element, checks } => array(
            input,
            element,
            zod_array_shape(input, checks).err().unwrap_or_default(),
            remaining,
            order,
        )?,
        ZodJsonSchema::CheckedArray { element, checks } => array(
            input,
            element,
            zod_array_checks(input, checks).err().unwrap_or_default(),
            remaining,
            order,
        )?,
        ZodJsonSchema::Tuple { items, rest } => {
            tuple::evaluate(input, items, rest.as_deref(), remaining, order)?
        }
        ZodJsonSchema::Object(fields) => object(input, fields, remaining, order, false)?,
        ZodJsonSchema::StrictObject(fields) => object(input, fields, remaining, order, true)?,
        ZodJsonSchema::ObjectWithPolicy {
            fields,
            unknown,
            catchall,
        } => object_policy::evaluate(
            input,
            fields,
            remaining,
            order,
            *unknown,
            catchall.as_deref(),
        )?,
        ZodJsonSchema::Union(branches) => union(input, branches, remaining, order)?,
        ZodJsonSchema::Record { key, value } => {
            record::evaluate(input, key, value, remaining, order)?
        }
    })
}

fn parsed_number(
    input: Option<&Value>,
    checks: &[ZodNumberCheck<'_>],
) -> Result<Parsed, ZodJsonUnsupported> {
    Ok(match zod_number_checks(input, checks) {
        Ok(value) => Parsed::valid(Some(Value::Number(
            serde_json::Number::from_f64(value).ok_or(ZodJsonUnsupported::NonFiniteNumber)?,
        ))),
        Err(errors) => match number(input) {
            Ok(value) => {
                let mut parsed = Parsed::invalid(errors, Status::Dirty);
                parsed.finite_output = value.is_finite();
                parsed.value = serde_json::Number::from_f64(value).map(Value::Number);
                parsed
            }
            Err(_) => Parsed::invalid(errors, Status::Aborted),
        },
    })
}

fn parsed_literal(
    input: Option<&Value>,
    literal: &ZodJsonLiteral<'_>,
) -> Result<Parsed, ZodJsonUnsupported> {
    Ok(if literal.matches(input) {
        Parsed::valid(input.map(clone_json).transpose()?)
    } else {
        Parsed::invalid(
            vec![ParseError {
                path: Vec::new(),
                message: literal.diagnostic(),
            }],
            Status::Aborted,
        )
    })
}

fn parsed_string(
    input: Option<&Value>,
    checks: &[ZodStringCheck<'_>],
    remaining: &Budget,
) -> Parsed {
    match zod_string_checks(input, &[]) {
        Err(errors) => Parsed::invalid(errors, Status::Aborted),
        Ok(mut value) => {
            let mut length = value.encode_utf16().count() as u64;
            let mut errors = Vec::new();
            for check in checks {
                if let Some(error) =
                    super::strict_string::apply_check(&mut value, &mut length, *check)
                {
                    errors.push(error);
                }
            }
            let status = if errors.is_empty() {
                Status::Valid
            } else {
                Status::Dirty
            };
            let mut parsed = Parsed::invalid(errors, status);
            if status == Status::Valid || remaining.track_origins {
                parsed.value = Some(Value::String(value.to_owned()));
            }
            parsed
        }
    }
}

fn intersect(
    input: Option<&Value>,
    left: &ZodJsonSchema<'_>,
    right: &ZodJsonSchema<'_>,
    budget: &mut Budget,
    order: Option<&Order>,
) -> Result<Parsed, ZodJsonUnsupported> {
    let mut left = evaluate_ordered(input, left, budget, order)?;
    let mut right = evaluate_ordered(input, right, budget, order)?;
    left.errors.append(&mut right.errors);
    if left.status == Status::Aborted || right.status == Status::Aborted {
        left.status = Status::Aborted;
        left.value = None;
        left.origin = Origin::default();
        return Ok(left);
    }
    if !left.merge_order_available || !right.merge_order_available {
        left.status = Status::Aborted;
        left.value = None;
        left.origin = Origin::default();
        left.errors.push(Issue::UnorderedObject);
        return Ok(left);
    }
    if !left.finite_output || !right.finite_output {
        return Err(ZodJsonUnsupported::NonFiniteNumber);
    }
    let merged = intersection::merge(
        left.value.take(),
        std::mem::take(&mut left.origin),
        right.value,
        right.origin,
        budget,
    )?;
    if let Some((value, origin)) = merged {
        left.value = value;
        left.origin = origin;
        if right.status == Status::Dirty {
            left.status = Status::Dirty;
        }
    } else {
        budget.charge(1, 64)?;
        left.status = Status::Aborted;
        left.errors.push(Issue::Leaf(ParseError {
            path: Vec::new(),
            message: "Intersection results could not be merged".to_owned(),
        }));
    }
    Ok(left)
}

fn terminal(input: Option<&Value>, expected: &str) -> Parsed {
    if input.is_none() && matches!(expected, "undefined" | "void") {
        return Parsed::valid(None);
    }
    if expected == "null" && input == Some(&Value::Null) {
        return Parsed::valid(Some(Value::Null));
    }
    let message = input.map_or_else(
        || "Required".to_owned(),
        |value| {
            let received = match value {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            };
            format!("Expected {expected}, received {received}")
        },
    );
    Parsed::invalid(
        vec![ParseError {
            path: Vec::new(),
            message,
        }],
        Status::Aborted,
    )
}

fn union(
    input: Option<&Value>,
    branches: &[ZodJsonSchema<'_>],
    remaining: &mut Budget,
    order: Option<&Order>,
) -> Result<Parsed, ZodJsonUnsupported> {
    let mut first_dirty = None;
    let mut aborted = Vec::new();
    for branch in branches {
        remaining.charge(1, 16)?;
        let parsed = evaluate_ordered(input, branch, remaining, order)?;
        match parsed.status {
            Status::Valid => return Ok(parsed),
            Status::Dirty if first_dirty.is_none() => {
                first_dirty = Some(parsed);
                aborted.clear();
            }
            Status::Aborted if first_dirty.is_none() => aborted.push(parsed.errors),
            Status::Dirty | Status::Aborted => {}
        }
    }
    Ok(first_dirty.unwrap_or_else(|| Parsed {
        finite_output: true,
        merge_order_available: true,
        origin: Origin::default(),
        value: None,
        status: Status::Aborted,
        errors: vec![Issue::Union {
            path: Vec::new(),
            branches: aborted,
        }],
    }))
}

fn array(
    input: Option<&Value>,
    element: &ZodJsonSchema<'_>,
    errors: Vec<ParseError>,
    remaining: &mut Budget,
    order: Option<&Order>,
) -> Result<Parsed, ZodJsonUnsupported> {
    let Some(Value::Array(values)) = input else {
        return Ok(Parsed::invalid(errors, Status::Aborted));
    };
    let mut parsed = Parsed::invalid(errors, Status::Valid);
    if !parsed.errors.is_empty() {
        parsed.status = Status::Dirty;
    }
    let origin_count = if remaining.track_origins {
        values.len()
    } else {
        0
    };
    remaining.charge(
        0,
        origin_count.saturating_mul(std::mem::size_of::<Origin>()),
    )?;
    let mut origins = Vec::with_capacity(origin_count);
    let mut output = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let mut child = evaluate_ordered(
            Some(value),
            element,
            remaining,
            order.and_then(|o| o.index(index)),
        )?;
        parsed.absorb(&mut child, &index.to_string(), remaining)?;
        if let Some(value) = child.value {
            output.push(value);
            if remaining.track_origins {
                origins.push(child.origin);
            }
        }
    }
    if parsed.status == Status::Valid || (remaining.track_origins && parsed.status == Status::Dirty)
    {
        parsed.value = Some(Value::Array(output));
        parsed.origin = Origin::Array {
            source: None,
            children: origins,
        };
    }
    Ok(parsed)
}

fn object(
    input: Option<&Value>,
    fields: &[ZodJsonField<'_>],
    remaining: &mut Budget,
    order: Option<&Order>,
    strict: bool,
) -> Result<Parsed, ZodJsonUnsupported> {
    object_policy::evaluate(
        input,
        fields,
        remaining,
        order,
        if strict {
            ZodJsonUnknownKeys::Strict
        } else {
            ZodJsonUnknownKeys::Strip
        },
        None,
    )
}

fn reject_unknown(
    values: &Map<String, Value>,
    fields: &[ZodJsonField<'_>],
    order: Option<&Order>,
    parsed: &mut Parsed,
    remaining: &mut Budget,
) -> Result<(), ZodJsonUnsupported> {
    remaining.charge(values.len() + fields.len(), 0)?;
    let declared: std::collections::BTreeSet<_> = fields.iter().map(|field| field.name).collect();
    if values.keys().all(|key| declared.contains(key.as_str())) {
        return Ok(());
    }
    let mut keys: Vec<_> = if let Some(keys) = order.and_then(Order::keys) {
        keys.iter()
            .filter(|key| !declared.contains(key.as_str()))
            .collect()
    } else {
        let keys: Vec<_> = values
            .keys()
            .filter(|key| !declared.contains(key.as_str()))
            .collect();
        // Numeric indices have specified order independently of insertion. One
        // remaining string key is unambiguous; two need original own-key order.
        if keys.iter().filter(|key| index_key(key).0).count() > 1 {
            parsed.errors.push(Issue::UnorderedObject);
            if parsed.status == Status::Valid {
                parsed.status = Status::Dirty;
            }
            return Ok(());
        }
        keys
    };
    keys.sort_by_key(|key| index_key(key));
    let bytes = keys
        .iter()
        .filter(|key| !declared.contains(key.as_str()))
        .fold(64usize, |sum, key| {
            sum.saturating_add(key.len()).saturating_add(4)
        });
    remaining.charge(keys.len(), bytes.saturating_mul(2))?;
    let extras: Vec<_> = keys
        .iter()
        .filter(|key| !declared.contains(key.as_str()))
        .map(|key| format!("'{key}'"))
        .collect();
    parsed.errors.push(Issue::Leaf(ParseError {
        path: Vec::new(),
        message: format!("Unrecognized key(s) in object: {}", extras.join(", ")),
    }));
    if parsed.status == Status::Valid {
        parsed.status = Status::Dirty;
    }
    Ok(())
}

fn index_key(name: &str) -> (bool, u32) {
    match name.parse::<u32>() {
        Ok(value) if value < u32::MAX && value.to_string() == name => (false, value),
        _ => (true, 0),
    }
}

#[cfg(test)]
#[path = "zod_json_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "zod_json_ordered_tests.rs"]
mod ordered_tests;

#[cfg(test)]
#[path = "zod_json_record_tests.rs"]
mod record_tests;

#[cfg(test)]
#[path = "zod_json_absence_tests.rs"]
mod absence_tests;

#[cfg(test)]
#[path = "zod_json_object_tests.rs"]
mod object_tests;
