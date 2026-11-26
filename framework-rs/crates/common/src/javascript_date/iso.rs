// Copyright 2011 the V8 project authors. All rights reserved.
// Derived from Node v24.2.0 V8 date parser. See LICENSE-V8 and provenance.json.
use super::{Kind, Scanner, State, Token};
pub(super) fn milliseconds(t: Token) -> i32 {
    match t.len {
        0 => 0,
        1 => t.n * 100,
        2 => t.n * 10,
        3 => t.n,
        n => t.n / 10_i32.pow(u32::try_from(n.min(9)).expect("at most nine digits") - 3),
    }
}
pub(super) fn iso(scanner: &mut Scanner<'_>, state: &mut State) -> Token {
    let invalid = Token::new(Kind::Invalid, -1, 0);
    if scanner.look.sign() {
        let sign = scanner.next();
        if !scanner.look.fixed(6) {
            return sign;
        }
        let year = scanner.next().n;
        let sign_n = 44 - sign.n;
        if sign_n < 0 && year == 0 {
            return sign;
        }
        state.day_add(sign_n * year);
    } else if scanner.look.fixed(4) {
        state.day_add(scanner.next().n);
    } else {
        return scanner.next();
    }
    if scanner.skip(b'-') {
        if !scanner.look.fixed(2) || !(1..=12).contains(&scanner.look.n) {
            return scanner.next();
        }
        state.day_add(scanner.next().n);
        if scanner.skip(b'-') {
            if !scanner.look.fixed(2) || !(1..=31).contains(&scanner.look.n) {
                return scanner.next();
            }
            state.day_add(scanner.next().n);
        }
    }
    if scanner.look.kind == Kind::Separator {
        scanner.next();
        if !scanner.look.fixed(2) || !(0..=24).contains(&scanner.look.n) {
            return invalid;
        }
        let h24 = scanner.look.n == 24;
        state.time_add(scanner.next().n);
        if !scanner.skip(b':') {
            return invalid;
        }
        if !scanner.look.fixed(2)
            || !(0..=59).contains(&scanner.look.n)
            || (h24 && scanner.look.n > 0)
        {
            return invalid;
        }
        state.time_add(scanner.next().n);
        if scanner.skip(b':') {
            if !scanner.look.fixed(2)
                || !(0..=59).contains(&scanner.look.n)
                || (h24 && scanner.look.n > 0)
            {
                return invalid;
            }
            state.time_add(scanner.next().n);
            if scanner.skip(b'.') {
                if !scanner.look.number() || (h24 && scanner.look.n > 0) {
                    return invalid;
                }
                state.time_add(milliseconds(scanner.next()));
            }
        }
        if scanner.look.z() {
            scanner.next();
            state.set_zone(0);
        } else if scanner.look.sign() {
            state.sign = Some(44 - scanner.next().n);
            if scanner.look.fixed(4) {
                let n = scanner.next().n;
                let (hours, minutes) = (n / 100, n % 100);
                if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
                    return invalid;
                }
                state.zone_hour = Some(hours);
                state.zone_min = Some(minutes);
            } else {
                if !scanner.look.fixed(2) || !(0..=23).contains(&scanner.look.n) {
                    return invalid;
                }
                state.zone_hour = Some(scanner.next().n);
                if !scanner.skip(b':')
                    || !scanner.look.fixed(2)
                    || !(0..=59).contains(&scanner.look.n)
                {
                    return invalid;
                }
                state.zone_min = Some(scanner.next().n);
            }
        }
        if scanner.look.kind != Kind::End {
            return invalid;
        }
    } else if scanner.look.kind != Kind::End {
        return scanner.next();
    }
    if state.zone_hour.is_none() && state.time.is_empty() {
        state.set_zone(0);
    }
    state.iso = true;
    Token::new(Kind::End, 0, 0)
}
