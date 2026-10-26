//! Fieldnotes application entry points and public composition API.
mod api;
pub mod application;
pub mod bootstrapper;
mod config;
pub mod registrations;
pub mod server;

pub use bootstrapper as bootstrap;
pub use registrations::AppState;
pub use application::router;

pub mod domain;
pub mod persistence;
pub use domain::types::*;
pub use fieldnotes_contracts as sdk;
pub use persistence::{
    AttachmentDownload, AttachmentRecord, NotesRepository, initialize_attachment_bucket,
};

pub mod migrations;
