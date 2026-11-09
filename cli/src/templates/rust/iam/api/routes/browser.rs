//! Cookie-session and OIDC browser routes, when `SESSION_COOKIE_ORIGIN` enables them.
//! They are served under `/auth/browser` without SDK or MCP exposure.
use crate::registrations::AppState;
use forklaunch_axum::ForkLaunchRouter;

pub fn mount(router: ForkLaunchRouter, state: &AppState) -> ForkLaunchRouter {
    match &state.browser {
        Some(browser) => browser.clone().mount(router),
        None => router,
    }
}
