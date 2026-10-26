//! Explicit absent/null/value semantics for JSON object updates.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A JSON object member that keeps, clears, or sets an existing value.
///
/// Declare direct members with `#[field(patch)]` inside `#[schema]`. The macro
/// owns omission, parsing and the optional/nullable schema projection. `Keep`
/// cannot be serialized outside that member binding; `Set` cannot encode null.
///
/// ```compile_fail
/// use forklaunch_core::contract::{schema, schema::Patch};
/// #[schema]
/// struct Invalid { value: Patch<String> }
/// ```
/// ```compile_fail
/// use forklaunch_core::contract::schema;
/// #[schema]
/// struct Invalid { #[field(patch)] value: String }
/// ```
/// ```compile_fail
/// use forklaunch_core::contract::{schema, schema::Patch};
/// #[schema]
/// struct Invalid { #[field(patch)] value: Vec<Patch<String>> }
/// ```
/// ```compile_fail
/// use forklaunch_core::contract::{schema, schema::Patch};
/// #[schema]
/// struct Invalid { #[field(patch, min_length=1)] value: Patch<String> }
/// ```
/// ```compile_fail
/// use forklaunch_core::contract::{schema, schema::Patch};
/// #[schema]
/// struct Invalid { #[field(patch)] #[serde(default)] value: Patch<String> }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Patch<T> {
    #[default]
    Keep,
    Clear,
    Set(T),
}

impl<T> Patch<T> {
    #[must_use]
    pub const fn is_keep(&self) -> bool {
        matches!(self, Self::Keep)
    }
}

impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Keep => Err(serde::ser::Error::custom(
                "Patch::Keep requires a patch object member",
            )),
            Self::Clear => serializer.serialize_none(),
            Self::Set(value) => {
                let value = serde_json::to_value(value)
                    .map_err(|_| serde::ser::Error::custom("patch value violates contract"))?;
                if value.is_null() {
                    return Err(serde::ser::Error::custom(
                        "Patch::Set cannot encode null; use Patch::Clear",
                    ));
                }
                value.serialize(serializer)
            }
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(|value| value.map_or(Self::Clear, Self::Set))
    }
}
