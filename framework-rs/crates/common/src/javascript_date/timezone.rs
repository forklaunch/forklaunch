// © 2016 and later: Unicode, Inc. and others.
// Copyright (C) 1997-2016, International Business Machines Corporation and
// others. All Rights Reserved. Derived ICU77 code: see LICENSE-ICU.
//! Host discovery follows ICU 77's Unix `detectHostTimeZone/uprv_tzname` flow.
//! Pinned named-zone rules are independent of the host's `TZif` version. Host `TZif`
//! is consulted only for the standard-offset fallback used by ICU itself.
#[cfg(unix)]
use std::{collections::HashSet, path::Path};
use tz::TimeZone;
#[cfg(unix)]
use tz::timezone::TransitionRule;

#[cfg(unix)]
fn plausible_olson(id: &str) -> bool {
    let bytes = id.as_bytes();
    let first = bytes
        .iter()
        .position(|c| c.is_ascii_digit() || *c == b',')
        .unwrap_or(bytes.len());
    let mut end = first;
    while end < bytes.len() && end < first + 2 && bytes[end].is_ascii_digit() {
        end += 1;
    }
    end == bytes.len() || matches!(id, "PST8PDT" | "MST7MDT" | "CST6CDT" | "EST5EDT")
}
#[cfg(unix)]
fn strip_zone_prefix(id: &str) -> &str {
    id.strip_prefix("posix/")
        .or_else(|| id.strip_prefix("right/"))
        .unwrap_or(id)
}
#[cfg(unix)]
fn linked_system_id() -> Option<String> {
    // ICU first resolves /etc/localtime; on macOS zoneinfo.default it reads the
    // symlink itself so the canonical IANA tail is retained.
    [
        std::fs::canonicalize("/etc/localtime"),
        std::fs::read_link("/etc/localtime"),
    ]
    .into_iter()
    .filter_map(Result::ok)
    .find_map(|path| {
        let text = path.to_str()?;
        let (_, id) = text.split_once("/zoneinfo/")?;
        let id = strip_zone_prefix(id);
        (id != "posixrules" && plausible_olson(id)).then(|| id.to_owned())
    })
}
pub(super) fn pinned(id: &str) -> Option<TimeZone> {
    TimeZone::from_tz_data(super::data::get(id)?).ok()
}
#[cfg(unix)]
fn standard_offset(zone: &TimeZone) -> i32 {
    match zone.as_ref().extra_rule() {
        Some(TransitionRule::Fixed(kind)) => kind.ut_offset(),
        Some(TransitionRule::Alternate(rule)) => rule.std().ut_offset(),
        None => zone
            .as_ref()
            .local_time_types()
            .iter()
            .rev()
            .find(|kind| !kind.is_dst())
            .map_or(0, tz::LocalTimeType::ut_offset),
    }
}
#[cfg(unix)]
// tz-rs reserves bare "localtime" for /etc/localtime; libc TZ resolves that
// literal as an ordinary zoneinfo filename, so force its file lookup spelling.
fn host_offset(tz: Option<&str>) -> i32 {
    let parsed = match tz {
        Some("") => return 0,
        Some(value) => {
            let spec = if value == "localtime" {
                ":localtime"
            } else {
                value
            };
            // glibc reads `:name` as a zoneinfo file and, when no such file
            // exists, parses `name` as a POSIX TZ string (TZ=":GMT+8" is UTC-8 on
            // a host without a GMT+8 file). tz-rs stops at the missing file.
            TimeZone::from_posix_tz(spec).or_else(|error| match spec.strip_prefix(':') {
                Some(rest) if !rest.is_empty() && rest != "localtime" => {
                    TimeZone::from_posix_tz(rest)
                }
                _ => Err(error),
            })
        }
        None => TimeZone::local(),
    };
    parsed.as_ref().map_or(0, standard_offset)
}
#[cfg(unix)]
pub fn discover(tz: Option<&str>) -> Result<TimeZone, &'static str> {
    let id = match tz.filter(|id| plausible_olson(id)) {
        Some(id) => strip_zone_prefix(id.strip_prefix(':').unwrap_or(id)).to_owned(),
        None => linked_system_id()
            .or_else(copied_system_id)
            .or_else(|| abbreviated_system_id(tz))
            .ok_or("host timezone identity unavailable")?,
    };
    let offset = host_offset(tz);
    if let Some(zone) = pinned(&id)
        && (!(3..=4).contains(&id.len()) || standard_offset(&zone) == offset)
    {
        return Ok(zone);
    }
    if id.is_empty() {
        return Ok(TimeZone::utc());
    }
    TimeZone::fixed(offset).map_err(|_| "host timezone offset unsupported")
}

// ICU searches the host zoneinfo tree when /etc/localtime is a copied file.
// Traverse in filesystem order like readdir, with a visited-directory guard.
#[cfg(unix)]
fn copied_system_id() -> Option<String> {
    let root = Path::new("/usr/share/zoneinfo");
    let expected = std::fs::read("/etc/localtime").ok()?;
    let mut stack = vec![std::fs::read_dir(root).ok()?];
    let mut visited = HashSet::new();
    while let Some(entries) = stack.last_mut() {
        let Some(entry) = entries.next() else {
            stack.pop();
            continue;
        };
        let Ok(entry) = entry else {
            continue;
        };
        if matches!(entry.file_name().to_str(), Some("localtime" | "posixrules")) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let Ok(canonical) = path.canonicalize() else {
                continue;
            };
            if visited.insert(canonical)
                && let Ok(directory) = std::fs::read_dir(&path)
            {
                stack.push(directory);
            }
        } else if path
            .metadata()
            .is_ok_and(|m| m.len() == expected.len() as u64)
            && std::fs::read(&path).is_ok_and(|data| data == expected)
        {
            let id = path.strip_prefix(root).ok()?.to_str()?;
            return Some(strip_zone_prefix(id).to_owned());
        }
    }
    None
}

#[cfg(unix)]
fn abbreviated_system_id(environment: Option<&str>) -> Option<String> {
    let zone = match environment {
        Some(value) => TimeZone::from_posix_tz(if value == "localtime" {
            ":localtime"
        } else {
            value
        }),
        None => TimeZone::local(),
    }
    .ok()?;
    let kinds = zone.as_ref();
    let standard = kinds
        .local_time_types()
        .iter()
        .rev()
        .find(|kind| !kind.is_dst())?;
    let daylight = kinds
        .local_time_types()
        .iter()
        .rev()
        .find(|kind| kind.is_dst())
        .unwrap_or(standard);
    let daylight_type = if zone.find_local_time_type(1_198_332_540).ok()?.is_dst() {
        2
    } else {
        u8::from(zone.find_local_time_type(1_182_478_260).ok()?.is_dst())
    };
    let offset = -standard_offset(&zone);
    ICU_SHORT_ZONES
        .iter()
        .find(|(seconds, season, std_name, dst_name, _)| {
            *seconds == offset
                && *season == daylight_type
                && *std_name == standard.time_zone_designation()
                && *dst_name == daylight.time_zone_designation()
        })
        .map_or_else(
            || Some(standard.time_zone_designation().to_owned()),
            |row| Some(row.4.to_owned()),
        )
}

// ICU77 mapping, retained under LICENSE-ICU.
#[cfg(unix)]
const ICU_SHORT_ZONES: &[(i32, u8, &str, &str, &str)] = &[
    (-45900, 2, "CHAST", "CHADT", "Pacific/Chatham"),
    (-43200, 1, "PETT", "PETST", "Asia/Kamchatka"),
    (-43200, 2, "NZST", "NZDT", "Pacific/Auckland"),
    (-43200, 1, "ANAT", "ANAST", "Asia/Anadyr"),
    (-39600, 1, "MAGT", "MAGST", "Asia/Magadan"),
    (-37800, 2, "LHST", "LHST", "Australia/Lord_Howe"),
    (-36000, 2, "EST", "EST", "Australia/Sydney"),
    (-36000, 1, "SAKT", "SAKST", "Asia/Sakhalin"),
    (-36000, 1, "VLAT", "VLAST", "Asia/Vladivostok"),
    (-34200, 2, "CST", "CST", "Australia/South"),
    (-32400, 1, "YAKT", "YAKST", "Asia/Yakutsk"),
    (-32400, 1, "CHOT", "CHOST", "Asia/Choibalsan"),
    (-31500, 2, "CWST", "CWST", "Australia/Eucla"),
    (-28800, 1, "IRKT", "IRKST", "Asia/Irkutsk"),
    (-28800, 1, "ULAT", "ULAST", "Asia/Ulaanbaatar"),
    (-28800, 2, "WST", "WST", "Australia/West"),
    (-25200, 1, "HOVT", "HOVST", "Asia/Hovd"),
    (-25200, 1, "KRAT", "KRAST", "Asia/Krasnoyarsk"),
    (-21600, 1, "NOVT", "NOVST", "Asia/Novosibirsk"),
    (-21600, 1, "OMST", "OMSST", "Asia/Omsk"),
    (-18000, 1, "YEKT", "YEKST", "Asia/Yekaterinburg"),
    (-14400, 1, "SAMT", "SAMST", "Europe/Samara"),
    (-14400, 1, "AMT", "AMST", "Asia/Yerevan"),
    (-14400, 1, "AZT", "AZST", "Asia/Baku"),
    (-10800, 1, "AST", "ADT", "Asia/Baghdad"),
    (-10800, 1, "MSK", "MSD", "Europe/Moscow"),
    (-10800, 1, "VOLT", "VOLST", "Europe/Volgograd"),
    (-7200, 0, "EET", "CEST", "Africa/Tripoli"),
    (-7200, 1, "EET", "EEST", "Europe/Athens"),
    (-7200, 1, "IST", "IDT", "Asia/Jerusalem"),
    (-3600, 0, "CET", "WEST", "Africa/Algiers"),
    (-3600, 2, "WAT", "WAST", "Africa/Windhoek"),
    (0, 1, "GMT", "IST", "Europe/Dublin"),
    (0, 1, "GMT", "BST", "Europe/London"),
    (0, 0, "WET", "WEST", "Africa/Casablanca"),
    (0, 0, "WET", "WET", "Africa/El_Aaiun"),
    (3600, 1, "AZOT", "AZOST", "Atlantic/Azores"),
    (3600, 1, "EGT", "EGST", "America/Scoresbysund"),
    (10800, 1, "PMST", "PMDT", "America/Miquelon"),
    (10800, 2, "UYT", "UYST", "America/Montevideo"),
    (10800, 1, "WGT", "WGST", "America/Godthab"),
    (10800, 2, "BRT", "BRST", "Brazil/East"),
    (12600, 1, "NST", "NDT", "America/St_Johns"),
    (14400, 1, "AST", "ADT", "Canada/Atlantic"),
    (14400, 2, "AMT", "AMST", "America/Cuiaba"),
    (14400, 2, "CLT", "CLST", "Chile/Continental"),
    (14400, 2, "FKT", "FKST", "Atlantic/Stanley"),
    (14400, 2, "PYT", "PYST", "America/Asuncion"),
    (18000, 1, "CST", "CDT", "America/Havana"),
    (18000, 1, "EST", "EDT", "US/Eastern"),
    (21600, 2, "EAST", "EASST", "Chile/EasterIsland"),
    (21600, 0, "CST", "MDT", "Canada/Saskatchewan"),
    (21600, 0, "CST", "CDT", "America/Guatemala"),
    (21600, 1, "CST", "CDT", "US/Central"),
    (25200, 1, "MST", "MDT", "US/Mountain"),
    (28800, 0, "PST", "PST", "Pacific/Pitcairn"),
    (28800, 1, "PST", "PDT", "US/Pacific"),
    (32400, 1, "AKST", "AKDT", "US/Alaska"),
    (36000, 1, "HAST", "HADT", "US/Aleutian"),
];
