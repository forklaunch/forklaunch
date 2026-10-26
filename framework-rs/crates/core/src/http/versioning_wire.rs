//! Bounded decoding for body-parser-compatible content encodings.
use super::{ContractValidationError, ContractValidationLocation, body_too_large, invalid_body};
use forklaunch_core::contract::BodyParserWireOptions;
use std::{borrow::Cow, io::Read};

pub(super) fn decode<'a>(
    bytes: &'a [u8],
    encoding: Option<&str>,
    options: &BodyParserWireOptions,
) -> Result<Cow<'a, [u8]>, ContractValidationError> {
    let encoding = encoding.unwrap_or("identity").to_ascii_lowercase();
    if encoding == "identity" {
        if bytes.len() > options.limit_bytes {
            return Err(body_too_large("body exceeds parser byte limit"));
        }
        return Ok(Cow::Borrowed(bytes));
    }
    let unsupported = || {
        ContractValidationError::new(
            ContractValidationLocation::BodyEncoding,
            "unsupported request content encoding",
        )
    };
    if !options.inflate {
        return Err(unsupported());
    }
    let decoder: Box<dyn Read + '_> = match encoding.as_str() {
        "gzip" => Box::new(flate2::read::MultiGzDecoder::new(bytes)),
        "deflate" => Box::new(flate2::read::ZlibDecoder::new(bytes)),
        "br" => Box::new(brotli::Decompressor::new(bytes, 4096)),
        _ => return Err(unsupported()),
    };
    let mut output = Vec::new();
    decoder
        .take(options.limit_bytes as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|_| invalid_body("invalid compressed request body"))?;
    if output.len() > options.limit_bytes {
        return Err(body_too_large("body exceeds parser byte limit"));
    }
    Ok(Cow::Owned(output))
}

/// Framing and exact media selection precede decoding and verifier callbacks.
pub(super) fn has_body_and_media(request: &crate::http::FrameworkRequest, expected: &str) -> bool {
    let framed = crate::http::header_value(&request.headers, "transfer-encoding").is_some()
        || crate::http::header_value(&request.headers, "content-length")
            .is_some_and(|value| value.parse::<u64>().is_ok());
    framed
        && crate::http::header_value(&request.headers, "content-type").is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case(expected)
        })
}
pub(super) fn request_bytes(request: &crate::http::FrameworkRequest) -> &[u8] {
    match &request.body {
        crate::http::Body::Empty => &[],
        crate::http::Body::Text(value) => value.as_bytes(),
        crate::http::Body::Binary(value) => value,
    }
}
