//! qs-style bracket paths, sparse arrays and object merging for UTF-8 forms.
use indexmap::IndexMap;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug)]
pub(super) enum Node {
    Text(String),
    Array(BTreeMap<u64, Self>),
    /// Insertion-ordered like a JavaScript object; see [`Node::into_value`].
    Object(IndexMap<String, Self>, Option<u64>),
}
impl Node {
    pub(super) fn empty() -> Self {
        Self::Object(IndexMap::new(), None)
    }
    pub(super) fn values(values: Vec<Value>) -> Self {
        let mut nodes: Vec<_> = values
            .into_iter()
            .map(|value| Self::Text(value.as_str().expect("form string").into()))
            .collect();
        if nodes.len() == 1 {
            nodes.remove(0)
        } else {
            Self::array(nodes)
        }
    }
    fn array(nodes: Vec<Self>) -> Self {
        Self::Array(
            nodes
                .into_iter()
                .enumerate()
                .map(|(index, node)| (index as u64, node))
                .collect(),
        )
    }
    fn object(self) -> (IndexMap<String, Self>, Option<u64>) {
        match self {
            Self::Object(fields, overflow) => (fields, overflow),
            Self::Array(fields) => (
                fields
                    .into_iter()
                    .map(|(index, node)| (index.to_string(), node))
                    .collect(),
                None,
            ),
            Self::Text(_) => unreachable!("object conversion requires container"),
        }
    }
    fn is_container(&self) -> bool {
        !matches!(self, Self::Text(_))
    }
    fn length(fields: &BTreeMap<u64, Self>) -> u64 {
        fields.last_key_value().map_or(0, |(index, _)| index + 1)
    }
    fn bounded_array(fields: BTreeMap<u64, Self>, limit: u64) -> Self {
        let length = Self::length(&fields);
        if length > limit {
            Self::Object(
                fields
                    .into_iter()
                    .map(|(index, node)| (index.to_string(), node))
                    .collect(),
                Some(length - 1),
            )
        } else {
            Self::Array(fields)
        }
    }
    pub(super) fn merge(self, source: Self, limit: u64) -> Self {
        match (self, source) {
            (target, Self::Text(value)) if value.is_empty() => target,
            (Self::Array(mut fields), source @ Self::Text(_)) => {
                fields.insert(Self::length(&fields), source);
                Self::bounded_array(fields, limit)
            }
            (Self::Object(mut fields, Some(max)), source @ Self::Text(_)) => {
                fields.insert((max + 1).to_string(), source);
                Self::Object(fields, Some(max + 1))
            }
            (target @ Self::Text(_), Self::Object(fields, Some(max))) => {
                let mut result = IndexMap::from([("0".into(), target)]);
                for (key, node) in fields {
                    let digits: String = key.chars().take_while(char::is_ascii_digit).collect();
                    let shifted = digits
                        .parse::<u64>()
                        .ok()
                        .and_then(|index| index.checked_add(1))
                        .map_or_else(|| "NaN".into(), |index| index.to_string());
                    result.insert(shifted, node);
                }
                Self::Object(result, Some(max + 1))
            }
            (target @ Self::Text(_), Self::Array(fields)) => {
                let mut result = BTreeMap::from([(0, target)]);
                result.extend(fields.into_iter().map(|(index, node)| (index + 1, node)));
                Self::bounded_array(result, limit)
            }
            (target, source @ Self::Text(_)) | (target @ Self::Text(_), source) => {
                Self::array(vec![target, source])
            }
            (Self::Array(mut target), Self::Array(source)) => {
                for (index, node) in source {
                    if target.get(&index).is_some_and(Self::is_container) && node.is_container() {
                        let previous = target.remove(&index).expect("indexed node");
                        target.insert(index, previous.merge(node, limit));
                    } else if target.contains_key(&index) {
                        target.insert(Self::length(&target), node);
                    } else {
                        target.insert(index, node);
                    }
                }
                Self::bounded_array(target, limit)
            }
            (target, source) => {
                let (mut target, mut overflow) = target.object();
                let (source, source_overflow) = source.object();
                overflow = overflow.or(source_overflow);
                for (key, value) in source {
                    if let Some(max) = overflow.as_mut()
                        && let Some(index) = numeric(&key)
                    {
                        *max = (*max).max(index);
                    }
                    // Assigning an existing JavaScript property keeps its position.
                    match target.get_mut(&key) {
                        Some(previous) => {
                            let merged =
                                std::mem::replace(previous, Self::empty()).merge(value, limit);
                            *previous = merged;
                        }
                        None => {
                            target.insert(key, value);
                        }
                    }
                }
                Self::Object(target, overflow)
            }
        }
    }
    pub(super) fn into_value(self, depth: usize) -> Option<Value> {
        if depth > 64 {
            return None;
        }
        match self {
            Self::Text(text) => Some(Value::String(text)),
            Self::Array(fields) => fields
                .into_values()
                .map(|node| node.into_value(depth + 1))
                .collect::<Option<Vec<_>>>()
                .map(Value::Array),
            Self::Object(fields, _) => {
                // qs builds plain objects, so handlers see JavaScript enumeration
                // order: array-index keys ascending, then the rest as inserted.
                let mut fields: Vec<_> = fields.into_iter().collect();
                fields.sort_by_key(|(key, _)| array_index(key).map_or((1, 0), |index| (0, index)));
                fields
                    .into_iter()
                    .map(|(key, node)| Some((key, node.into_value(depth + 1)?)))
                    .collect::<Option<Map<_, _>>>()
                    .map(Value::Object)
            }
        }
    }
    pub(super) fn within_depth(&self, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        match self {
            Self::Text(_) => true,
            Self::Array(fields) => fields.values().all(|node| node.within_depth(depth + 1)),
            Self::Object(fields, _) => fields.values().all(|node| node.within_depth(depth + 1)),
        }
    }
}
// Canonical JavaScript array-index property names enumerate first, ascending.
fn array_index(key: &str) -> Option<u32> {
    key.parse::<u32>()
        .ok()
        .filter(|index| *index != u32::MAX && index.to_string() == key)
}

// Number strings outside the exactly representable JS integer range stay keys.
fn numeric(key: &str) -> Option<u64> {
    key.parse::<u64>()
        .ok()
        .filter(|index| *index <= 9_007_199_254_740_991 && index.to_string() == key)
}

pub(super) fn wrap(key: &str, mut leaf: Node, limit: u64, depth: usize) -> Option<Node> {
    if depth == 0 {
        return Some(if key.is_empty() || key == "__proto__" {
            Node::empty()
        } else {
            Node::Object(IndexMap::from([(key.into(), leaf)]), None)
        });
    }
    if key.is_empty() {
        return Some(Node::empty());
    }
    let mut segments = Vec::new();
    let first = key.find('[').unwrap_or(key.len());
    if first > 0 {
        segments.push((&key[..first], false));
    }
    let mut offset = first;
    let mut count = 0;
    while offset < key.len() {
        if count >= depth {
            return None;
        }
        let mut level = 1;
        let mut close = None;
        for (index, byte) in key.as_bytes().iter().enumerate().skip(offset + 1) {
            match byte {
                b'[' => level += 1,
                b']' => level -= 1,
                _ => {}
            }
            if level == 0 {
                close = Some(index);
                break;
            }
        }
        let Some(close) = close else {
            segments.push((&key[offset..], false));
            break;
        };
        segments.push((&key[offset + 1..close], true));
        count += 1;
        offset = key[close + 1..]
            .find('[')
            .map_or(key.len(), |index| close + 1 + index);
    }
    for (name, bracket) in segments.into_iter().rev() {
        leaf = if bracket && name.is_empty() {
            match leaf {
                Node::Array(_) | Node::Object(_, Some(_)) => leaf,
                other => Node::array(vec![other]),
            }
        } else if bracket && let Some(index) = numeric(name) {
            if index < limit {
                Node::Array(BTreeMap::from([(index, leaf)]))
            } else {
                Node::Object(IndexMap::from([(name.into(), leaf)]), Some(index))
            }
        } else if name == "__proto__" {
            Node::empty()
        } else {
            Node::Object(IndexMap::from([(name.into(), leaf)]), None)
        };
    }
    Some(leaf)
}
