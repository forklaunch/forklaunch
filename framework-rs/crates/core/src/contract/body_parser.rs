//! Serializable parser choices shared by all versions of a route.
use forklaunch_common::json_string;

// Independent switches mirror body-parser options, rather than mutually exclusive states.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "json-schema",
    serde(default, rename_all = "camelCase", deny_unknown_fields)
)]
pub struct BodyParserOptions {
    pub json_wire: BodyParserWireOptions,
    pub text_wire: BodyParserWireOptions,
    pub form_wire: BodyParserWireOptions,
    pub raw_wire: BodyParserWireOptions,
    pub json_strict: bool,
    pub json_default_charset: String,
    pub text_default_charset: String,
    pub form_default_charset: String,
    pub form_extended: bool,
    pub form_charset_sentinel: bool,
    pub form_interpret_numeric_entities: bool,
    /// Bounded by the runtime to 32 bracket levels.
    pub form_depth: usize,
    /// Bounded by the runtime to 100,000 parameters.
    pub form_parameter_limit: usize,
}
impl Default for BodyParserOptions {
    fn default() -> Self {
        Self {
            json_wire: BodyParserWireOptions::default(),
            text_wire: BodyParserWireOptions::default(),
            form_wire: BodyParserWireOptions::default(),
            raw_wire: BodyParserWireOptions::default(),
            json_strict: true,
            json_default_charset: "utf-8".into(),
            text_default_charset: "utf-8".into(),
            form_default_charset: "utf-8".into(),
            form_extended: true,
            form_charset_sentinel: false,
            form_interpret_numeric_entities: false,
            form_depth: 32,
            form_parameter_limit: 1000,
        }
    }
}
impl BodyParserOptions {
    /// Validate the explicit parser subset reproducible by both generated SDKs.
    /// Compressed wire and non-UTF-8 media charsets remain rejected by SDK transport.
    /// # Errors
    /// Rejects unsupported default charsets and limits exceeding bounded SDK admission.
    pub fn validate_portable(&self) -> Result<(), String> {
        if [
            &self.json_default_charset,
            &self.text_default_charset,
            &self.form_default_charset,
        ]
        .into_iter()
        .any(|charset| charset != "utf-8")
        {
            return Err("Portable version parser profiles currently require UTF-8 defaults".into());
        }
        if [
            &self.json_wire,
            &self.text_wire,
            &self.form_wire,
            &self.raw_wire,
        ]
        .into_iter()
        .any(|wire| wire.limit_bytes > 2 * 1024 * 1024)
        {
            return Err("Portable version parser byte limits cannot exceed 2 MiB".into());
        }
        if self.form_depth > 32
            || self.form_parameter_limit == 0
            || self.form_parameter_limit > 100_000
        {
            return Err("Portable form limits require depth <=32 and parameters 1..=100000".into());
        }
        Ok(())
    }
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{{\"jsonStrict\":{},\"jsonDefaultCharset\":{},\"textDefaultCharset\":{},\"formDefaultCharset\":{},\"formExtended\":{},\"formCharsetSentinel\":{},\"formInterpretNumericEntities\":{},\"formDepth\":{},\"formParameterLimit\":{},\"jsonWire\":{},\"textWire\":{},\"formWire\":{},\"rawWire\":{}}}",
            self.json_strict,
            json_string(&self.json_default_charset),
            json_string(&self.text_default_charset),
            json_string(&self.form_default_charset),
            self.form_extended,
            self.form_charset_sentinel,
            self.form_interpret_numeric_entities,
            self.form_depth,
            self.form_parameter_limit,
            self.json_wire.to_json(),
            self.text_wire.to_json(),
            self.form_wire.to_json(),
            self.raw_wire.to_json()
        )
    }
}

/// Byte limits apply after decompression; transport limits still bound wire bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "json-schema",
    serde(default, rename_all = "camelCase", deny_unknown_fields)
)]
pub struct BodyParserWireOptions {
    pub limit_bytes: usize,
    pub inflate: bool,
}
impl Default for BodyParserWireOptions {
    fn default() -> Self {
        Self {
            limit_bytes: 100 * 1024,
            inflate: true,
        }
    }
}
impl BodyParserWireOptions {
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{{\"limitBytes\":{},\"inflate\":{}}}",
            self.limit_bytes, self.inflate
        )
    }
}

#[cfg(all(test, feature = "json-schema"))]
mod portable_tests {
    use super::*;

    #[test]
    fn partial_settings_preserve_defaults_and_reject_unknown_configuration() {
        let options: BodyParserOptions = serde_json::from_str(
            r#"{"jsonStrict":false,"formDepth":2,"jsonWire":{"limitBytes":10}}"#,
        )
        .unwrap();
        assert!(!options.json_strict);
        assert_eq!(options.form_depth, 2);
        assert_eq!(options.json_wire.limit_bytes, 10);
        assert!(options.json_wire.inflate);
        assert_eq!(options.text_wire, BodyParserWireOptions::default());
        assert!(options.validate_portable().is_ok());
        for wire in [
            r#"{"jsonStrict":"false"}"#,
            r#"{"reviver":{}}"#,
            r#"{"jsonWire":{"verify":true}}"#,
            r#"{"formDepth":-1}"#,
        ] {
            assert!(serde_json::from_str::<BodyParserOptions>(wire).is_err());
        }
        for wire in [
            r#"{"jsonDefaultCharset":"utf-16"}"#,
            r#"{"rawWire":{"limitBytes":2097153}}"#,
            r#"{"formParameterLimit":0}"#,
            r#"{"formDepth":33}"#,
        ] {
            assert!(
                serde_json::from_str::<BodyParserOptions>(wire)
                    .unwrap()
                    .validate_portable()
                    .is_err()
            );
        }
    }
}
