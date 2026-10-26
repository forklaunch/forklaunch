/// An owned multipart body. Text fields and file parts retain their insertion order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultipartForm {
    parts: Vec<(String, MultipartPart)>,
}

/// A binary file or UTF-8 text field with safely encoded MIME metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartPart {
    bytes: Vec<u8>,
    filename: Option<String>,
    content_type: Option<String>,
}

impl MultipartPart {
    #[must_use]
    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            filename: None,
            content_type: None,
        }
    }

    #[must_use]
    pub fn file_name(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn mime_str(mut self, content_type: &str) -> Result<Self, &'static str> {
        let valid = content_type.split_once('/').is_some_and(|(kind, subtype)| {
            !kind.is_empty() && !subtype.is_empty() && !subtype.contains('/')
        }) && content_type
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/!#$&^_.+-".contains(&byte));
        if !valid {
            return Err("Invalid multipart content type");
        }
        self.content_type = Some(content_type.into());
        Ok(self)
    }
}

impl MultipartForm {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn text(self, name: impl Into<String>, value: impl AsRef<str>) -> Self {
        self.part(
            name,
            MultipartPart::bytes(multipart_normalize(value.as_ref()).into_bytes()),
        )
    }

    #[must_use]
    pub fn part(mut self, name: impl Into<String>, part: MultipartPart) -> Self {
        self.parts.push((name.into(), part));
        self
    }

    fn encode(self) -> (Vec<u8>, String) {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let boundary = loop {
            let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let candidate = format!("forklaunch-sdk-{time:x}-{sequence:x}");
            if self.parts.iter().all(|(name, part)| {
                !name.contains(&candidate)
                    && !part
                        .filename
                        .as_ref()
                        .is_some_and(|name| name.contains(&candidate))
                    && !part
                        .bytes
                        .windows(candidate.len())
                        .any(|bytes| bytes == candidate.as_bytes())
            }) {
                break candidate;
            }
        };
        let mut output = Vec::new();
        for (name, part) in self.parts {
            let mut header = format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"",
                multipart_quoted(&name)
            );
            if let Some(filename) = part.filename {
                header.push_str(&format!("; filename=\"{}\"", multipart_quoted(&filename)));
                header.push_str(&format!(
                    "\r\nContent-Type: {}",
                    part.content_type
                        .as_deref()
                        .unwrap_or("application/octet-stream")
                ));
            } else if let Some(content_type) = part.content_type {
                header.push_str(&format!("\r\nContent-Type: {content_type}"));
            }
            header.push_str("\r\n\r\n");
            output.extend_from_slice(header.as_bytes());
            output.extend_from_slice(&part.bytes);
            output.extend_from_slice(b"\r\n");
        }
        output.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        (output, format!("multipart/form-data; boundary={boundary}"))
    }
}

fn multipart_normalize(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

fn multipart_quoted(value: &str) -> String {
    multipart_normalize(value)
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace('"', "%22")
}
