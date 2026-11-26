use std::fmt::Write;

/// Public validator error, matching TypeScript's string-segment `ParseError` DTO.
/// Messages are public-facing; callers must redact private provider details.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParseError {
    pub path: Vec<String>,
    pub message: String,
}

/// Formats errors exactly as TypeScript `prettyPrintParseErrors` for UTF-8 strings.
/// Missing and empty error lists produce the same summary. An empty prefix is
/// distinct from no prefix. Paths and messages retain whitespace and punctuation.
/// This formats supplied errors; it does not select a schema validator or alter
/// the native HTTP error envelope.
#[must_use]
pub fn pretty_print_parse_errors(errors: Option<&[ParseError]>, prefix: Option<&str>) -> String {
    let mut output = prefix.map_or_else(
        || "Validation failed".to_owned(),
        |prefix| format!("{prefix} validation failed"),
    );
    let errors = errors.unwrap_or_default();
    if errors.is_empty() {
        return output;
    }
    output.push_str(" with the following errors:\n");
    for (index, error) in errors.iter().enumerate() {
        if index > 0 {
            output.push_str("\n\n");
        }
        let path = if error.path.is_empty() {
            "root".to_owned()
        } else {
            error.path.join(" > ")
        };
        // Formatting into a String is infallible.
        write!(
            output,
            "{}. Path: {path}\n   Message: {}",
            index + 1,
            error.message
        )
        .expect("formatting into String");
    }
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractValidationLocation {
    PathParameter,
    Query,
    Header,
    Body,
    BodyParser,
    BodySize,
    BodyEncoding,
    BodyVerification,
    BodyParserConfiguration,
    /// Operational schema evaluator failure; fatal in every validation mode.
    SchemaExecution,
    /// Explicitly public, preformatted diagnostics from a registered version parser.
    VersionRequestDiagnostic,
    ResponseStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractValidationError {
    pub location: ContractValidationLocation,
    pub field: Option<String>,
    pub message: String,
}

impl ContractValidationError {
    /// Parser and operational evaluator failures remain fatal in every mode.
    #[must_use]
    pub const fn is_parser_error(&self) -> bool {
        matches!(
            self.location,
            ContractValidationLocation::BodyParser
                | ContractValidationLocation::BodySize
                | ContractValidationLocation::BodyEncoding
                | ContractValidationLocation::BodyVerification
                | ContractValidationLocation::BodyParserConfiguration
                | ContractValidationLocation::SchemaExecution
        )
    }

    #[must_use]
    pub fn new(location: ContractValidationLocation, message: impl Into<String>) -> Self {
        Self {
            location,
            field: None,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }
}
