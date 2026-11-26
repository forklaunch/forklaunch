//! Preserve JSON declaration order for reviver traversal without changing global
//! `serde_json` map features (which would alter unrelated generated artifacts).
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::fmt;

pub(super) enum Node {
    Scalar(Value),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}
impl Node {
    pub(super) fn map_scalars(&mut self, transform: fn(&mut Value)) {
        match self {
            Self::Scalar(value) => transform(value),
            Self::Array(values) => values
                .iter_mut()
                .for_each(|value| value.map_scalars(transform)),
            Self::Object(values) => values
                .iter_mut()
                .for_each(|(_, value)| value.map_scalars(transform)),
        }
    }
}
impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NodeVisitor;
        impl<'de> Visitor<'de> for NodeVisitor {
            type Value = Node;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value")
            }
            fn visit_bool<E>(self, value: bool) -> Result<Node, E> {
                Ok(Node::Scalar(value.into()))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Node, E> {
                Ok(Node::Scalar(value.into()))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Node, E> {
                Ok(Node::Scalar(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Node, E> {
                serde_json::Number::from_f64(value)
                    .map(|value| Node::Scalar(Value::Number(value)))
                    .ok_or_else(|| E::custom("invalid JSON number"))
            }
            fn visit_str<E>(self, value: &str) -> Result<Node, E> {
                Ok(Node::Scalar(value.into()))
            }
            fn visit_string<E>(self, value: String) -> Result<Node, E> {
                Ok(Node::Scalar(value.into()))
            }
            fn visit_unit<E>(self) -> Result<Node, E> {
                Ok(Node::Scalar(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element()? {
                    values.push(value);
                }
                Ok(Node::Array(values))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
                let mut values: Vec<(String, Node)> = Vec::new();
                let mut positions = std::collections::HashMap::new();
                while let Some((key, value)) = map.next_entry::<String, Node>()? {
                    if let Some(index) = positions.get(&key).copied() {
                        values[index] = (key, value);
                    } else {
                        positions.insert(key.clone(), values.len());
                        values.push((key, value));
                    }
                }
                Ok(Node::Object(values))
            }
        }
        deserializer.deserialize_any(NodeVisitor)
    }
}

pub(super) fn revive(
    callback: &super::Reviver,
    key: &str,
    node: Node,
    depth: usize,
) -> Result<Option<Value>, String> {
    if depth > 128 {
        return Err("JSON reviver depth exceeded".into());
    }
    let value = match node {
        Node::Scalar(value) => value,
        Node::Object(mut fields) => {
            fields.sort_by_key(|(key, _)| {
                let integer = key
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n != u32::MAX && n.to_string() == *key);
                (integer.is_none(), integer.unwrap_or(0))
            });
            let mut result = serde_json::Map::new();
            for (key, child) in fields {
                if let Some(child) = revive(callback, &key, child, depth + 1)? {
                    result.insert(key, child);
                }
            }
            Value::Object(result)
        }
        Node::Array(array) => {
            let mut result = Vec::with_capacity(array.len());
            for (index, child) in array.into_iter().enumerate() {
                result.push(
                    revive(callback, &index.to_string(), child, depth + 1)?
                        .ok_or("JSON values cannot represent sparse array removal")?,
                );
            }
            Value::Array(result)
        }
    };
    callback(key, value)
}
