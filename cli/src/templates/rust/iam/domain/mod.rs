//! IAM domain types and services come from the interface and base crates.
//! Application-specific domain code belongs beside these re-exports.
pub mod schemas {
    pub use forklaunch_iam_interface::services::{
        CreateOrganizationDto, CreatePermissionDto, CreateRoleDto, CreateUserDto, OrganizationDto, PermissionDto,
        RoleDto, UpdateOrganizationDto, UpdatePermissionDto, UpdateRoleDto, UpdateUserDto, UserDto,
    };
    pub use forklaunch_iam_interface::{Credentials, Organization, Permission, Role, Session, Token, User};
}

pub mod services {
    pub use forklaunch_iam_base::{directory::service::BaseIamDirectory, postgres::PostgresIamService};
    pub use forklaunch_iam_interface::services::{OrganizationService, PermissionService, RoleService, UserService};
}
