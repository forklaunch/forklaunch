// Copyright 2011 the V8 project authors. All rights reserved.
// Derived from Node v24.2.0 V8 date parser. See LICENSE-V8 and provenance.json.
use super::{
    Kind, Scanner, State,
    iso::{iso, milliseconds},
};
pub(super) fn parse(input: &[u16]) -> Option<(i64, Option<i32>)> {
    let mut s = Scanner::new(input);
    let mut v = State::default();
    let mut token = iso(&mut s, &mut v);
    if token.kind == Kind::Invalid && token.len == 0 {
        return None;
    }
    let mut read_number = !v.day.is_empty();
    while token.kind != Kind::End {
        if token.number() {
            read_number = true;
            number(&mut s, &mut v, token)?;
        } else if matches!(
            token.kind,
            Kind::Month | Kind::Zone | Kind::Separator | Kind::AmPm | Kind::Invalid
        ) {
            match token.kind {
                Kind::AmPm if !v.time.is_empty() => v.hour_offset = Some(token.n),
                Kind::Month => {
                    v.month = Some(token.n);
                    s.skip(b'-');
                }
                Kind::Zone if read_number => v.set_zone(token.n),
                _ => {
                    if read_number || s.look.number() {
                        return None;
                    }
                }
            }
        } else if token.sign()
            && ((v.zone_hour == Some(0) && v.zone_min == Some(0)) || !v.time.is_empty())
        {
            v.sign = Some(44 - token.n);
            let (n, len) = if s.look.number() {
                let t = s.next();
                (t.n, t.len)
            } else {
                (0, 0)
            };
            read_number = true;
            if s.look.symbol(b':') {
                v.zone_hour = Some(n);
                v.zone_min = None;
            } else if len == 1 || len == 2 {
                v.zone_hour = Some(n);
                v.zone_min = Some(0);
            } else if len == 3 || len == 4 {
                v.zone_hour = Some(n / 100);
                v.zone_min = Some(n % 100);
            } else {
                return None;
            }
        } else if (token.sign() || token.symbol(b')')) && read_number {
            return None;
        }
        token = s.next();
    }
    super::compose::compose(v)
}

fn number(s: &mut Scanner<'_>, v: &mut State, token: super::Token) -> Option<()> {
    let n = token.n;
    if s.skip(b':') {
        if s.skip(b':') {
            if !v.time.is_empty() {
                return None;
            }
            v.time_add(n);
            v.time_add(0);
        } else {
            if !v.time_add(n) {
                return None;
            }
            if s.look.symbol(b'.') {
                s.next();
            }
        }
    } else if s.skip(b'.') && v.expecting(n) {
        v.time_add(n);
        if !s.look.number() {
            return None;
        }
        let ms = milliseconds(s.next());
        if ms < 0 {
            return None;
        }
        v.time_final(ms);
    } else if v.zone_hour.is_some() && v.zone_min.is_none() && (0..=59).contains(&n) {
        v.zone_min = Some(n);
    } else if v.expecting(n) {
        v.time_final(n);
        let p = s.look;
        if p.kind != Kind::End && p.kind != Kind::Space && !p.z() && !p.sign() {
            return None;
        }
    } else {
        if !v.day_add(n) {
            return None;
        }
        s.skip(b'-');
    }
    Some(())
}
