//! Line-oriented UTF-16 wire oracle. No JS runtime is used by the library.
use forklaunch_common::javascript_date::{DateError, LocalTimeZone, parse_utf16, parse_utf16_host};
use std::io::{self, BufRead};

fn main() {
    let argument = std::env::args().nth(1).expect("zone identifier or --host");
    let zone =
        (argument != "--host").then(|| LocalTimeZone::named(&argument).expect("known exact zone"));
    for line in io::stdin().lock().lines() {
        let line = line.expect("oracle input");
        assert_eq!(line.len() % 4, 0, "UTF-16 hex units");
        let units: Vec<_> = (0..line.len())
            .step_by(4)
            .map(|start| u16::from_str_radix(&line[start..start + 4], 16).expect("hex unit"))
            .collect();
        let result = if let Some(zone) = &zone {
            parse_utf16(&units, zone)
        } else {
            parse_utf16_host(&units)
        };
        match result {
            Ok(value) => println!("{value}"),
            Err(DateError::InvalidDate) => println!("null"),
            Err(DateError::TimeZoneUnavailable) => println!("timezone-unavailable"),
        }
    }
}
