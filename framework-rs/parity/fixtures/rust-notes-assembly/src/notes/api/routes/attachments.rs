use crate::api::context::NotesPolicy;
use crate::api::controllers::{download_attachment, list_attachments, upload_attachment};
use crate::registrations::AppState;
use axum::Router;
use fieldnotes_contracts::attachments;
use framework::native::ContractRouter;
pub(crate) fn attachment_routes() -> Router<AppState> {
    attachments::Bindings {
        list_attachments: attachments::ListAttachments::bind_authorized(
            NotesPolicy,
            list_attachments,
        ),
        download_attachment: attachments::DownloadAttachment::bind_authorized(
            NotesPolicy,
            download_attachment,
        ),
        upload_attachment: attachments::UploadAttachment::bind_authorized(
            NotesPolicy,
            upload_attachment,
        ),
    }
    .mount(ContractRouter::new(attachments::registry()))
    .expect("valid attachments binding set")
    .finish()
    .expect("complete attachments binding set")
}
