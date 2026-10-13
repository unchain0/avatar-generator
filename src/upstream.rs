use crate::models::AvatarOptions;
use futures_util::StreamExt;
use std::time::Duration;

pub const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug)]
pub enum UpstreamError {
    Request(String),
    Status(u16),
    Empty,
    TooLarge,
    NotAnImage,
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpstreamError::Request(msg) => write!(f, "request failed: {msg}"),
            UpstreamError::Status(code) => write!(f, "upstream returned status {code}"),
            UpstreamError::Empty => write!(f, "upstream returned an empty body"),
            UpstreamError::TooLarge => {
                write!(f, "upstream response exceeded {MAX_AVATAR_BYTES} bytes")
            }
            UpstreamError::NotAnImage => write!(f, "upstream body was not a PNG image"),
        }
    }
}

impl std::error::Error for UpstreamError {}

pub fn avatar_url(base: &str, seed: &str, options: &AvatarOptions) -> String {
    let base = base.trim_end_matches('/');
    let mut url = format!(
        "{base}/{seed}?set={}&size={}x{}",
        options.set.as_str(),
        options.size,
        options.size
    );
    if let Some(bg) = options.background.upstream_param() {
        url.push_str("&bgset=");
        url.push_str(bg);
    }
    url
}

fn validate_image(bytes: &[u8]) -> Result<(), UpstreamError> {
    if bytes.is_empty() {
        return Err(UpstreamError::Empty);
    }
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err(UpstreamError::TooLarge);
    }
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err(UpstreamError::NotAnImage);
    }
    Ok(())
}

pub async fn fetch_avatar(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, UpstreamError> {
    let response = client
        .get(url)
        .timeout(UPSTREAM_TIMEOUT)
        .send()
        .await
        .map_err(|e| UpstreamError::Request(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpstreamError::Status(response.status().as_u16()));
    }
    let ct = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !ct.starts_with("image/png") {
        return Err(UpstreamError::NotAnImage);
    }

    if response
        .content_length()
        .is_some_and(|len| len > MAX_AVATAR_BYTES as u64)
    {
        return Err(UpstreamError::TooLarge);
    }

    let mut bytes = Vec::with_capacity(1024);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| UpstreamError::Request(e.to_string()))?;
        if bytes.len() + chunk.len() > MAX_AVATAR_BYTES {
            return Err(UpstreamError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }

    validate_image(&bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Background, RoboSet};

    #[test]
    fn url_includes_set_and_size() {
        let options = AvatarOptions {
            set: RoboSet::Set3,
            background: Background::Bg1,
            size: 300,
        };
        let url = avatar_url("https://robohash.org/", "abcdef", &options);
        assert_eq!(
            url,
            "https://robohash.org/abcdef?set=set3&size=300x300&bgset=bg1"
        );
    }

    #[test]
    fn url_omits_bg_when_none() {
        let options = AvatarOptions {
            set: RoboSet::Set1,
            background: Background::None,
            size: 50,
        };
        let url = avatar_url("https://robohash.org", "abcdef", &options);
        assert_eq!(url, "https://robohash.org/abcdef?set=set1&size=50x50");
    }

    #[test]
    fn validate_accepts_png_and_rejects_other_bodies() {
        assert!(validate_image(&PNG_SIGNATURE).is_ok());
        assert!(validate_image(b"<html>oops</html>").is_err());
        assert!(validate_image(b"").is_err());
    }
}
