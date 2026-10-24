//! Immutable JSON input retaining local own-property order without path copies.
use super::resources::MAX_ADMISSION_BYTES;
use super::{MAX_DEPTH, MAX_NODES, ZodJsonUnsupported, index_key};
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::{collections::BTreeMap, fmt};

/// Bounded JSON parsing failure, separate from schema validation diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderedJsonError {
    /// Invalid syntax or outside the finite Unicode/IEEE-754 JSON input domain.
    InvalidJson,
    Unsupported(ZodJsonUnsupported),
}

/// Immutable parsed input with original own-key order. Integer-index keys follow
/// ECMAScript order; duplicate keys keep their first position and last value.
/// Parsing admits at most 16 MiB raw bytes and a 16 MiB logical value/metadata
/// allowance (32 units per node plus retained string/key bytes), 100,000 nodes,
/// and depth 64. Allocator overhead is not measured. Local order trees avoid
/// ancestor-path copies.
/// Numbers normalize to finite IEEE-754, preserving negative zero. Lone UTF-16
/// surrogates and nonfinite numbers remain unsupported. Outputs of the evaluator
/// are semantic JSON values, not a promise about serialized property order.
/// Never reparse stale original bytes after revivers or schema transformations.
#[derive(Debug)]
pub struct OrderedJsonDocument {
    pub(super) value: Value,
    pub(super) order: Order,
    pub(super) bytes: usize,
}
impl OrderedJsonDocument {
    /// Parse a complete JSON document without executing user callbacks.
    /// # Errors
    /// Returns invalid syntax or an explicit resource/numeric boundary.
    pub fn parse(bytes: &[u8]) -> Result<Self, OrderedJsonError> {
        if bytes.len() > MAX_ADMISSION_BYTES {
            return Err(OrderedJsonError::Unsupported(
                ZodJsonUnsupported::InputBytes,
            ));
        }
        let numbers = numeric_tokens(bytes)?;
        let mut state = Admission {
            nodes: 0,
            bytes: numbers.len() * 8,
            failure: None,
            numbers: numbers.into_iter(),
        };
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        let result = Seed {
            state: &mut state,
            depth: 0,
        }
        .deserialize(&mut decoder);
        let (value, order) = result.map_err(|_| state.error())?;
        decoder.end().map_err(|_| OrderedJsonError::InvalidJson)?;
        if state.numbers.len() != 0 {
            return Err(OrderedJsonError::InvalidJson);
        }
        Ok(Self {
            value,
            order,
            bytes: state.bytes,
        })
    }

    /// Embed this immutable document under a new owned object field. The
    /// context's child objects do not acquire invented source order; only this
    /// document retains its original order. Combined admission precedes key
    /// copies and all body values/order metadata are moved, never cloned.
    /// # Errors
    /// Rejects duplicate field names and aggregate input budgets.
    pub fn embed_in_object(
        self,
        name: &str,
        context: Map<String, Value>,
    ) -> Result<Self, ZodJsonUnsupported> {
        if context.contains_key(name) {
            return Err(ZodJsonUnsupported::DuplicateField);
        }
        let order_bytes = context
            .keys()
            .try_fold(0usize, |size, key| {
                size.checked_add(key.len())
                    .and_then(|size| size.checked_add(32))
            })
            .ok_or(ZodJsonUnsupported::InputBytes)?;
        let context = Value::Object(context);
        let (_, context_bytes) = super::admit_value_counted(&context, None)?;
        let bytes = self
            .bytes
            .checked_add(context_bytes)
            .and_then(|bytes| bytes.checked_add(order_bytes))
            .and_then(|bytes| bytes.checked_add(name.len().saturating_mul(3).saturating_add(64)))
            .filter(|bytes| *bytes <= MAX_ADMISSION_BYTES)
            .ok_or(ZodJsonUnsupported::InputBytes)?;
        let Value::Object(mut context) = context else {
            unreachable!()
        };
        let mut keys = context.keys().cloned().collect::<Vec<_>>();
        keys.push(name.to_owned());
        keys.sort_by_key(|key| index_key(key));
        // Root order is the newly constructed context, not a reconstructed wire object.
        let mut children = BTreeMap::new();
        children.insert(name.to_owned(), self.order);
        context.insert(name.to_owned(), self.value);
        let value = Value::Object(context);
        super::admit_value_counted(&value, None)?;
        Ok(Self {
            value,
            order: Order::Object { keys, children },
            bytes,
        })
    }

    /// Borrow the value. No mutable access is exposed: order cannot become stale.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
}

#[derive(Debug)]
pub(super) enum Order {
    Scalar,
    Array(Vec<Self>),
    Object {
        keys: Vec<String>,
        children: BTreeMap<String, Self>,
    },
}
impl Order {
    pub(super) fn index(&self, index: usize) -> Option<&Self> {
        if let Self::Array(values) = self {
            values.get(index)
        } else {
            None
        }
    }
    pub(super) fn field(&self, key: &str) -> Option<&Self> {
        if let Self::Object { children, .. } = self {
            children.get(key)
        } else {
            None
        }
    }
    pub(super) fn keys(&self) -> Option<&[String]> {
        if let Self::Object { keys, .. } = self {
            Some(keys)
        } else {
            None
        }
    }
}
struct Admission {
    nodes: usize,
    bytes: usize,
    failure: Option<ZodJsonUnsupported>,
    numbers: std::vec::IntoIter<f64>,
}
impl Admission {
    fn fail<E: de::Error>(&mut self, error: ZodJsonUnsupported) -> E {
        self.failure = Some(error);
        E::custom("ordered JSON admission")
    }
    fn charge<E: de::Error>(&mut self, bytes: usize) -> Result<(), E> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > MAX_ADMISSION_BYTES {
            return Err(self.fail(ZodJsonUnsupported::InputBytes));
        }
        Ok(())
    }
    fn error(&self) -> OrderedJsonError {
        self.failure
            .map_or(OrderedJsonError::InvalidJson, OrderedJsonError::Unsupported)
    }
}
struct Seed<'a> {
    state: &'a mut Admission,
    depth: usize,
}
impl Seed<'_> {
    fn number<E: de::Error>(self) -> Result<(Value, Order), E> {
        let value = self
            .state
            .numbers
            .next()
            .ok_or_else(|| E::custom("missing numeric token"))?;
        let number = Number::from_f64(value).ok_or_else(|| E::custom("nonfinite numeric token"))?;
        Ok((Value::Number(number), Order::Scalar))
    }
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = (Value, Order);
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        if self.depth > MAX_DEPTH {
            return Err(self.state.fail(ZodJsonUnsupported::InputDepth));
        }
        self.state.nodes += 1;
        if self.state.nodes > MAX_NODES {
            return Err(self.state.fail(ZodJsonUnsupported::InputNodes));
        }
        self.state.charge(32)?;
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = (Value, Order);
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok((Value::Null, Order::Scalar))
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok((Value::Bool(value), Order::Scalar))
    }
    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        self.number()
    }
    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        self.number()
    }
    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        self.number()
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.state.charge(value.len())?;
        Ok((Value::String(value.to_owned()), Order::Scalar))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        self.state.charge(value.len())?;
        Ok((Value::String(value), Order::Scalar))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        let mut orders = Vec::new();
        while let Some((value, order)) = sequence.next_element_seed(Seed {
            state: self.state,
            depth: self.depth + 1,
        })? {
            values.push(value);
            orders.push(order);
        }
        Ok((Value::Array(values), Order::Array(orders)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let first = map.next_key_seed(KeySeed)?;
        if let Some(Key::Number) = first {
            let spelling: String = map.next_value()?;
            self.state.charge(spelling.len())?;
            return self.number();
        }
        let mut values = Map::new();
        let mut keys = Vec::new();
        let mut children = BTreeMap::new();
        let mut current = first;
        while let Some(key) = current {
            let Key::Literal(key) = key else {
                return Err(de::Error::custom("unexpected number key"));
            };
            // Reserve all three retained copies before cloning a key. Duplicate
            // occurrences are conservatively charged despite replacing old data.
            self.state
                .charge(key.len().saturating_mul(3).saturating_add(32))?;
            let (value, order) = map.next_value_seed(Seed {
                state: self.state,
                depth: self.depth + 1,
            })?;
            if !values.contains_key(&key) {
                keys.push(key.clone());
            }
            children.insert(key.clone(), order);
            values.insert(key, value);
            current = map.next_key_seed(KeySeed)?;
        }
        keys.sort_by_key(|key| index_key(key));
        Ok((Value::Object(values), Order::Object { keys, children }))
    }
}
// The vendored serde adapter distinguishes literal JSON keys using a unit enum.
// Numeric private-protocol keys instead visit strings. Never infer protocol from
// spelling alone: an actual object may legally contain the same property name.
enum Key {
    Literal(String),
    Number,
}
struct KeySeed;
impl<'de> DeserializeSeed<'de> for KeySeed {
    type Value = Key;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Key, D::Error> {
        d.deserialize_enum("$serde_json::private::LiteralKey", &[], self)
    }
}
impl<'de> Visitor<'de> for KeySeed {
    type Value = Key;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON key")
    }
    fn visit_enum<A: EnumAccess<'de>>(self, value: A) -> Result<Key, A::Error> {
        let (key, variant) = value.variant::<String>()?;
        variant.unit_variant()?;
        Ok(Key::Literal(key))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Key, E> {
        if value == "$serde_json::private::Number" {
            Ok(Key::Number)
        } else {
            Err(E::custom("unexpected private key"))
        }
    }
}

// serde's arbitrary_precision visitor turns lexical -0 into integer 0. Capture
// numeric spellings independently, without interpreting object keys or strings;
// the actual serde decoder remains the full JSON grammar authority. Every
// numeric visit consumes exactly one source token, preserving IEEE semantics.
fn numeric_tokens(bytes: &[u8]) -> Result<Vec<f64>, OrderedJsonError> {
    let mut numbers = Vec::new();
    let mut position = 0;
    while position < bytes.len() {
        match bytes[position] {
            b'"' => {
                position += 1;
                while position < bytes.len() {
                    let byte = bytes[position];
                    position += 1;
                    if byte == b'"' {
                        break;
                    }
                    if byte == b'\\' {
                        position = position.saturating_add(1);
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = position;
                position += 1;
                while position < bytes.len()
                    && matches!(
                        bytes[position],
                        b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'
                    )
                {
                    position += 1;
                }
                if numbers.len() >= MAX_NODES {
                    return Err(OrderedJsonError::Unsupported(
                        ZodJsonUnsupported::InputNodes,
                    ));
                }
                let spelling = std::str::from_utf8(&bytes[start..position])
                    .map_err(|_| OrderedJsonError::InvalidJson)?;
                let value = spelling
                    .parse::<f64>()
                    .map_err(|_| OrderedJsonError::InvalidJson)?;
                if !value.is_finite() {
                    return Err(OrderedJsonError::InvalidJson);
                }
                numbers.push(value);
            }
            _ => position += 1,
        }
    }
    Ok(numbers)
}
