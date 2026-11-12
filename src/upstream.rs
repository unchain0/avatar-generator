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
    InvalidBase(String),
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
            UpstreamError::InvalidBase(msg) => write!(f, "invalid upstream base url: {msg}"),
        }
    }
}

impl std::error::Error for UpstreamError {}

/// Builds the renderer URL from parsed parts. The seed is pushed as a path
/// segment, so it is percent-encoded instead of being concatenated raw.
pub fn avatar_url(
    base: &str,
    seed: &str,
    options: &AvatarOptions,
) -> Result<String, UpstreamError> {
    let mut url =
        reqwest::Url::parse(base).map_err(|e| UpstreamError::InvalidBase(e.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| UpstreamError::InvalidBase(format!("{base} cannot have a path")))?
        .pop_if_empty()
        .push(seed);
    let size = format!("{}x{}", options.size, options.size);
    url.query_pairs_mut()
        .append_pair("set", options.set.as_str())
        .append_pair("size", &size);
    if let Some(bg) = options.background.upstream_param() {
        url.query_pairs_mut().append_pair("bgset", bg);
    }
    Ok(url.to_string())
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
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => return Err(UpstreamError::Request(error.to_string())),
        };
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
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn url_includes_set_and_size() {
        let options = AvatarOptions {
            set: RoboSet::Set3,
            background: Background::Bg1,
            size: 300,
        };
        let url = avatar_url("https://robohash.org/", "abcdef", &options).unwrap();
        assert_eq!(
            url,
            "https://robohash.org/abcdef?set=set3&size=300x300&bgset=bg1"
        );
    }

    #[test]
    fn url_encodes_seed_as_a_path_segment() {
        let options = AvatarOptions {
            set: RoboSet::Set1,
            background: Background::None,
            size: 50,
        };
        let url = avatar_url("https://robohash.org", "a/../b?c d", &options).unwrap();
        assert_eq!(
            url,
            "https://robohash.org/a%2F..%2Fb%3Fc%20d?set=set1&size=50x50"
        );
    }

    #[test]
    fn url_rejects_a_base_that_is_not_a_url() {
        let options = AvatarOptions {
            set: RoboSet::Set1,
            background: Background::None,
            size: 50,
        };
        assert!(avatar_url("not a url", "abc", &options).is_err());
    }

    #[test]
    fn url_omits_bg_when_none() {
        let options = AvatarOptions {
            set: RoboSet::Set1,
            background: Background::None,
            size: 50,
        };
        let url = avatar_url("https://robohash.org", "abcdef", &options).unwrap();
        assert_eq!(url, "https://robohash.org/abcdef?set=set1&size=50x50");
    }

    #[test]
    fn validate_accepts_png_and_rejects_other_bodies() {
        assert!(validate_image(&PNG_SIGNATURE).is_ok());
        assert!(validate_image(b"<html>oops</html>").is_err());
        assert!(validate_image(b"").is_err());
    }

    #[test]
    fn url_rejects_a_base_without_path_segments() {
        let options = AvatarOptions {
            set: RoboSet::Set1,
            background: Background::None,
            size: 50,
        };
        let error =
            avatar_url("mailto:ops@example.com", "abc", &options).expect_err("invalid base");
        assert!(error.to_string().contains("invalid upstream base url"));
    }

    #[tokio::test]
    async fn fetch_avatar_reports_a_transport_failure() {
        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, "http://127.0.0.1:1/")
            .await
            .expect_err("transport failure");
        assert!(error.to_string().contains("request failed"));
    }

    #[tokio::test]
    async fn fetch_avatar_reports_a_broken_body_stream() {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ntransfer-encoding: chunked\r\n\r\n",
                )
                .await
                .unwrap();
            socket
                .write_all(b"8\r\n\x89PNG\r\n\x1a\n\r\n")
                .await
                .unwrap();
            socket.write_all(b"zz\r\n").await.unwrap();
            socket.shutdown().await.unwrap();
        });

        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, &format!("http://{address}/"))
            .await
            .expect_err("broken body stream");
        assert!(error.to_string().contains("request failed"));
    }

    #[test]
    fn validate_rejects_too_large() {
        let large = vec![0u8; MAX_AVATAR_BYTES + 1];
        assert!(validate_image(&large).is_err());
    }

    #[test]
    fn error_messages_name_the_failure() {
        assert!(UpstreamError::Request("timeout".to_string())
            .to_string()
            .contains("request failed: timeout"));
        assert!(UpstreamError::Status(502).to_string().contains("502"));
        assert!(UpstreamError::Empty.to_string().contains("empty"));
        assert!(UpstreamError::TooLarge.to_string().contains("exceeded"));
        assert!(UpstreamError::NotAnImage.to_string().contains("not a PNG"));
        assert!(UpstreamError::InvalidBase("bad".to_string())
            .to_string()
            .contains("invalid upstream base url: bad"));
    }

    async fn oversized_png() -> (
        [(axum::http::HeaderName, &'static str); 1],
        axum::body::Body,
    ) {
        use axum::http::header::CONTENT_TYPE;
        let chunk = axum::body::Bytes::from(vec![0u8; 1024 * 1024]);
        let mut chunks: Vec<Result<axum::body::Bytes, std::io::Error>> = Vec::with_capacity(6);
        for _ in 0..6 {
            chunks.push(Ok(chunk.clone()));
        }
        (
            [(CONTENT_TYPE, "image/png")],
            axum::body::Body::from_stream(futures_util::stream::iter(chunks)),
        )
    }

    #[tokio::test]
    async fn fetch_avatar_rejects_a_chunked_body_over_the_limit() {
        use axum::{routing::get, Router};

        let app = Router::new().route("/", get(oversized_png));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, &format!("http://{address}/"))
            .await
            .expect_err("oversized chunked body");
        assert!(error.to_string().contains("exceeded"));

        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn fetch_avatar_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(PNG_SIGNATURE),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_avatar(&client, &server.uri()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PNG_SIGNATURE.to_vec());
    }

    #[tokio::test]
    async fn fetch_avatar_rejects_non_success_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, &server.uri())
            .await
            .expect_err("status error");
        assert!(error.to_string().contains("status 404"));
    }

    #[tokio::test]
    async fn fetch_avatar_rejects_non_png_content_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string("<html>oops</html>"),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, &server.uri())
            .await
            .expect_err("not an image");
        assert!(error.to_string().contains("not a PNG"));
    }

    #[tokio::test]
    async fn fetch_avatar_rejects_stream_exceeding_limit() {
        let server = MockServer::start().await;
        let mut body = Vec::with_capacity(MAX_AVATAR_BYTES + 100);
        body.extend_from_slice(&PNG_SIGNATURE);
        body.resize(MAX_AVATAR_BYTES + 100, 0);

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(body),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, &server.uri())
            .await
            .expect_err("oversized body");
        assert!(error.to_string().contains("exceeded"));
    }

    #[tokio::test]
    async fn fetch_avatar_rejects_empty_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(vec![]),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, &server.uri())
            .await
            .expect_err("empty body");
        assert!(error.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn fetch_avatar_rejects_invalid_png_signature() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_string("not a png"),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let error = fetch_avatar(&client, &server.uri())
            .await
            .expect_err("not an image");
        assert!(error.to_string().contains("not a PNG"));
    }
}
