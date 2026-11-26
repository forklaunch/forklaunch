//! Explicit deterministic subset of configured TypeScript date preprocessing.
use chrono::{DateTime, SecondsFormat, Utc};
use forklaunch_core::contract::RouteContract;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_validator::typescript_coerce::{DateCoercionError, date_epoch_millis};
use serde_json::{Value, json};

/// Request-only profile. Responses retain their existing ISO date-time schemas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BillingRequestDateProfile {
    #[default]
    Legacy,
    /// Numeric epoch milliseconds or deterministic UTC/offset ISO grammar, limited
    /// to UTC years 0000 through 9999. Locale/offsetless strings remain unsupported.
    UtcIsoAndEpoch,
}
impl std::str::FromStr for BillingRequestDateProfile {
    type Err = BlueprintError;
    fn from_str(value: &str) -> BlueprintResult<Self> {
        match value {
            "legacy" => Ok(Self::Legacy),
            "utc-iso-and-epoch" => Ok(Self::UtcIsoAndEpoch),
            _ => Err(BlueprintError::new(
                "billing_configuration",
                "Unknown billing request date profile",
            )),
        }
    }
}
fn unsupported() -> BlueprintError {
    BlueprintError::new(
        "billing_date_profile_unsupported",
        "Date is outside the configured billing date profile",
    )
}
fn invalid() -> BlueprintError {
    BlueprintError::new("billing_validation", "Invalid billing request")
}
const FIELDS: [&str; 3] = ["startDate", "endDate", "expiresAt"];
const MIN: i64 = -62_167_219_200_000;
const MAX: i64 = 253_402_300_799_999;
impl BillingRequestDateProfile {
    /// Project only request date properties. JSON Schema describes admission, not
    /// an assertion that arbitrary strings satisfy the supported date grammar.
    /// # Errors
    /// Rejects malformed JSON in an explicitly supplied request schema.
    pub fn project(self, route: &mut RouteContract) -> BlueprintResult<()> {
        if self == Self::Legacy {
            return Ok(());
        }
        let Some(body) = route.request.body.as_mut() else {
            return Ok(());
        };
        let Some(text) = body.schema.json_schema.as_mut() else {
            return Ok(());
        };
        let mut schema: Value = serde_json::from_str(text).map_err(|_| invalid())?;
        let Some(properties) = schema["properties"].as_object_mut() else {
            return Ok(());
        };
        for name in FIELDS {
            if let Some(field) = properties.get_mut(name) {
                *field = json!({"anyOf":[{"type":"string"},{"type":"number"}],"description":"Opt-in billing UTC ISO/epoch-ms input; numeric fractions truncate toward zero. UTC years 0000..9999 only; locale and offsetless datetime strings unsupported. Null is not accepted.","x-forklaunch-date-profile":{"name":"utc-iso-and-epoch","normalizedEpochMillisMinimum":MIN,"normalizedEpochMillisMaximum":MAX,"grammar":"deterministic typescript_date ISO subset"}});
            }
        }
        *text = schema.to_string();
        Ok(())
    }
    /// No opaque provider payload traversal. Missing optional fields stay missing;
    /// required checks and all other field parsing remain the normal route's work.
    pub(crate) fn normalize(self, route: &RouteContract, input: &mut Value) -> BlueprintResult<()> {
        if self == Self::Legacy {
            return Ok(());
        }
        let Some(text) = route
            .request
            .body
            .as_ref()
            .and_then(|b| b.schema.json_schema.as_ref())
        else {
            return Ok(());
        };
        let schema: Value = serde_json::from_str(text).map_err(|_| invalid())?;
        let Some(object) = input.as_object_mut() else {
            return Ok(());
        };
        // Stage candidates first: failure never leaves a partially normalized input.
        let mut normalized = Vec::new();
        for name in FIELDS {
            if schema["properties"].get(name).is_none() {
                continue;
            }
            let Some(value) = object.get(name) else {
                continue;
            };
            let millis = date_epoch_millis(Some(value)).map_err(|error| match error {
                DateCoercionError::Validation(_) => invalid(),
                DateCoercionError::UnsupportedStringFormat => unsupported(),
            })?;
            if !(MIN..=MAX).contains(&millis) {
                return Err(unsupported());
            }
            let date = DateTime::<Utc>::from_timestamp_millis(millis).ok_or_else(unsupported)?;
            normalized.push((
                name,
                Value::String(date.to_rfc3339_opts(SecondsFormat::Millis, true)),
            ));
        }
        for (name, value) in normalized {
            object.insert(name.into(), value);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::contract::BodyContract;
    fn route() -> RouteContract {
        let mut body = BodyContract::json("Dates");
        body.schema.json_schema=Some(json!({"type":"object","properties":{"startDate":{"type":"string","format":"date-time"},"endDate":{"type":"string"},"providerFields":{"type":"object"}},"required":["startDate"]}).to_string());
        RouteContract::post("Dates", "Dates", "/dates").with_request_body(body)
    }
    #[test]
    fn normalization_is_atomic_and_never_touches_opaque_fields() {
        let mut route = route();
        let profile = BillingRequestDateProfile::UtcIsoAndEpoch;
        profile.project(&mut route).unwrap();
        let original = json!({"startDate":1.9,"endDate":null,"providerFields":{"startDate":4}});
        let mut value = original.clone();
        assert!(profile.normalize(&route, &mut value).is_err());
        assert_eq!(value, original);
        value.as_object_mut().unwrap().remove("endDate");
        profile.normalize(&route, &mut value).unwrap();
        assert_eq!(value["startDate"], "1970-01-01T00:00:00.001Z");
        assert_eq!(value["providerFields"], original["providerFields"]);
        assert!(!value.as_object().unwrap().contains_key("endDate"));
    }
    #[test]
    fn profile_is_explicit_and_preserves_legacy_metadata() {
        assert_eq!(
            "legacy".parse::<BillingRequestDateProfile>().unwrap(),
            BillingRequestDateProfile::Legacy
        );
        assert_eq!(
            "utc-iso-and-epoch"
                .parse::<BillingRequestDateProfile>()
                .unwrap(),
            BillingRequestDateProfile::UtcIsoAndEpoch
        );
        assert!("automatic".parse::<BillingRequestDateProfile>().is_err());
        let mut route = route();
        let before = route.clone();
        BillingRequestDateProfile::Legacy
            .project(&mut route)
            .unwrap();
        assert_eq!(route, before);
        let mut value = json!({"startDate":1.9});
        BillingRequestDateProfile::Legacy
            .normalize(&route, &mut value)
            .unwrap();
        assert_eq!(value["startDate"], 1.9);
    }
}
