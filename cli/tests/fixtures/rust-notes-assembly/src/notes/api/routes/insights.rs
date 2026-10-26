use crate::api::context::{InternalPolicy, NotesPolicy};
use crate::api::controllers::{apply_insight, request_insights};
use crate::registrations::AppState;
use axum::Router;
use fieldnotes_contracts::insights;
use framework::native::ContractRouter;
pub(crate) fn insight_routes() -> Router<AppState> {
    insights::Bindings {
        request_insights: insights::RequestInsights::bind_authorized(NotesPolicy, request_insights),
        apply_insight: insights::ApplyInsight::bind_authorized(InternalPolicy, apply_insight),
    }
    .mount(ContractRouter::new(insights::registry()))
    .expect("valid insights binding set")
    .finish()
    .expect("complete insights binding set")
}
