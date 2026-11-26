{{#modules}}pub mod {{module}};
{{/modules}}
use forklaunch_axum::ForkLaunchRouter;
use forklaunch_core::contract::RouteContract;
use std::iter;

pub fn contracts() -> Vec<RouteContract> {
    iter::empty()
{{#modules}}        .chain({{module}}::contracts())
{{/modules}}        .collect()
}

pub fn register_router(router: ForkLaunchRouter) -> ForkLaunchRouter {
{{#modules}}    let router = {{module}}::register(router);
{{/modules}}    router
}
