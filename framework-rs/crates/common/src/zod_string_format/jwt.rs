//! Raw Zod JWT header syntax. No signature, payload or claim verification.
#[derive(Default)]
enum Atom {
    #[default]
    False,
    Truthy,
    Text(Vec<u16>),
}
impl Atom {
    fn truthy(&self) -> bool {
        match self {
            Self::False => false,
            Self::Truthy => true,
            Self::Text(v) => !v.is_empty(),
        }
    }
    fn text_eq(&self, value: &str) -> bool {
        matches!(self, Self::Text(v) if v.iter().copied().eq(value.encode_utf16()))
    }
}
fn alphabet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}
fn decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 4 * 3 + 2);
    let (mut bits, mut pending) = (0, 0u32);
    for byte in value.bytes() {
        pending = (pending << 6) | u32::from(alphabet(byte)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push(u8::try_from((pending >> bits) & 255).ok()?);
        }
    }
    Some(bytes)
}
/// JavaScript atob supplies Latin1 code units, not UTF8. A small iterative JSON
/// reader preserves escaped lone UTF16 surrogates and overflowing JSON numbers
/// without materializing irrelevant header members or recursing on the call stack.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn space(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn string(&mut self, capture: bool) -> Option<Vec<u16>> {
        if !self.take(b'"') {
            return None;
        }
        let mut out = Vec::new();
        loop {
            let mut value = u16::from(*self.bytes.get(self.at)?);
            self.at += 1;
            if value == 34 {
                return Some(out);
            }
            if value < 32 {
                return None;
            }
            if value == 92 {
                let escaped = *self.bytes.get(self.at)?;
                self.at += 1;
                value = match escaped {
                    b'"' | b'\\' | b'/' => u16::from(escaped),
                    b'b' => 8,
                    b'f' => 12,
                    b'n' => 10,
                    b'r' => 13,
                    b't' => 9,
                    b'u' => {
                        let mut number = 0u16;
                        for _ in 0..4 {
                            let b = *self.bytes.get(self.at)?;
                            self.at += 1;
                            let digit = match b {
                                b'0'..=b'9' => b - b'0',
                                b'a'..=b'f' => b - b'a' + 10,
                                b'A'..=b'F' => b - b'A' + 10,
                                _ => return None,
                            };
                            number = number * 16 + u16::from(digit);
                        }
                        number
                    }
                    _ => return None,
                };
            }
            if capture {
                out.push(value);
            }
        }
    }
    fn number(&mut self) -> Option<Atom> {
        let start = self.at;
        if self.bytes.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        match self.bytes.get(self.at)? {
            b'0' => self.at += 1,
            b'1'..=b'9' => {
                self.at += 1;
                self.digits();
            }
            _ => return None,
        }
        if self.bytes.get(self.at) == Some(&b'.') {
            self.at += 1;
            let start = self.at;
            self.digits();
            if self.at == start {
                return None;
            }
        }
        if matches!(self.bytes.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.bytes.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            let start = self.at;
            self.digits();
            if self.at == start {
                return None;
            }
        }
        let number: f64 = std::str::from_utf8(&self.bytes[start..self.at])
            .ok()?
            .parse()
            .ok()?;
        Some(if number == 0.0 {
            Atom::False
        } else {
            Atom::Truthy
        })
    }
    fn digits(&mut self) {
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
    }
    fn atom(&mut self, capture: bool) -> Option<Atom> {
        self.space();
        match *self.bytes.get(self.at)? {
            b'"' => self.string(capture).map(Atom::Text),
            b'-' | b'0'..=b'9' => self.number(),
            b't' if self.bytes[self.at..].starts_with(b"true") => {
                self.at += 4;
                Some(Atom::Truthy)
            }
            b'f' if self.bytes[self.at..].starts_with(b"false") => {
                self.at += 5;
                Some(Atom::False)
            }
            b'n' if self.bytes[self.at..].starts_with(b"null") => {
                self.at += 4;
                Some(Atom::False)
            }
            _ => None,
        }
    }
    fn value(&mut self, capture: bool) -> Option<Atom> {
        self.space();
        if !matches!(self.bytes.get(self.at), Some(b'{' | b'[')) {
            return self.atom(capture);
        }
        // Each frame records whether a container is an object and whether its
        // next member may be absent. Nested values are processed iteratively.
        let mut stack = Vec::new();
        self.open(&mut stack)?;
        while let Some((object, first)) = stack.pop() {
            let close = if object { b'}' } else { b']' };
            if first && self.take(close) {
                continue;
            }
            if !first {
                if self.take(close) {
                    continue;
                }
                if !self.take(b',') {
                    return None;
                }
            }
            if object {
                self.string(false)?;
                if !self.take(b':') {
                    return None;
                }
            }
            stack.push((object, false));
            self.space();
            if matches!(self.bytes.get(self.at), Some(b'{' | b'[')) {
                self.open(&mut stack)?;
            } else {
                self.atom(false)?;
            }
        }
        Some(Atom::Truthy)
    }
    fn open(&mut self, stack: &mut Vec<(bool, bool)>) -> Option<()> {
        self.space();
        let object = match self.bytes.get(self.at)? {
            b'{' => true,
            b'[' => false,
            _ => return None,
        };
        self.at += 1;
        stack.push((object, true));
        Some(())
    }
    fn header(&mut self, algorithm: Option<&str>) -> Option<bool> {
        if !self.take(b'{') {
            return Some(false);
        }
        let mut alg = Atom::False;
        let mut typ = None;
        if !self.take(b'}') {
            loop {
                let key = self.string(true)?;
                if !self.take(b':') {
                    return None;
                }
                let is_alg = key == [97, 108, 103];
                let is_typ = key == [116, 121, 112];
                let value = self.value(is_alg || is_typ)?;
                if is_alg {
                    alg = value;
                } else if is_typ {
                    typ = Some(value);
                }
                if self.take(b'}') {
                    break;
                }
                if !self.take(b',') {
                    return None;
                }
            }
        }
        self.space();
        Some(
            self.at == self.bytes.len()
                && typ.as_ref().is_none_or(|v| v.text_eq("JWT"))
                && alg.truthy()
                && algorithm
                    .filter(|v| !v.is_empty())
                    .is_none_or(|v| alg.text_eq(v)),
        )
    }
}
pub(super) fn accepts(value: &str, algorithm: Option<&str>) -> bool {
    let mut pieces = value.split('.');
    let Some(header) = pieces.next().filter(|s| !s.is_empty()) else {
        return false;
    };
    let Some(payload) = pieces.next().filter(|s| !s.is_empty()) else {
        return false;
    };
    let Some(signature) = pieces.next() else {
        return false;
    };
    if pieces.next().is_some()
        || !payload
            .bytes()
            .chain(signature.bytes())
            .all(|b| alphabet(b).is_some())
    {
        return false;
    }
    let Some(bytes) = decode(header) else {
        return false;
    };
    Reader {
        bytes: &bytes,
        at: 0,
    }
    .header(algorithm)
    .unwrap_or(false)
}
