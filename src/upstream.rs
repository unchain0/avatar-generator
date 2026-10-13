use crate::models::AvatarOptions;
use std::time::Duration;

pub const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum UpstreamError {
    Request(String),
    Status(u16),
    Empty,
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpstreamError::Request(msg) => write!(f, "request failed: {msg}"),
            UpstreamError::Status(code) => write!(f, "upstream returned status {code}"),
            UpstreamError::Empty => write!(f, "upstream returned an empty body"),
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
        url.push_str("&bg=");
        url.push_str(bg);
    }
    url
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

    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Request(e.to_string()))?;

    if bytes.is_empty() {
        return Err(UpstreamError::Empty);
    }
    Ok(bytes.to_vec())
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
            "https://robohash.org/abcdef?set=set3&size=300x300&bg=bg1"
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
}
