//! Omitted versus supplied JSON object members, without implicit nullability.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A member that is either omitted or supplied. Use `#[field(optional)]`.
/// Unlike `Option<T>`, a missing member is distinct from a supplied null;
/// null is accepted only when `T` itself accepts null, such as `Option<String>`.
///
/// ```
/// use forklaunch_core::contract::{schema, schema::Optional};
/// #[schema(unknown_fields="deny")]
/// struct Update {
///     #[field(optional, min_length=1, max_length=200)]
///     title: Optional<String>,
///     #[field(optional, min=0, max=100)]
///     revision: Optional<i64>,
/// }
/// let update = Update { title: Optional::Missing, revision: Optional::Value(1) };
/// assert_eq!(update.revision.as_option(), Some(&1));
/// ```
///
/// ```compile_fail
/// use forklaunch_core::contract::{schema, schema::Optional};
/// #[schema]
/// struct Invalid { value: Optional<String> }
/// ```
/// ```compile_fail
/// use forklaunch_core::contract::{schema, schema::Optional};
/// #[schema]
/// struct Invalid { #[field(optional, patch)] value: Optional<String> }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Optional<T> {
    #[default]
    Missing,
    Value(T),
}
impl<T> Optional<T> {
    /// Borrow a supplied value while retaining omission as `None`.
    #[must_use]
    pub const fn as_option(&self) -> Option<&T> {
        match self {
            Self::Missing => None,
            Self::Value(value) => Some(value),
        }
    }
    /// Consume this member, returning its supplied value if present.
    #[must_use]
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Missing => None,
            Self::Value(value) => Some(value),
        }
    }

    #[must_use]
    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}
impl<T: Serialize> Serialize for Optional<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Missing => Err(serde::ser::Error::custom(
                "Optional::Missing requires an optional object member",
            )),
            Self::Value(value) => value.serialize(serializer),
        }
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Optional<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Value)
    }
}
