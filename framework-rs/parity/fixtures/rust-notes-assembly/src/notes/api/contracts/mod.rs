pub mod attachments;
pub mod insights;
pub mod notes;
mod realtime;
mod registry;
mod routes;
pub use realtime::{WatchInput, WatchRequest, presence_contract, presence_endpoint, watch_notes};
pub use realtime::{sockets, streams};
pub use registry::registry;
pub use routes::{
    ATTACHMENT_CONTENT, ATTACHMENTS, EVENTS, INSIGHTS, INTERNAL_INSIGHT, NOTE, NOTE_FORM,
    NOTE_HTML, NOTE_TEXT, NOTES, PRESENCE,
};
