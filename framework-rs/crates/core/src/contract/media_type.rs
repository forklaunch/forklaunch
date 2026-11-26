//! Media overrides preserve a codec's Rust payload representation.
use crate::contract::{BodyContract, BodyKind, ContentType};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Json,
    Text,
    Html,
    Xml,
    Css,
    JavaScript,
    Csv,
    Markdown,
    TextXml,
    Rtf,
    Yaml,
    XYaml,
    Binary,
    Pdf,
    Excel,
    Spreadsheet,
    Word,
    WordDocument,
    Zip,
    Jpeg,
    Png,
    Gif,
    Mp3,
    Wav,
    Mp4,
    Form,
    FormUrlEncoded,
    FormWwwUrlEncoded,
    FormUrlencode,
    Multipart,
    EventStream,
    /// An application media type with an explicit codec category.
    Custom {
        representation: ContentType,
        value: &'static str,
    },
}
impl MediaType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Text => "text/plain; charset=utf-8",
            Self::Html => "text/html; charset=utf-8",
            Self::Xml => "application/xml",
            Self::Css => "text/css",
            Self::JavaScript => "text/javascript",
            Self::Csv => "text/csv",
            Self::Markdown => "text/markdown",
            Self::TextXml => "text/xml",
            Self::Rtf => "text/rtf",
            Self::Yaml => "text/yaml",
            Self::XYaml => "text/x-yaml",
            Self::Binary => "application/octet-stream",
            Self::Pdf => "application/pdf",
            Self::Excel => "application/vnd.ms-excel",
            Self::Spreadsheet => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
            Self::Word => "application/msword",
            Self::WordDocument => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            Self::Zip => "application/zip",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Mp3 => "audio/mpeg",
            Self::Wav => "audio/wav",
            Self::Mp4 => "video/mp4",
            Self::Form => "application/x-www-form-urlencoded",
            Self::FormUrlEncoded => "application/x-url-encoded",
            Self::FormWwwUrlEncoded => "application/x-www-url-encoded",
            Self::FormUrlencode => "application/x-urlencode",
            Self::Multipart => "multipart/form-data",
            Self::EventStream => "text/event-stream",
            Self::Custom { value, .. } => value,
        }
    }
    #[must_use]
    pub const fn body_kind(self) -> BodyKind {
        match self {
            Self::Json => BodyKind::Json,
            Self::Text
            | Self::Html
            | Self::Xml
            | Self::Css
            | Self::JavaScript
            | Self::Csv
            | Self::Markdown
            | Self::TextXml
            | Self::Rtf
            | Self::Yaml
            | Self::XYaml => BodyKind::Text,
            Self::Binary
            | Self::Pdf
            | Self::Excel
            | Self::Spreadsheet
            | Self::Word
            | Self::WordDocument
            | Self::Zip
            | Self::Jpeg
            | Self::Png
            | Self::Gif
            | Self::Mp3
            | Self::Wav
            | Self::Mp4 => BodyKind::Binary,
            Self::Form | Self::FormUrlEncoded | Self::FormWwwUrlEncoded | Self::FormUrlencode => {
                BodyKind::UrlEncoded
            }
            Self::Multipart => BodyKind::Multipart,
            Self::EventStream => BodyKind::ServerSentEvent,
            Self::Custom { representation, .. } => representation.body_kind(),
        }
    }
}
impl BodyContract {
    /// Override MIME without changing the typed codec or schema.
    /// # Errors
    /// Rejects invalid MIME/header values, codec changes, and non-form-data multipart.
    pub fn with_media_type(mut self, media: MediaType) -> Result<Self, &'static str> {
        let value = media.as_str();
        let normalized = value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let valid = normalized
            .split_once('/')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/'))
            && normalized
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/!#$&^_.+-".contains(&byte))
            && !value.chars().any(char::is_control);
        if !valid
            || self.kind != media.body_kind()
            || (self.kind == BodyKind::Multipart && normalized != "multipart/form-data")
            || (self.kind == BodyKind::ServerSentEvent && normalized != "text/event-stream")
        {
            return Err("media override must preserve a supported codec representation");
        }
        self.content_type = Some(value.to_owned());
        Ok(self)
    }
}
