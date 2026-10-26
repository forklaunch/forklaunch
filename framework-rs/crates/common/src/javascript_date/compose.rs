// Copyright 2011 the V8 project authors. All rights reserved.
// Derived from Node v24.2.0 V8 date parser. See LICENSE-V8 and provenance.json.
use super::State;
pub(super) fn compose(mut v: State) -> Option<(i64, Option<i32>)> {
    if v.day.is_empty() {
        return None;
    }
    v.day.resize(3, 1);
    let (mut year, month, day) = if let Some(month) = v.month {
        if (1..=31).contains(&v.day[0]) {
            (v.day[1], month, v.day[0])
        } else {
            (v.day[0], month, v.day[1])
        }
    } else if v.iso || !(1..=31).contains(&v.day[0]) {
        (v.day[0], v.day[1], v.day[2])
    } else {
        (v.day[2], v.day[0], v.day[1])
    };
    if !v.iso {
        if (0..=49).contains(&year) {
            year += 2000;
        } else if (50..=99).contains(&year) {
            year += 1900;
        }
    }
    if !(-1_000_000..=1_000_000).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
    {
        return None;
    }
    v.time.resize(4, 0);
    if let Some(offset) = v.hour_offset {
        if !(0..=12).contains(&v.time[0]) {
            return None;
        }
        v.time[0] = v.time[0] % 12 + offset;
    }
    let (h, m, se, ms) = (v.time[0], v.time[1], v.time[2], v.time[3]);
    if !((0..=23).contains(&h)
        && (0..=59).contains(&m)
        && (0..=59).contains(&se)
        && (0..=999).contains(&ms))
        && !(h == 24 && m == 0 && se == 0 && ms == 0)
    {
        return None;
    }
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from(month) + if month > 2 { -3 } else { 9 };
    let days =
        era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + (153 * mp + 2) / 5 + i64::from(day)
            - 1
            - 719_468;
    let wall = days * 86_400_000
        + i64::from(h) * 3_600_000
        + i64::from(m) * 60_000
        + i64::from(se) * 1000
        + i64::from(ms);
    let offset = if let Some(sign) = v.sign {
        let total = u32::try_from(v.zone_hour.unwrap_or(0))
            .ok()?
            .wrapping_mul(3600)
            .wrapping_add(
                u32::try_from(v.zone_min.unwrap_or(0))
                    .ok()?
                    .wrapping_mul(60),
            );
        if total > 0x7fff_ffff {
            return None;
        }
        Some(sign * i32::try_from(total).ok()?)
    } else {
        None
    };
    Some((wall, offset))
}
