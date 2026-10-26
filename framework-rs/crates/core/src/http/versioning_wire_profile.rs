//! First-version wire profiles share framing, decoding and trusted parser hooks.
use super::{BodyParserHooks, JsonRequestParsers, ParsedJsonBody, charset, wire};
use crate::http::{ContractValidationError, FrameworkRequest};
use forklaunch_core::contract::{BodyKind, BodyParserOptions, RouteContract};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub(super) enum VersionWireParser {
    Json(JsonRequestParsers),
    Text {
        media: String,
        options: Arc<BodyParserOptions>,
        hooks: BodyParserHooks,
    },
}
impl VersionWireParser {
    pub(super) fn text(route: &RouteContract, hooks: BodyParserHooks) -> Result<Self, String> {
        let body = route
            .request
            .body
            .as_ref()
            .ok_or("Text parser requires a body")?;
        if body.kind != BodyKind::Text || hooks.has_json_reviver() {
            return Err("Text parser requires text body without JSON reviver".into());
        }
        let media = body.content_type.as_deref().unwrap_or("text/plain");
        if media.parse::<mime::Mime>().is_err() || media.contains(['*', ';']) || !media.is_ascii() {
            return Err("Text parser requires an exact media type without parameters".into());
        }
        super::validate_parser_options(&route.options.body_parser)?;
        Ok(Self::Text {
            media: media.into(),
            options: Arc::new(route.options.body_parser.clone()),
            hooks,
        })
    }
    pub(super) fn parse(
        &self,
        route: &RouteContract,
        request: &FrameworkRequest,
    ) -> Result<Option<ParsedJsonBody>, ContractValidationError> {
        let (media, options, hooks) = match self {
            Self::Json(parser) => return parser.parse(route, request),
            Self::Text {
                media,
                options,
                hooks,
            } => (media, options, hooks),
        };
        if !wire::has_body_and_media(request, media) {
            return Ok(Some(ParsedJsonBody::from_wire(None, None)));
        }
        let charset = charset::charset(
            crate::http::header_value(&request.headers, "content-type"),
            &options.text_default_charset,
        );
        charset::decode(&[], &charset)?;
        let decoded = wire::decode(
            wire::request_bytes(request),
            crate::http::header_value(&request.headers, "content-encoding"),
            &options.text_wire,
        )?;
        hooks.verify(request, &decoded, Some(&charset))?;
        let value = Value::String(charset::decode(&decoded, &charset)?.into_owned());
        Ok(Some(ParsedJsonBody::from_wire(
            Some(Arc::new(value)),
            Some(Arc::new(decoded.into_owned())),
        )))
    }
}
