//! CAC domain types and the code-set provider come from the interface and base
//! crates. Application-specific domain code belongs beside these re-exports.
pub mod schemas {
    pub use forklaunch_cac_interface::{CodeSetDescriptorDto, CodeSetType, ProcedureCodeDto, ProcedureCodeLookupDto};
}

pub mod services {
    pub use forklaunch_cac_base::MockProcedureCodeProvider;
    pub use forklaunch_cac_interface::CodeSetProvider;
}
