//! Pinned Busboy text-field decoding over Rust-representable string values.
//! Unknown decoder names return undefined (None), matching the installed parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusboyTextError {
    /// JavaScript may retain isolated UTF-16 surrogates; Rust strings cannot.
    UnpairedSurrogate,
}
fn decode_busboy_text(bytes: &[u8], charset: &str) -> Result<Option<String>, BusboyTextError> {
    if bytes.is_empty() {
        return Ok(Some(String::new()));
    }
    let output = match charset.to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => String::from_utf8_lossy(bytes).into_owned(),
        "latin1" | "ascii" | "us-ascii" | "iso-8859-1" | "iso8859-1" | "iso88591"
        | "iso_8859-1" | "windows-1252" | "iso_8859-1:1987" | "cp1252" | "x-cp1252" => {
            bytes.iter().map(|byte| char::from(*byte)).collect()
        }
        "utf16le" | "utf-16le" | "ucs2" | "ucs-2" => char::decode_utf16(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]])),
        )
        .collect::<Result<String, _>>()
        .map_err(|_| BusboyTextError::UnpairedSurrogate)?,
        "base64" => base64(bytes),
        _ => return Ok(None),
    };
    Ok(Some(output))
}
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        result.push(char::from(ALPHABET[usize::from(first >> 2)]));
        result.push(char::from(
            ALPHABET[usize::from((first & 3) << 4 | second >> 4)],
        ));
        result.push(if chunk.len() > 1 {
            char::from(ALPHABET[usize::from((second & 15) << 2 | third >> 6)])
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            char::from(ALPHABET[usize::from(third & 63)])
        } else {
            '='
        });
    }
    result
}

/// Opt-in Busboy 1.6 text field decoding. Install as an Axum Extension for typed
/// extraction, and register on `VersionBodyHooksRegistry` for versioned routes.
/// The default charset is UTF-8. Explicit part charset takes precedence.
/// Unknown decoder results are represented by absent JSON fields; own undefined
/// property identity and unpaired UTF-16 surrogates are outside this JSON profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusboyTextOptions {
    default_charset: String,
}
impl Default for BusboyTextOptions {
    fn default() -> Self {
        Self {
            default_charset: "utf-8".into(),
        }
    }
}
impl BusboyTextOptions {
    #[must_use]
    pub fn with_default_charset(mut self, charset: impl Into<String>) -> Self {
        self.default_charset = charset.into();
        self
    }
    /// Decode after the caller enforces its existing multipart byte limits.
    /// None represents Busboy's undefined result for unsupported decoder names.
    /// # Errors
    /// Returns an explicit representation error for unpaired UTF-16 surrogates.
    pub fn decode(
        &self,
        bytes: &[u8],
        charset: Option<&str>,
    ) -> Result<Option<String>, BusboyTextError> {
        decode_busboy_text(bytes, charset.unwrap_or(&self.default_charset))
    }
}

/// Extract an explicitly declared MIME charset without decoding the field body.
#[must_use]
pub fn multipart_text_charset(content_type: &str) -> Option<String> {
    let mime: mime::Mime = content_type.parse().ok()?;
    mime.get_param("charset")
        .map(|value| value.as_str().to_owned())
}
