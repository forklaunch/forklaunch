//! Bounded filters over declared directory fields. SQL operators and arbitrary
//! property paths are intentionally not accepted. Evaluation occurs on decoded
//! tenant-scoped rows; scalar predicates preserve SQL three-valued null logic.
use super::{invalid, storage_error};
use forklaunch_core::domain::BlueprintResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod sealed {
    pub trait Sealed {}
}
/// Closed field vocabulary associated with one directory family.
pub trait DirectoryField: sealed::Sealed + Copy + Send + Sync {
    #[doc(hidden)]
    fn descriptor(self) -> FieldDescriptor;
}
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct FieldDescriptor {
    name: &'static str,
    uuid: bool,
    relation: bool,
}
macro_rules! fields {
    ($name:ident { $($variant:ident => ($wire:literal,$uuid:literal,$relation:literal)),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
        #[serde(rename_all="camelCase")]
        pub enum $name { $($variant),* }
        impl sealed::Sealed for $name {}
        impl DirectoryField for $name {
            fn descriptor(self)->FieldDescriptor { match self { $(Self::$variant=>FieldDescriptor{name:$wire,uuid:$uuid,relation:$relation}),* } }
        }
    };
}
fields!(UserField {Id=>("id",true,false),Email=>("email",false,false),FirstName=>("firstName",false,false),LastName=>("lastName",false,false),PhoneNumber=>("phoneNumber",false,false),Subscription=>("subscription",false,false),OrganizationId=>("organization",true,false),RoleIds=>("roles",true,true)});
fields!(RoleField {Id=>("id",true,false),Name=>("name",false,false),PermissionIds=>("permissions",true,true)});
fields!(PermissionField {Id=>("id",true,false),Slug=>("slug",false,false)});
fields!(OrganizationField {Id=>("id",true,false),Name=>("name",false,false),Domain=>("domain",false,false),LogoUrl=>("logoUrl",false,false),Subscription=>("subscription",false,false),Status=>("status",false,false)});

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Predicate {
    Eq(String),
    Ne(String),
    In(Vec<String>),
    NotIn(Vec<String>),
    IsNull,
    IsNotNull,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum DirectoryFilter<F> {
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
    Compare {
        field: F,
        test: Predicate,
    },
    /// At least one of the related IDs is present; negation means none present.
    Related {
        field: F,
        ids: Vec<String>,
    },
}
#[derive(Clone, Copy, PartialEq)]
enum Truth {
    True,
    False,
    Unknown,
    // MikroORM omits empty conjunction SQL, including inside disjunctions.
    // This is distinct from a concrete true expression.
    Omitted,
}
impl Truth {
    fn negate(self) -> Self {
        match self {
            Self::True | Self::Omitted => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}
impl<F: DirectoryField> DirectoryFilter<F> {
    /// Validate budgets and field/operator compatibility before reading rows.
    /// Empty AND emits no predicate; empty OR is false. Nested groups follow
    /// `MikroORM` SQL-fragment omission, so OR ignores an empty AND child.
    /// # Errors
    /// Rejects excessive depth/nodes/values/bytes, malformed UUIDs, or using a
    /// collection field as a scalar (or scalar field as a relationship).
    pub fn validate(&self) -> BlueprintResult<()> {
        let mut nodes = 0;
        let mut values = 0;
        let mut bytes = 0;
        self.validate_inner(0, &mut nodes, &mut values, &mut bytes)
    }
    fn validate_inner(
        &self,
        depth: usize,
        nodes: &mut usize,
        values: &mut usize,
        bytes: &mut usize,
    ) -> BlueprintResult<()> {
        *nodes += 1;
        if depth > 16 || *nodes > 256 {
            return Err(invalid());
        }
        let (field, items) = match self {
            Self::All(children) | Self::Any(children) => {
                for child in children {
                    child.validate_inner(depth + 1, nodes, values, bytes)?;
                }
                return Ok(());
            }
            Self::Not(child) => return child.validate_inner(depth + 1, nodes, values, bytes),
            Self::Compare { field, test } => {
                let descriptor = field.descriptor();
                if descriptor.relation {
                    return Err(invalid());
                }
                let items = match test {
                    Predicate::Eq(value) | Predicate::Ne(value) => std::slice::from_ref(value),
                    Predicate::In(values) | Predicate::NotIn(values) => values.as_slice(),
                    Predicate::IsNull | Predicate::IsNotNull => &[],
                };
                (descriptor, items)
            }
            Self::Related { field, ids } => {
                let descriptor = field.descriptor();
                if !descriptor.relation {
                    return Err(invalid());
                }
                (descriptor, ids.as_slice())
            }
        };
        *values = values.checked_add(items.len()).ok_or_else(invalid)?;
        if *values > 4096 {
            return Err(invalid());
        }
        for item in items {
            *bytes = bytes.checked_add(item.len()).ok_or_else(invalid)?;
            if *bytes > 65536 {
                return Err(invalid());
            }
            if field.uuid {
                parse_uuid(item)?;
            }
        }
        Ok(())
    }
    pub(super) fn matches(&self, row: &Value, remaining: &mut usize) -> BlueprintResult<bool> {
        Ok(matches!(
            self.evaluate(row, remaining)?,
            Truth::True | Truth::Omitted
        ))
    }
    fn evaluate(&self, row: &Value, remaining: &mut usize) -> BlueprintResult<Truth> {
        spend(remaining)?;
        match self {
            Self::All(children) => {
                let mut result = Truth::Omitted;
                for child in children {
                    match child.evaluate(row, remaining)? {
                        Truth::False => return Ok(Truth::False),
                        Truth::Unknown => result = Truth::Unknown,
                        Truth::True if result == Truth::Omitted => result = Truth::True,
                        Truth::True | Truth::Omitted => {}
                    }
                }
                Ok(result)
            }
            Self::Any(children) => {
                let mut result = if children.is_empty() {
                    Truth::False
                } else {
                    Truth::Omitted
                };
                for child in children {
                    match child.evaluate(row, remaining)? {
                        Truth::True => return Ok(Truth::True),
                        Truth::Unknown => result = Truth::Unknown,
                        Truth::False if result == Truth::Omitted => result = Truth::False,
                        Truth::False | Truth::Omitted => {}
                    }
                }
                Ok(result)
            }
            Self::Not(child) => Ok(child.evaluate(row, remaining)?.negate()),
            Self::Related { field, ids } => {
                let descriptor = field.descriptor();
                let stored = row
                    .get(descriptor.name)
                    .and_then(Value::as_array)
                    .ok_or_else(storage_error)?;
                for value in stored {
                    let id = value.as_str().ok_or_else(storage_error)?;
                    for expected in ids {
                        if equal(id, expected, true, remaining)? {
                            return Ok(Truth::True);
                        }
                    }
                }
                Ok(Truth::False)
            }
            Self::Compare { field, test } => {
                let descriptor = field.descriptor();
                let value = row.get(descriptor.name).filter(|value| !value.is_null());
                if matches!(test, Predicate::IsNull) {
                    return Ok(truth(value.is_none()));
                }
                if matches!(test, Predicate::IsNotNull) {
                    return Ok(truth(value.is_some()));
                }
                if let Predicate::In(values) | Predicate::NotIn(values) = test
                    && values.is_empty()
                {
                    return Ok(truth(matches!(test, Predicate::NotIn(_))));
                }
                let Some(value) = value else {
                    return Ok(Truth::Unknown);
                };
                let value = value.as_str().ok_or_else(storage_error)?;
                let matched = match test {
                    Predicate::Eq(expected) => equal(value, expected, descriptor.uuid, remaining)?,
                    Predicate::Ne(expected) => !equal(value, expected, descriptor.uuid, remaining)?,
                    Predicate::In(values) | Predicate::NotIn(values) => {
                        let mut found = false;
                        for expected in values {
                            found |= equal(value, expected, descriptor.uuid, remaining)?;
                        }
                        if matches!(test, Predicate::NotIn(_)) {
                            !found
                        } else {
                            found
                        }
                    }
                    Predicate::IsNull | Predicate::IsNotNull => unreachable!(),
                };
                Ok(truth(matched))
            }
        }
    }
}
fn truth(value: bool) -> Truth {
    if value { Truth::True } else { Truth::False }
}
fn equal(
    value: &str,
    expected: &str,
    is_uuid: bool,
    remaining: &mut usize,
) -> BlueprintResult<bool> {
    spend(remaining)?;
    if is_uuid {
        Ok(uuid::Uuid::parse_str(value).map_err(|_| storage_error())? == parse_uuid(expected)?)
    } else {
        Ok(value == expected)
    }
}

fn parse_uuid(value: &str) -> BlueprintResult<uuid::Uuid> {
    // Admit canonical hyphenated UUID spelling (either case), not alternate URN
    // forms accepted by Rust but rejected by the actual PostgreSQL UUID input.
    if value.len() != 36
        || [8, 13, 18, 23]
            .iter()
            .any(|index| value.as_bytes()[*index] != b'-')
    {
        return Err(invalid());
    }
    uuid::Uuid::parse_str(value).map_err(|_| invalid())
}
pub(super) fn normalize_id(value: &str) -> BlueprintResult<String> {
    Ok(parse_uuid(value)?.to_string())
}

fn spend(remaining: &mut usize) -> BlueprintResult<()> {
    *remaining = remaining.checked_sub(1).ok_or_else(|| {
        forklaunch_core::domain::BlueprintError::new(
            "iam_directory_filter_limit",
            "Directory filter evaluation exceeds its operation budget",
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn evaluation_budget_is_shared_across_rows_and_rejects_alternate_uuid_forms() {
        let filter = DirectoryFilter::Compare {
            field: UserField::Email,
            test: Predicate::Eq("match".into()),
        };
        let mut remaining = 3;
        assert!(
            filter
                .matches(&json!({"email":"match"}), &mut remaining)
                .unwrap()
        );
        assert_eq!(
            filter
                .matches(&json!({"email":"match"}), &mut remaining)
                .unwrap_err()
                .code,
            "iam_directory_filter_limit"
        );
        assert_eq!(remaining, 0);
        let uuid = uuid::Uuid::new_v4();
        for value in [
            format!("urn:uuid:{uuid}"),
            uuid.simple().to_string(),
            "bad-uuid".into(),
        ] {
            assert!(
                DirectoryFilter::Compare {
                    field: UserField::Id,
                    test: Predicate::Eq(value)
                }
                .validate()
                .is_err()
            );
        }
    }
}
