use crate::{
    api::{
        context::NotesPolicy,
        controllers::{presence, watch_notes},
    },
    registrations::AppState,
};
use axum::Router;
use framework::native::{ContractRouter, Shutdown, StreamOptions};
use std::time::Duration;

pub(crate) fn realtime_routes(shutdown: Shutdown) -> Router<AppState> {
    let mut registry = fieldnotes_contracts::streams::registry();
    fieldnotes_contracts::sockets::register_into(&mut registry).expect("static socket contracts");
    let options = StreamOptions::new(
        shutdown,
        16 * 1024,
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .expect("bounded streams");
    let router = fieldnotes_contracts::streams::Bindings {
        watch_notes: fieldnotes_contracts::streams::WatchNotes::bind_sse_authorized(
            NotesPolicy,
            options.clone(),
            watch_notes,
        ),
    }
    .mount(ContractRouter::new(registry))
    .expect("watch notes binding");
    fieldnotes_contracts::sockets::Bindings {
        presence: fieldnotes_contracts::sockets::Presence::bind_authorized(
            NotesPolicy,
            options,
            presence,
        ),
    }
    .mount(router)
    .expect("presence binding")
    .finish()
    .expect("all realtime contracts bound")
}
