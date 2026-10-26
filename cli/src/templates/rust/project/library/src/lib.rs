pub mod error;
pub mod models;
pub mod service;

pub use error::{LibraryError, Result};
pub use models::LibraryItem;
pub use service::LibraryService;

pub const LIBRARY_NAME: &str = "{{{name}}}";

#[must_use]
pub fn library_name() -> &'static str {
    LIBRARY_NAME
}
