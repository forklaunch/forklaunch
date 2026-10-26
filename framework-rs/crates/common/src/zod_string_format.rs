//! Pure Zod v3 string predicates shared by schema and runtime consumers.

/// Match the exact ASCII UUID grammar used by Zod v3.
#[must_use]
pub fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

/// Match the exact ASCII email grammar used by Zod v3.
#[must_use]
pub fn email(value: &str) -> bool {
    if !value.is_ascii() || value.starts_with('.') || value.contains("..") {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    let terminal = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-');
    if local.is_empty()
        || !local
            .bytes()
            .all(|byte| terminal(byte) || matches!(byte, b'\'' | b'.'))
        || !local.bytes().last().is_some_and(terminal)
    {
        return false;
    }
    let Some((labels, top)) = domain.rsplit_once('.') else {
        return false;
    };
    top.len() >= 2
        && top.bytes().all(|byte| byte.is_ascii_alphabetic())
        && labels.split('.').all(|label| {
            label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Match Zod v3 CUID syntax, counting its tail in UTF-16 code units.
#[must_use]
pub fn cuid(value: &str) -> bool {
    let Some(tail) = value.strip_prefix(['c', 'C']) else {
        return false;
    };
    tail.encode_utf16().count() >= 8
        && !tail.chars().any(|c| {
            c == '-'
                || matches!(c,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
        })
}

/// Match Zod v3 CUID2 syntax without imposing a generator-specific length.
#[must_use]
pub fn cuid2(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
}

/// Match Zod v3 ULID syntax without imposing a timestamp-overflow restriction.
#[must_use]
pub fn ulid(value: &str) -> bool {
    value.len() == 26 && value.bytes().all(|c| c.is_ascii_digit() || matches!(c.to_ascii_uppercase(), b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
}

/// Match Zod v3 Nano ID's fixed 21-character alphabet.
#[must_use]
pub fn nanoid(value: &str) -> bool {
    value.len() == 21
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}

/// Match Zod v3 Base64 syntax, including empty input and noncanonical pad bits.
#[must_use]
pub fn base64(value: &str) -> bool {
    base64_syntax(value, false)
}

/// Match Zod v3 URL-safe Base64 syntax, with optional terminal padding.
#[must_use]
pub fn base64url(value: &str) -> bool {
    base64_syntax(value, true)
}

fn base64_syntax(value: &str, url: bool) -> bool {
    let data = value.trim_end_matches('=');
    let padding = value.len() - data.len();
    if !data.bytes().all(|c| {
        c.is_ascii_alphanumeric()
            || if url {
                matches!(c, b'_' | b'-')
            } else {
                matches!(c, b'+' | b'/')
            }
    }) {
        return false;
    }
    match padding {
        0 => {
            if url {
                data.len() % 4 != 1
            } else {
                data.len().is_multiple_of(4)
            }
        }
        1 => data.len() % 4 == 3,
        2 => data.len() % 4 == 2,
        _ => false,
    }
}

/// Match Zod v3's four-digit Gregorian date grammar, including year zero.
#[must_use]
pub fn date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
    {
        return false;
    }
    let year = bytes[..4]
        .iter()
        .fold(0u32, |acc, b| acc * 10 + u32::from(b - b'0'));
    let pair = |i| u32::from(bytes[i] - b'0') * 10 + u32::from(bytes[i + 1] - b'0');
    let days = match pair(5) {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) {
                29
            } else {
                28
            }
        }
        _ => return false,
    };
    (1..=days).contains(&pair(8))
}

/// Match Zod v3 duration syntax without numeric conversion or backtracking.
/// Components may carry independent signs; a fraction must end the duration.
#[must_use]
pub fn duration(value: &str) -> bool {
    let mut input = value.as_bytes();
    if matches!(input.first(), Some(b'+' | b'-')) {
        input = &input[1..];
    }
    let Some(mut input) = input.strip_prefix(b"P") else {
        return false;
    };
    if input.is_empty() {
        return false;
    }
    let mut time = false;
    let mut previous = 0;
    while !input.is_empty() {
        if input[0] == b'T' {
            if time {
                return false;
            }
            time = true;
            previous = 0;
            input = &input[1..];
            if input.is_empty() {
                return false;
            }
        }
        if matches!(input.first(), Some(b'+' | b'-')) {
            input = &input[1..];
        }
        let digits = input.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 {
            return false;
        }
        input = &input[digits..];
        let fraction = matches!(input.first(), Some(b'.' | b','));
        if fraction {
            input = &input[1..];
            let digits = input.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits == 0 {
                return false;
            }
            input = &input[digits..];
        }
        let Some((&unit, rest)) = input.split_first() else {
            return false;
        };
        let order = match (time, unit) {
            (false, b'Y') | (true, b'H') => 1,
            (_, b'M') => 2,
            (false, b'W') | (true, b'S') => 3,
            (false, b'D') => 4,
            _ => return false,
        };
        if order <= previous || (fraction && !rest.is_empty()) {
            return false;
        }
        previous = order;
        input = rest;
    }
    true
}

/// Match raw Zod time syntax with a bounded admitted fractional precision.
/// Positive precision preserves Zod's repeated seconds-group acceptance.
#[must_use]
pub fn time(value: &str, precision: Option<u16>) -> bool {
    let bytes = value.as_bytes();
    let pair = |v: &[u8], max: u8| {
        v.len() == 2 && v.iter().all(u8::is_ascii_digit) && (v[0] - b'0') * 10 + v[1] - b'0' <= max
    };
    if bytes.len() < 5 || bytes[2] != b':' || !pair(&bytes[..2], 23) || !pair(&bytes[3..5], 59) {
        return false;
    }
    let mut rest = &bytes[5..];
    if rest.is_empty() {
        return precision.is_none_or(|p| p == 0);
    }
    loop {
        if rest.len() < 3 || rest[0] != b':' || !pair(&rest[1..3], 59) {
            return false;
        }
        rest = &rest[3..];
        match precision {
            Some(0) => return rest.is_empty(),
            Some(count) => {
                let count = usize::from(count);
                if rest.len() < count + 1
                    || rest[0] != b'.'
                    || !rest[1..=count].iter().all(u8::is_ascii_digit)
                {
                    return false;
                }
                rest = &rest[count + 1..];
                if rest.is_empty() {
                    return true;
                }
            }
            None => {
                return rest.is_empty()
                    || (rest.len() > 1
                        && rest[0] == b'.'
                        && rest[1..].iter().all(u8::is_ascii_digit));
            }
        }
    }
}

/// Raw Zod datetime grammar; timezone digits are lexical, not range-checked.
#[must_use]
pub fn datetime(value: &str, precision: Option<u16>, local: bool, offset: bool) -> bool {
    let Some(date_part) = value.get(..10) else {
        return false;
    };
    if !date(date_part) || value.as_bytes().get(10) != Some(&b'T') {
        return false;
    }
    let clock = &value[11..];
    if let Some(clock) = clock.strip_suffix('Z') {
        return time(clock, precision);
    }
    if offset {
        for size in [5, 6] {
            if let Some(start) = clock.len().checked_sub(size)
                && let Some(zone) = clock.get(start..)
            {
                let b = zone.as_bytes();
                if matches!(b[0], b'+' | b'-')
                    && b[1..].iter().enumerate().all(|(i, b)| {
                        if size == 6 && i == 2 {
                            *b == b':'
                        } else {
                            b.is_ascii_digit()
                        }
                    })
                {
                    return time(&clock[..start], precision);
                }
            }
        }
    }
    local && time(clock, precision)
}

/// Match Zod's IP grammar, including its legacy mixed-address and zone syntax.
#[must_use]
pub fn ip(value: &str, version: Option<u8>) -> bool {
    if !matches!(version, None | Some(4 | 6)) {
        return false;
    }
    (version != Some(6) && value.parse::<std::net::Ipv4Addr>().is_ok())
        || (version != Some(4) && ipv6(value))
}
fn ipv6(value: &str) -> bool {
    if let Some((address, zone)) = value.split_once('%') {
        if zone.is_empty() || !zone.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return false;
        }
        let Some(groups) = address.strip_prefix("fe80:") else {
            return false;
        };
        if groups.is_empty() {
            return true;
        }
        let Some(groups) = groups.strip_prefix(':') else {
            return false;
        };
        let mut parts = groups.split(':');
        return parts.clone().take(5).count() <= 4
            && parts.all(|part| part.len() <= 4 && part.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    if !value.contains('.') {
        return value.parse::<std::net::Ipv6Addr>().is_ok();
    }
    let Some((head, tail)) = value.rsplit_once(':') else {
        return false;
    };
    let mut parts = tail.split('.');
    if parts.clone().take(5).count() != 4
        || !parts.all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part.len() != 3 || !part.starts_with('0'))
                && part.parse::<u8>().is_ok()
        })
    {
        return false;
    }
    // Exact lowercase mapped prefix and optional zero group in Zod's first branch.
    if head == ":" || head == "::ffff" {
        return true;
    }
    if let Some(zero) = head.strip_prefix("::ffff:") {
        return !zero.is_empty() && zero.len() <= 4 && zero.bytes().all(|b| b == b'0');
    }
    let Some(groups) = head.strip_suffix(':') else {
        return false;
    };
    let mut parts = groups.split(':');
    (1..=4).contains(&parts.clone().take(5).count())
        && parts.all(|part| {
            !part.is_empty() && part.len() <= 4 && part.bytes().all(|b| b.is_ascii_hexdigit())
        })
}
/// Zod CIDR validates lexical prefix widths without normalizing host bits.
#[must_use]
pub fn cidr(value: &str, version: Option<u8>) -> bool {
    if !matches!(version, None | Some(4 | 6)) {
        return false;
    }
    let Some((address, width)) = value.split_once('/') else {
        return false;
    };
    if width.is_empty()
        || width.len() > 3
        || !width.bytes().all(|b| b.is_ascii_digit())
        || (width.len() > 1 && width.starts_with('0'))
    {
        return false;
    }
    let Ok(width) = width.parse::<u8>() else {
        return false;
    };
    (version != Some(6) && width <= 32 && ip(address, Some(4)))
        || (version != Some(4) && width <= 128 && ip(address, Some(6)))
}

mod emoji_table;
/// Match raw Zod emoji property membership against Node24.2 / Unicode16.
/// Individual emoji components are accepted; this is not grapheme validation.
#[must_use]
pub fn emoji(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|ch| {
            let code = u32::from(ch);
            let ranges = emoji_table::RANGES;
            let index = ranges.partition_point(|(_, end)| *end < code);
            ranges.get(index).is_some_and(|(start, _)| *start <= code)
        })
}

mod jwt;
/// Match raw Zod JWT structure/header checks. No signature or claims are verified.
#[must_use]
pub fn jwt(value: &str, algorithm: Option<&str>) -> bool {
    jwt::accepts(value, algorithm)
}
