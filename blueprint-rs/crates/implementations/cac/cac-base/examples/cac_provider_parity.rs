//! Emits provider results for the TypeScript/Rust conformance fixture.
use forklaunch_cac_base::MockProcedureCodeProvider;
use forklaunch_cac_interface::{CodeSetProvider, ProcedureCodeLookupDto};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = MockProcedureCodeProvider::default();
    let mut results = Vec::new();
    for code in [
        "PROC-001",
        "PROC-002",
        "PROC-003",
        "unknown",
        "proc-001",
        " PROC-001",
        "99213",
        "",
    ] {
        results.push(
            provider
                .lookup_procedure_code(ProcedureCodeLookupDto {
                    code: code.to_owned(),
                })
                .await?,
        );
    }
    println!(
        "{}",
        serde_json::json!({"descriptor":provider.describe(), "results":results})
    );
    Ok(())
}
