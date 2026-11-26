//! Exact bounded string grammars from the repository's Zod v3 factories.

/// Runtime formats matching Zod v3, not generic JSON Schema format semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZodStringFormat {
    /// Hyphenated hex groups; no UUID version or variant restriction.
    Uuid,
    /// Zod's ASCII email grammar, without DNS, delivery or length validation.
    Email,
    Cuid,
    Cuid2,
    Ulid,
    Nanoid,
    Base64,
    Base64Url,
    Date,
    Duration,
    Time {
        precision: ZodTimePrecision,
    },
    DateTime {
        precision: ZodTimePrecision,
        local: bool,
        offset: bool,
    },
    Ip(Option<ZodIpVersion>),
    Cidr(Option<ZodIpVersion>),
    Emoji,
    Jwt,
    /// Node 24.2.0 / Ada 3.2.4 URL acceptance; never normalizes output.
    /// Requires the opt-in `zod-uri` feature and a C++20 build toolchain.
    #[cfg(feature = "zod-uri")]
    Uri,
}

impl ZodStringFormat {
    pub(super) fn rejection(self, value: &str) -> Option<String> {
        let (valid, name) = match self {
            Self::Uuid => (uuid(value), "uuid"),
            Self::Email => (email(value), "email"),
            Self::Cuid => (cuid(value), "cuid"),
            Self::Cuid2 => (cuid2(value), "cuid2"),
            Self::Ulid => (ulid(value), "ulid"),
            Self::Nanoid => (nanoid(value), "nanoid"),
            Self::Base64 => (base64(value), "base64"),
            Self::Base64Url => (base64url(value), "base64url"),
            Self::Date => (date(value), "date"),
            Self::Duration => (duration(value), "duration"),
            Self::Time { precision } => (time(value, precision.0), "time"),
            Self::DateTime {
                precision,
                local,
                offset,
            } => (datetime(value, precision.0, local, offset), "datetime"),
            Self::Ip(version) => (ip(value, version.map(ZodIpVersion::number)), "ip"),
            Self::Cidr(version) => (cidr(value, version.map(ZodIpVersion::number)), "cidr"),
            Self::Emoji => (emoji(value), "emoji"),
            Self::Jwt => (jwt(value, None), "jwt"),
            #[cfg(feature = "zod-uri")]
            Self::Uri => (ada_url::Url::can_parse(value, None), "url"),
        };
        (!valid).then(|| format!("Invalid {name}"))
    }
}

/// Unsupported format registration, distinct from a value's parse errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZodStringFormatParameterError {
    TimePrecision,
    IpVersion,
}
impl std::fmt::Display for ZodStringFormatParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TimePrecision => {
                "time precision must be absent or an integer from 0 through 1000"
            }
            Self::IpVersion => "IP version must be v4 or v6",
        })
    }
}
impl std::error::Error for ZodStringFormatParameterError {}

/// A checked time precision. `None` preserves Zod's unspecified precision.
/// The finite 0..1000 profile matches the schema capability's admission bound.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ZodTimePrecision(Option<u16>);
impl ZodTimePrecision {
    /// Construct a supported precision without interpreting invalid bounds as input errors.
    /// # Errors
    /// Rejects negative values and values above 1000.
    pub fn new(value: Option<i64>) -> Result<Self, ZodStringFormatParameterError> {
        value
            .map(|value| {
                u16::try_from(value)
                    .ok()
                    .filter(|value| *value <= 1000)
                    .ok_or(ZodStringFormatParameterError::TimePrecision)
            })
            .transpose()
            .map(Self)
    }
}

/// Exact Zod IP/CIDR version selection; `None` on the format accepts either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZodIpVersion {
    V4,
    V6,
}
impl ZodIpVersion {
    const fn number(self) -> u8 {
        match self {
            Self::V4 => 4,
            Self::V6 => 6,
        }
    }
}
impl TryFrom<&str> for ZodIpVersion {
    type Error = ZodStringFormatParameterError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "v4" => Ok(Self::V4),
            "v6" => Ok(Self::V6),
            _ => Err(Self::Error::IpVersion),
        }
    }
}

use forklaunch_common::zod_string_format::{
    base64, base64url, cidr, cuid, cuid2, date, datetime, duration, email, emoji, ip, jwt, nanoid,
    time, ulid, uuid,
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn format_parameter_admission_is_separate_from_input_errors() {
        for precision in [None, Some(0), Some(1), Some(1000)] {
            assert!(ZodTimePrecision::new(precision).is_ok());
        }
        for precision in [-1, 1001, i64::MAX] {
            assert_eq!(
                ZodTimePrecision::new(Some(precision)),
                Err(ZodStringFormatParameterError::TimePrecision)
            );
        }
        assert_eq!(ZodIpVersion::try_from("v4"), Ok(ZodIpVersion::V4));
        assert_eq!(ZodIpVersion::try_from("v6"), Ok(ZodIpVersion::V6));
        assert_eq!(
            ZodIpVersion::try_from("V4"),
            Err(ZodStringFormatParameterError::IpVersion)
        );
    }

    #[test]
    fn jwt_algorithm_bytes_share_composition_admission_and_repeated_work_budgets() {
        use crate::typescript_coerce::{
            ZodJsonSchema as Schema, ZodJsonUnsupported, ZodStringCheck, ZodStringRule, zod_json,
        };
        let oversized = "a".repeat(16 * 1024 * 1024 + 1);
        let check = ZodStringCheck {
            rule: ZodStringRule::Jwt {
                algorithm: Some(&oversized),
            },
            message: None,
        };
        assert_eq!(
            zod_json(Some(&serde_json::json!("x")), &Schema::String(vec![check])),
            Err(ZodJsonUnsupported::SchemaBytes)
        );
        #[cfg(feature = "zod-regex")]
        assert_eq!(
            zod_json(
                Some(&serde_json::json!("x")),
                &Schema::CheckedString(vec![crate::typescript_coerce::ZodJsonStringCheck::Scalar(
                    check
                )])
            ),
            Err(ZodJsonUnsupported::SchemaBytes)
        );
        let algorithm = "a".repeat(1024 * 1024);
        let repeated = Schema::Array {
            element: Box::new(Schema::String(vec![ZodStringCheck {
                rule: ZodStringRule::Jwt {
                    algorithm: Some(&algorithm),
                },
                message: None,
            }])),
            checks: vec![],
        };
        let input = serde_json::json!(vec!["x"; 100]);
        assert_eq!(
            zod_json(Some(&input), &repeated),
            Err(ZodJsonUnsupported::EvaluationBytes)
        );
    }
    #[test]
    fn source_grammar_preserves_nonstandard_acceptance_and_rejection() {
        assert!(uuid("FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"));
        assert!(!uuid("urn:uuid:00000000-0000-0000-0000-000000000000"));
        assert!(email("a'b@b-.co"));
        for invalid in [
            "a%b@b.co",
            "a'@b.co",
            "a..b@b.co",
            "a@-b.co",
            "é@b.co",
            "a@b.co\n",
        ] {
            assert!(!email(invalid), "{invalid:?}");
        }
    }
}

#[cfg(all(test, feature = "zod-uri"))]
mod uri_tests {
    use super::ZodStringFormat;
    use crate::typescript_coerce::{
        ZodStringCheck, ZodStringRule, ZodStringStep, zod_string_checks,
    };
    use serde_json::json;
    #[test]
    fn node_pinned_unicode_and_original_string_are_preserved() {
        for value in [
            " https://example.com/a b ",
            "http://a\u{061d}b.test",
            "http://xn--ab-hsd.test",
            "http://a%D8%9Db.test",
            "file:///C:/a",
            "mailto:a@b",
        ] {
            let input = json!(value);
            assert_eq!(
                zod_string_checks(
                    Some(&input),
                    &[ZodStringCheck {
                        rule: ZodStringRule::Format(ZodStringFormat::Uri),
                        message: None
                    }]
                ),
                Ok(value)
            );
        }
        for value in [
            "relative/path",
            "http://a\u{0c5c}b.test",
            "http://a%E0%B1%9Cb.test",
            "http://[::1",
            "http://host:65536",
        ] {
            assert_eq!(
                ZodStringFormat::Uri.rejection(value).as_deref(),
                Some("Invalid url")
            );
        }
    }
    #[test]
    fn ordered_uri_errors_and_literal_override_preserve_existing_string_semantics() {
        let input = json!(" invalid ");
        let checks = [
            ZodStringCheck {
                rule: ZodStringRule::Format(ZodStringFormat::Uri),
                message: Some("\0custom\n💡"),
            },
            ZodStringCheck {
                rule: ZodStringRule::Step(ZodStringStep::Trim),
                message: None,
            },
            ZodStringCheck {
                rule: ZodStringRule::EndsWith("z"),
                message: None,
            },
        ];
        let errors = zod_string_checks(Some(&input), &checks).unwrap_err();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].message, "\0custom\n💡");
        assert_eq!(input, " invalid ");
        assert_eq!(
            zod_string_checks(None, &checks).unwrap_err()[0].message,
            "Required"
        );
    }
}
