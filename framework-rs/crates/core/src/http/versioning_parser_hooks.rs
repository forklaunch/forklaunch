//! Native callbacks around the shared version body parser.
use crate::http::{ContractValidationError, ContractValidationLocation, FrameworkRequest};
use serde_json::Value;
#[path = "versioning_reviver.rs"]
mod json_reviver;
use std::sync::Arc;

type Verify = dyn Fn(&FrameworkRequest, &[u8], Option<&str>) -> Result<(), String> + Send + Sync;

type Reviver = dyn Fn(&str, Value) -> Result<Option<Value>, String> + Send + Sync;

/// Runtime parser callbacks, separate from serializable parser options.
#[derive(Clone, Default)]
pub struct BodyParserHooks {
    verify: Option<Arc<Verify>>,
    reviver: Option<Arc<Reviver>>,
}
impl std::fmt::Debug for BodyParserHooks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BodyParserHooks")
            .field("verify", &self.verify.is_some())
            .field("json_reviver", &self.reviver.is_some())
            .finish()
    }
}
impl BodyParserHooks {
    /// Inspect bounded, decompressed bytes before character decoding or parsing.
    /// Charset is absent for raw/file bodies. Errors reject the request with 403.
    #[must_use]
    pub fn with_verify(
        mut self,
        callback: impl Fn(&FrameworkRequest, &[u8], Option<&str>) -> Result<(), String>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.verify = Some(Arc::new(callback));
        self
    }
    /// Visit JSON children before their parent, ending with the root key "".
    /// Returning `None` removes an object property or the root. Sparse array
    /// removal is rejected because JSON values cannot represent array holes.
    #[must_use]
    pub fn with_json_reviver(
        mut self,
        callback: impl Fn(&str, Value) -> Result<Option<Value>, String> + Send + Sync + 'static,
    ) -> Self {
        self.reviver = Some(Arc::new(callback));
        self
    }
    pub(super) fn has_json_reviver(&self) -> bool {
        self.reviver.is_some()
    }

    pub(super) fn revive_json(&self, text: &str) -> Result<Option<Value>, ContractValidationError> {
        self.revive_json_profile(text, false)
    }

    pub(super) fn revive_finite_json(
        &self,
        text: &str,
    ) -> Result<Option<Value>, ContractValidationError> {
        self.revive_json_profile(text, true)
    }

    fn revive_json_profile(
        &self,
        text: &str,
        finite_numbers: bool,
    ) -> Result<Option<Value>, ContractValidationError> {
        let failure = || {
            ContractValidationError::new(
                ContractValidationLocation::BodyParser,
                "invalid JSON reviver result",
            )
        };
        let callback = self.reviver.as_ref().expect("configured reviver");
        let mut node: json_reviver::Node = serde_json::from_str(text).map_err(|_| failure())?;
        if finite_numbers {
            node.map_scalars(super::json_request::normalize_numbers);
        }
        json_reviver::revive(callback.as_ref(), "", node, 0).map_err(|_| failure())
    }

    pub(super) fn verify(
        &self,
        request: &FrameworkRequest,
        bytes: &[u8],
        charset: Option<&str>,
    ) -> Result<(), ContractValidationError> {
        if let Some(callback) = &self.verify {
            callback(request, bytes, charset).map_err(|_| {
                ContractValidationError::new(
                    ContractValidationLocation::BodyVerification,
                    "request body verification failed",
                )
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    #[test]
    fn reviver_preserves_declaration_numeric_and_duplicate_key_order() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let calls = trace.clone();
        let hooks = BodyParserHooks::default().with_json_reviver(move |key, value| {
            calls.lock().unwrap().push(key.to_owned());
            if key == "drop" {
                return Ok(None);
            }
            Ok(Some(
                value
                    .as_i64()
                    .map_or(value.clone(), |value| json!(value + 1)),
            ))
        });
        let result = hooks
            .revive_json(
                r#"{"z":1,"2":2,"1":1,"10":10,"nested":{"drop":0,"q":3},"array":[4,5],"z":9}"#,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            *trace.lock().unwrap(),
            [
                "1", "2", "10", "z", "drop", "q", "nested", "0", "1", "array", ""
            ]
        );
        assert_eq!(
            result,
            json!({"z":10,"2":3,"1":2,"10":11,"nested":{"q":4},"array":[5,6]})
        );
    }

    #[test]
    fn reviver_root_removal_and_sparse_array_boundary_are_explicit() {
        let hooks = BodyParserHooks::default().with_json_reviver(|_, _| Ok(None));
        assert_eq!(hooks.revive_json("42").unwrap(), None);
        let errors = [hooks.revive_json("[42]").unwrap_err()];
        assert_eq!(
            crate::http::validation_error_response(&errors, "test").status_code,
            400
        );
        assert!(errors[0].is_parser_error());
    }

    #[test]
    fn empty_json_bypasses_reviver_but_nonempty_root_invokes_it() {
        use crate::http::{Body, FrameworkRequest, HttpMethod, RequestContext, VersionedRoutePlan};
        use forklaunch_core::contract::{
            BodyContract, RequestContract, RouteContract, VersionedContract,
        };
        let route =
            RouteContract::post("Revive", "Revive", "/revive").with_version(VersionedContract {
                version: "one".into(),
                request: RequestContract::default().with_body(BodyContract::json("Input")),
                responses: vec![],
            });
        let hooks = BodyParserHooks::default()
            .with_json_reviver(|_, _| Err("private reviver detail".into()));
        let plan = VersionedRoutePlan::compile(&route)
            .unwrap()
            .with_parser_hooks(hooks)
            .unwrap();
        for bytes in [vec![], vec![0xef, 0xbb, 0xbf], b"{}".to_vec()] {
            let request =
                FrameworkRequest::new(HttpMethod::Post, "/revive", RequestContext::new("test"))
                    .with_header("content-type", "application/json")
                    .with_body(Body::Binary(bytes.clone()));
            assert_eq!(plan.select(&request).is_ok(), bytes != b"{}");
        }
    }
}
