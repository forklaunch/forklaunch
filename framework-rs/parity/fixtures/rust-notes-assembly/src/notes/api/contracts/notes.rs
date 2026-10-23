//! Static typed operation declarations shared by host builds and runtime bindings.
#[framework_core::contract::api]
mod api {
    use super::super::routes::{NOTE, NOTES, route};
    use crate::schemas::{CreateNote as CreateNoteInput, Note, PatchNote};
    use framework_core::contract::{
        BodyContract, HttpMethod,
        endpoint::{Endpoint, JsonBody, NoBody},
    };
    #[access(Authenticated)]
    pub fn list_notes() -> Endpoint<NoBody, JsonBody<Vec<Note>>> {
        Endpoint::from_contract(
            route("List Notes", HttpMethod::Get, NOTES)
                .with_response(200, Some(BodyContract::json_type::<Vec<Note>>())),
        )
        .expect("valid list notes contract")
    }
    #[access(Authenticated)]
    pub fn get_note() -> Endpoint<NoBody, JsonBody<Note>> {
        Endpoint::from_contract(
            route("Get Note", HttpMethod::Get, NOTE)
                .with_response(200, Some(BodyContract::json_type::<Note>())),
        )
        .expect("valid get note contract")
    }
    #[access(Authenticated)]
    pub fn create_note() -> Endpoint<JsonBody<CreateNoteInput>, JsonBody<Note>> {
        Endpoint::from_contract(
            route("Create Note", HttpMethod::Post, NOTES)
                .with_request_body(BodyContract::json_type::<CreateNoteInput>())
                .with_response(201, Some(BodyContract::json_type::<Note>())),
        )
        .expect("valid create note contract")
    }
    #[access(Authenticated)]
    pub fn update_note() -> Endpoint<JsonBody<PatchNote>, JsonBody<Note>> {
        Endpoint::from_contract(
            route("Update Note", HttpMethod::Patch, NOTE)
                .with_request_body(BodyContract::json_type::<PatchNote>())
                .with_response(200, Some(BodyContract::json_type::<Note>())),
        )
        .expect("valid update note contract")
    }
    #[access(Authenticated)]
    pub fn archive_note() -> Endpoint<NoBody, NoBody> {
        Endpoint::from_contract(
            route("Archive Note", HttpMethod::Delete, NOTE).with_response(204, None),
        )
        .expect("valid archive note contract")
    }

    #[access(Authenticated)]
    pub fn export_note_text() -> Endpoint<NoBody, framework_core::contract::endpoint::TextBody> {
        Endpoint::from_contract(
            route(
                "Export Note Text",
                HttpMethod::Get,
                super::super::routes::NOTE_TEXT,
            )
            .with_response(
                200,
                Some(BodyContract::typed::<String>(
                    framework_core::contract::ContentType::Text,
                )),
            ),
        )
        .expect("valid text export")
    }
    #[access(Authenticated)]
    pub fn export_note_html() -> Endpoint<NoBody, framework_core::contract::endpoint::HtmlBody> {
        Endpoint::from_contract(
            route(
                "Export Note Html",
                HttpMethod::Get,
                super::super::routes::NOTE_HTML,
            )
            .with_response(
                200,
                Some(BodyContract::typed::<String>(
                    framework_core::contract::ContentType::Html,
                )),
            ),
        )
        .expect("valid html export")
        .with_response_header(
            "content-security-policy",
            "default-src 'none'; sandbox; frame-ancestors 'none'",
        )
        .expect("valid html policy")
    }
    #[access(Authenticated)]
    pub fn import_note_form()
    -> Endpoint<framework_core::contract::endpoint::FormBody<CreateNoteInput>, JsonBody<Note>> {
        Endpoint::from_contract(
            route(
                "Import Note Form",
                HttpMethod::Post,
                super::super::routes::NOTE_FORM,
            )
            .with_request_body(BodyContract::typed::<CreateNoteInput>(
                framework_core::contract::ContentType::Form,
            ))
            .with_response(201, Some(BodyContract::json_type::<Note>())),
        )
        .expect("valid form import")
    }
}
pub use api::*;
