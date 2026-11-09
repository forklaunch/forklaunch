use super::*;

fn parse(input: &str, zone: &str) -> Result<i64, DateError> {
    parse_utf16(
        &input.encode_utf16().collect::<Vec<_>>(),
        &LocalTimeZone::named(zone).unwrap(),
    )
}
#[test]
fn local_gap_fold_and_pre_epoch_offsets() {
    for (input, zone, expected) in [
        (
            "2026-03-08T02:30",
            "America/Los_Angeles",
            "2026-03-08T10:30Z",
        ),
        (
            "2026-11-01T01:30",
            "America/Los_Angeles",
            "2026-11-01T08:30Z",
        ),
        ("2026-03-08T02:30", "America/New_York", "2026-03-08T07:30Z"),
        ("2026-11-01T01:30", "America/New_York", "2026-11-01T05:30Z"),
        ("2026-01-01T00:00", "Asia/Kathmandu", "2025-12-31T18:15Z"),
        ("1900-01-01T00:00", "Asia/Kathmandu", "1899-12-31T18:18:44Z"),
    ] {
        assert_eq!(parse(input, zone), parse(expected, "UTC"), "{input} {zone}");
    }
}
#[test]
fn legacy_continuation_and_timeclip_boundaries() {
    for (input, canonical) in [
        ("10/15/2026", "2026-10-15T00:00:00Z"),
        ("2026-1-2", "2026-01-02T00:00:00Z"),
        (
            "2026-01-01T00:00:00.00000000123456789Z",
            "2026-01-01T00:00:00.123Z",
        ),
        ("49", "2049-01-01T00:00:00Z"),
        ("50", "1950-01-01T00:00:00Z"),
        ("(nested(comment)) Jan 1 2026", "2026-01-01T00:00:00Z"),
        ("2026-02-31", "2026-03-03T00:00:00Z"),
        ("2026-01-01T24:00:00.0000Z", "2026-01-02T00:00:00Z"),
        ("2026-01-01T00:00:00.123999999Z", "2026-01-01T00:00:00.123Z"),
    ] {
        assert_eq!(parse(input, "UTC"), parse(canonical, "UTC"), "{input}");
    }
    assert_eq!(parse("-271821-04-20T00:00:00.000Z", "UTC"), Ok(-TIME_CLIP));
    assert_eq!(parse("+275760-09-13T00:00:00.000Z", "UTC"), Ok(TIME_CLIP));
    for invalid in [
        "+275760-09-13T00:00:00.001Z",
        "-271821-04-19T23:59:59.999Z",
        "2026-01-01T24:00:00.0001Z",
        "2026-13-01",
        "not-a-date",
    ] {
        assert_eq!(
            parse(invalid, "UTC"),
            Err(DateError::InvalidDate),
            "{invalid}"
        );
    }
}
#[test]
fn timezone_identifier_is_exact_and_does_not_invent_utc() {
    assert!(LocalTimeZone::named("America/Los_Angeles").is_ok());
    assert!(LocalTimeZone::named("america/los_angeles").is_err());
    assert!(LocalTimeZone::named("No/SuchZone").is_err());
    assert_eq!(TIME_ZONE_VERSION, Some("2025b"));
}

#[test]
fn explicit_offset_and_date_only_never_discover_host_timezone() {
    for input in ["2026-01-01", "2026-01-01T12:00Z", "Jan 1 2026 GMT"] {
        let units = input.encode_utf16().collect::<Vec<_>>();
        assert!(
            finish(legacy::parse(&units).unwrap(), |_| panic!(
                "unexpected host discovery"
            ))
            .is_ok()
        );
    }
}
