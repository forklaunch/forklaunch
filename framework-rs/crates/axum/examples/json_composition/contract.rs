use forklaunch_core::contract::{
    AccessLevel, BodyContract, RouteContract, SchemaRef, ValidationMode,
};
pub fn route(name: &str, mode: ValidationMode, profile: &str) -> RouteContract {
    let mut body = BodyContract::json("Input");
    body.schema = SchemaRef::named("Input").with_json_schema("{}");
    let mut output = BodyContract::json("Output");
    output.schema = SchemaRef::named("Output").with_json_schema("{}");
    let mut route = RouteContract::post(
        format!("Transform{name}{profile}"),
        "Transform",
        format!("/transform/{name}/{profile}"),
    )
    .with_request_body(body)
    .with_response(200, Some(output));
    route.options.request_validation = mode;
    route.options.response_validation = ValidationMode::None;
    route.options.body_parser.json_strict = false;
    if profile == "protected" {
        route.access = AccessLevel::Authenticated;
    }
    route
}
