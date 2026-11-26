use crate::api::context::NotesPolicy;
use crate::api::controllers::{
    archive_note, create_note, export_note_html, export_note_text, get_note, import_note_form,
    list_notes, update_note,
};
use crate::registrations::AppState;
use axum::Router;
use fieldnotes_contracts::notes;
use framework::native::ContractRouter;
pub(crate) fn notes_routes() -> Router<AppState> {
    notes::Bindings {
        list_notes: notes::ListNotes::bind_authorized(NotesPolicy, list_notes),
        get_note: notes::GetNote::bind_authorized(NotesPolicy, get_note),
        create_note: notes::CreateNote::bind_authorized(NotesPolicy, create_note),
        update_note: notes::UpdateNote::bind_authorized(NotesPolicy, update_note),
        archive_note: notes::ArchiveNote::bind_authorized(NotesPolicy, archive_note),
        export_note_text: notes::ExportNoteText::bind_authorized(NotesPolicy, export_note_text),
        export_note_html: notes::ExportNoteHtml::bind_authorized(NotesPolicy, export_note_html),
        import_note_form: notes::ImportNoteForm::bind_authorized(NotesPolicy, import_note_form),
    }
    .mount(ContractRouter::new(notes::registry()))
    .expect("valid notes binding set")
    .finish()
    .expect("complete notes binding set")
}
