#![forbid(unsafe_code)]
//! Shared native SDK transport and checked response runtime.
//! Generated service clients retain DTOs and route/status bindings.
mod error;
mod request;
mod request_objects;
mod response;
mod stream;
mod transport;
mod versions;
pub use error::SdkError;
pub use forklaunch_core::contract::datetime;
pub use forklaunch_core::http::{encode_urlencoded, sse};
pub use request::{NoMultipart, SdkRequest, path_with_params, request_fields};
pub use reqwest::multipart::{Form as MultipartForm, Part as MultipartPart};
pub use response::{
    MultipartResponse, MultipartResponsePart, ResponseValidator, SdkResponseWithHeaders,
    decode_json_response, decode_multipart_response, decode_response_headers, decode_text_response,
    response_content_type,
};
pub use stream::SdkEventStream;
pub use transport::{HmacSigner, MultipartPayload, SdkAuth, UniversalSdk};
pub use versions::MatchedVersionResponses;
