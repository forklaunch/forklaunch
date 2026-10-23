use anyhow::{Result, bail};

/// Normalize a declared MIME type, ignoring case and parameters.
///
/// # Errors
/// Rejects invalid MIME tokens.
pub fn normalized_media_type(value: &str) -> Result<String> {
    let value = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !value
        .split_once('/')
        .is_some_and(|(left, right)| !left.is_empty() && !right.is_empty() && !right.contains('/'))
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/!#$&^_.+-".contains(&byte))
    {
        bail!("Invalid SDK body media type");
    }
    Ok(value)
}

/// Preserve request MIME parameters while rejecting invalid header values.
///
/// # Errors
/// Rejects invalid MIME types and control characters in header values.
pub fn request_media_type(value: &str) -> Result<String> {
    normalized_media_type(value)?;
    if value.chars().any(|ch| ch.is_control() && ch != '\t') {
        bail!("Invalid SDK request media type header value");
    }
    Ok(value.trim().to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_parameters_survive_but_header_injection_is_rejected() {
        let mime = " Application/Vnd.Example+JSON; profile=\"https://example.test/v1\" ";
        assert_eq!(
            normalized_media_type(mime).unwrap(),
            "application/vnd.example+json"
        );
        assert_eq!(request_media_type(mime).unwrap(), mime.trim());
        for invalid in [
            "application/json\r\nX-Injected: yes",
            "text/plain; x=\0",
            "text/plain; x=\u{7f}",
            "invalid",
        ] {
            assert!(request_media_type(invalid).is_err(), "{invalid:?}");
        }
    }
}
