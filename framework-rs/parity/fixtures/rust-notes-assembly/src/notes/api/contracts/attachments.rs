//! Static typed operation declarations shared by host builds and runtime bindings.
#[framework_core::contract::api]
mod api {
    use super::super::routes::route;
    use framework_core::contract::{
        BodyContract, HttpMethod,
        endpoint::{Endpoint, JsonBody, NoBody},
    };
    #[access(Authenticated)]
    pub fn list_attachments() -> Endpoint<NoBody, JsonBody<Vec<crate::schemas::Attachment>>> {
        Endpoint::from_contract(
            route(
                "List Attachments",
                HttpMethod::Get,
                super::super::routes::ATTACHMENTS,
            )
            .with_response(
                200,
                Some(BodyContract::json_type::<Vec<crate::schemas::Attachment>>()),
            ),
        )
        .expect("valid attachment list")
    }
    #[access(Authenticated)]
    pub fn download_attachment() -> Endpoint<NoBody, framework_core::contract::endpoint::BinaryBody>
    {
        Endpoint::from_contract(
            route(
                "Download Attachment",
                HttpMethod::Get,
                super::super::routes::ATTACHMENT_CONTENT,
            )
            .with_response(
                200,
                Some(BodyContract::typed::<String>(
                    framework_core::contract::ContentType::Binary,
                )),
            ),
        )
        .expect("valid attachment download")
        .with_response_header("content-disposition", "attachment; filename=\"attachment\"")
        .expect("valid attachment disposition")
    }

    #[access(Authenticated)]
    pub fn upload_attachment() -> Endpoint<
        framework_core::contract::endpoint::MultipartBody<crate::schemas::AttachmentUpload>,
        JsonBody<crate::schemas::Attachment>,
    > {
        Endpoint::from_contract(
            route(
                "Upload Attachment",
                HttpMethod::Post,
                super::super::routes::ATTACHMENTS,
            )
            .with_request_body(BodyContract::typed::<crate::schemas::AttachmentUpload>(
                framework_core::contract::ContentType::Multipart,
            ))
            .with_response(
                201,
                Some(BodyContract::json_type::<crate::schemas::Attachment>()),
            ),
        )
        .expect("valid attachment upload")
    }
}
pub use api::*;
