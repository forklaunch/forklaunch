use serde_json::Value;

use crate::{
    path::{JsonPointerNode, JsonPointerSegment},
    resource::unescape_segment,
};

/// Convert an AJV URI fragment to an internal JSON pointer. URI decoding happens
/// within tokens, before JSON-pointer unescaping; an encoded slash is a key
/// character, never a separator. Internal pointers must not be decoded again.
#[cfg(feature = "forklaunch-ajv-pointer-decoding")]
pub(crate) fn fragment_pointer(pointer: &str) -> Result<std::borrow::Cow<'_, str>, crate::Error> {
    if !pointer.starts_with('/') || !pointer.contains('%') {
        return Ok(std::borrow::Cow::Borrowed(pointer));
    }
    let mut canonical = String::with_capacity(pointer.len());
    for token in pointer[1..].split('/') {
        let bytes = token.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'%'
                && (index + 2 >= bytes.len()
                    || !bytes[index + 1].is_ascii_hexdigit()
                    || !bytes[index + 2].is_ascii_hexdigit())
            {
                return Err(crate::Error::pointer_to_nowhere(pointer));
            }
        }
        let decoded = percent_encoding::percent_decode_str(token)
            .decode_utf8()
            .map_err(|error| crate::Error::invalid_percent_encoding(pointer, error))?;
        let decoded = unescape_segment(&decoded);
        canonical.push('/');
        for character in decoded.chars() {
            match character {
                '~' => canonical.push_str("~0"),
                '/' => canonical.push_str("~1"),
                other => canonical.push(other),
            }
        }
    }
    Ok(std::borrow::Cow::Owned(canonical))
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedPointer {
    pub(crate) segments: Vec<ParsedPointerSegment>,
}

impl ParsedPointer {
    pub(crate) fn from_json_pointer(pointer: &str) -> Option<Self> {
        if pointer.is_empty() {
            return Some(Self::default());
        }
        if !pointer.starts_with('/') {
            return None;
        }

        let mut segments = Vec::new();
        for token in pointer.split('/').skip(1).map(unescape_segment) {
            if let Some(index) = parse_index(&token) {
                segments.push(ParsedPointerSegment::Index(index));
            } else {
                segments.push(ParsedPointerSegment::Key(
                    token.into_owned().into_boxed_str(),
                ));
            }
        }
        Some(Self { segments })
    }

    pub(crate) fn from_pointer_node(path: &JsonPointerNode<'_, '_>) -> Self {
        let mut segments = Vec::new();
        let mut head = path;

        while let Some(parent) = head.parent() {
            segments.push(match head.segment() {
                JsonPointerSegment::Key(key) => ParsedPointerSegment::Key(key.as_ref().into()),
                JsonPointerSegment::Index(idx) => ParsedPointerSegment::Index(*idx),
            });
            head = parent;
        }

        segments.reverse();
        Self { segments }
    }

    pub(crate) fn lookup<'a>(&self, document: &'a Value) -> Option<&'a Value> {
        self.segments
            .iter()
            .try_fold(document, |target, token| match token {
                ParsedPointerSegment::Key(key) => match target {
                    Value::Object(map) => map.get(&**key),
                    _ => None,
                },
                ParsedPointerSegment::Index(index) => match target {
                    Value::Array(list) => list.get(*index),
                    _ => None,
                },
            })
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ParsedPointerSegment {
    Key(Box<str>),
    Index(usize),
}

/// Look up a value by a JSON Pointer.
///
/// **NOTE**: A slightly faster version of pointer resolution based on `Value::pointer` from `serde_json`.
pub fn pointer<'a>(document: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(document);
    }
    if !pointer.starts_with('/') {
        return None;
    }
    pointer.split('/').skip(1).map(unescape_segment).try_fold(
        document,
        |target, token| match target {
            Value::Object(map) => map.get(&*token),
            Value::Array(list) => parse_index(&token).and_then(|x| list.get(x)),
            _ => None,
        },
    )
}

// Taken from `serde_json`.
#[must_use]
pub fn parse_index(s: &str) -> Option<usize> {
    if s.starts_with('+') || (s.starts_with('0') && s.len() != 1) {
        return None;
    }
    s.parse().ok()
}
