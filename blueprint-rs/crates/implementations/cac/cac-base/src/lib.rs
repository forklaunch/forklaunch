//! The same free, non-AMA placeholder procedure codes as TypeScript CAC.
//! This provider is deliberately unlicensed and never supplies clinical CPT data.
//!
//! Lookup is an exact, case-sensitive match over three codes (`PROC-001`
//! Office Visit, `PROC-002` Annual Physical Exam, `PROC-003` Diagnostic Lab
//! Panel); anything else, including real CPT codes such as `99213`, is not
//! found. There is no fuzzy match or suggest-from-text operation, in either
//! runtime. Applications that need real codes or suggestions implement
//! [`CodeSetProvider`] over a licensed code set.
#![forbid(unsafe_code)]

#[cfg(feature = "http")]
pub mod http;
use forklaunch_cac_interface::{
    CodeSetDescriptorDto, CodeSetProvider, CodeSetType, ProcedureCodeDto, ProcedureCodeLookupDto,
};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::telemetry::OpenTelemetryCollector;

pub struct MockProcedureCodeProvider {
    collector: OpenTelemetryCollector,
}

impl MockProcedureCodeProvider {
    #[must_use]
    pub fn new(collector: OpenTelemetryCollector) -> Self {
        Self { collector }
    }
}

impl Default for MockProcedureCodeProvider {
    fn default() -> Self {
        Self::new(OpenTelemetryCollector::new("cac"))
    }
}

impl CodeSetProvider for MockProcedureCodeProvider {
    async fn lookup_procedure_code(
        &self,
        lookup: ProcedureCodeLookupDto,
    ) -> BlueprintResult<Option<ProcedureCodeDto>> {
        self.collector.debug("Looking up mock procedure code", "");
        let description = match lookup.code.as_str() {
            "PROC-001" => "Office Visit",
            "PROC-002" => "Annual Physical Exam",
            "PROC-003" => "Diagnostic Lab Panel",
            _ => return Ok(None),
        };
        Ok(Some(ProcedureCodeDto {
            code: lookup.code,
            description: description.to_owned(),
        }))
    }

    fn describe(&self) -> CodeSetDescriptorDto {
        CodeSetDescriptorDto {
            code_set_type: CodeSetType::Mock,
            licensed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn exact_lookup_does_not_trim_fold_case_or_invent_licensed_codes() {
        let provider = MockProcedureCodeProvider::default();
        assert_eq!(provider.describe().code_set_type, CodeSetType::Mock);
        assert!(!provider.describe().licensed);
        for code in ["", "unknown", "proc-001", " PROC-001", "PROC-001 ", "99213"] {
            assert!(
                provider
                    .lookup_procedure_code(ProcedureCodeLookupDto {
                        code: code.to_owned()
                    })
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        for (code, description) in [
            ("PROC-001", "Office Visit"),
            ("PROC-002", "Annual Physical Exam"),
            ("PROC-003", "Diagnostic Lab Panel"),
        ] {
            assert_eq!(
                provider
                    .lookup_procedure_code(ProcedureCodeLookupDto {
                        code: code.to_owned()
                    })
                    .await
                    .unwrap(),
                Some(ProcedureCodeDto {
                    code: code.to_owned(),
                    description: description.to_owned()
                })
            );
        }
    }
}
