//! Private unmodified jiff-tzdb 0.1.4 / `IANA2025b` data. See tzdb licenses.
#[path = "tzdb/tzname.rs"]
#[rustfmt::skip]
// Preserve the upstream generated offsets byte-for-byte.
#[allow(clippy::unreadable_literal)]
mod tzname;
static DATA: &[u8] = include_bytes!("tzdb/concatenated-zoneinfo.dat");

pub(super) fn get(name: &str) -> Option<&'static [u8]> {
    // ICU uses exact system identifiers. Do not reuse jiff's case-folded lookup.
    tzname::TZNAME_TO_OFFSET
        .iter()
        .find_map(|(candidate, range)| (*candidate == name).then(|| &DATA[range.clone()]))
}
pub(super) const VERSION: Option<&str> = tzname::VERSION;
