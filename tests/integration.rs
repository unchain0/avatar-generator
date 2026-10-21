use avatar_generator::{app, build_state, config::Config, store::Store};
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xDE, 0xAD, 0xBE, 0xEF,
];

fn peer() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 51234)))
}

fn config(base_url: String, data_dir: PathBuf, rate_limit: u32) -> Config {
    Config {
        port: 0,
        data_dir,
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        robohash_base: base_url,
        default_ttl_days: 1,
        cleanup_interval_secs: 600,
        max_upstream_concurrency: 10,
        rate_limit_per_minute: rate_limit,
        trust_forwarded_headers: false,
    }
}

async fn harness(server: &MockServer, rate_limit: u32) -> (Router, TempDir) {
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(PNG),
        )
        .mount(server)
        .await;

    let dir = TempDir::new().unwrap();
    let store = Arc::new(Store::new(dir.path().to_path_buf()).unwrap());
    let state = build_state(
        config(server.uri(), dir.path().to_path_buf(), rate_limit),
        store,
    );
    (app(state), dir)
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}

fn json_request(method: &str, uri: &str, body: Value, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .extension(peer())
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(uri).extension(peer());
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

fn del(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .extension(peer())
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

fn post(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .extension(peer())
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

fn session_cookie(headers: &HeaderMap) -> String {
    headers
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .expect("response sets a session cookie")
        .to_string()
}

#[tokio::test]
async fn healthz_returns_ok() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, _, body) = send(&router, get("/healthz", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"ok");
}

#[tokio::test]
async fn generate_stores_and_serves_png() {
    let server = MockServer::start().await;
    let (router, dir) = harness(&server, 100).await;

    let body = json!({
        "count": 2,
        "seed": "hello",
        "set": "set2",
        "background": "bg1",
        "size": 100,
        "ttl_days": 1,
    });
    let (status, _headers, bytes) =
        send(&router, json_request("POST", "/generate", body, None)).await;
    assert_eq!(status, StatusCode::OK);

    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["count"], 2);
    let avatars = payload["avatars"].as_array().unwrap();
    assert_eq!(avatars.len(), 2);

    let id = avatars[0]["id"].as_str().unwrap();
    let (status, headers, image) = send(&router, get(&format!("/avatar/{id}"), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert_eq!(image, PNG);

    assert!(dir.path().read_dir().unwrap().count() >= 2);
}

#[tokio::test]
async fn generate_rejects_invalid_input() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let cases = [
        json!({"count": 0, "set": "set1", "background": "none", "size": 100}),
        json!({"count": 51, "set": "set1", "background": "none", "size": 100}),
        json!({"count": 1, "set": "set9", "background": "none", "size": 100}),
        json!({"count": 1, "set": "set1", "background": "rainbow", "size": 100}),
        json!({"count": 1, "set": "set1", "background": "none", "size": 123}),
        json!({"count": 1, "set": "set1", "background": "none", "size": 100, "ttl_days": 31}),
    ];

    for case in cases {
        let (status, _, _) = send(&router, json_request("POST", "/generate", case, None)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn download_all_returns_zip_for_session() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let body = json!({"count": 2, "seed": "zip", "set": "set1", "background": "none", "size": 100});
    let (_, headers, _) = send(&router, json_request("POST", "/generate", body, None)).await;
    let cookie = session_cookie(&headers);

    let (status, headers, zip) = send(&router, get("/download-all", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/zip"
    );
    assert_eq!(&zip[..4], b"PK\x03\x04");
}

#[tokio::test]
async fn delete_removes_avatar_from_session() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let body = json!({"count": 1, "seed": "del", "set": "set1", "background": "none", "size": 100});
    let (_, headers, bytes) = send(&router, json_request("POST", "/generate", body, None)).await;
    let cookie = session_cookie(&headers);
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    let id = payload["avatars"][0]["id"].as_str().unwrap().to_string();

    let (status, _, _) = send(&router, del(&format!("/avatar/{id}"), &cookie)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = send(&router, get(&format!("/avatar/{id}"), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn clear_all_empties_the_session() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let body =
        json!({"count": 2, "seed": "clear", "set": "set1", "background": "none", "size": 100});
    let (_, headers, bytes) = send(&router, json_request("POST", "/generate", body, None)).await;
    let cookie = session_cookie(&headers);
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    let id = payload["avatars"][0]["id"].as_str().unwrap().to_string();

    let (status, _, _) = send(&router, post("/clear-all", &cookie)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = send(&router, get(&format!("/avatar/{id}"), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send(&router, get("/download-all", Some(&cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn index_renders_gallery_markup() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, headers, body) = send(&router, get("/", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key(header::SET_COOKIE));
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("generateForm"));
    assert!(html.contains("app.js"));
}

#[tokio::test]
async fn rate_limit_blocks_after_budget() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 1).await;

    let body = json!({"count": 1, "seed": "rl", "set": "set1", "background": "none", "size": 100});
    let (first, _, _) = send(
        &router,
        json_request("POST", "/generate", body.clone(), None),
    )
    .await;
    assert_eq!(first, StatusCode::OK);

    let (second, _, _) = send(&router, json_request("POST", "/generate", body, None)).await;
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn encoded_traversal_id_is_not_served() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, _, _) = send(&router, get("/avatar/%2e%2e%2f%2e%2e%2fetc%2fpasswd", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn same_seed_in_two_sessions_keeps_both_avatars() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let body =
        json!({"count": 1, "seed": "shared", "set": "set1", "background": "none", "size": 100});
    let (_, first_headers, first_body) = send(
        &router,
        json_request("POST", "/generate", body.clone(), None),
    )
    .await;
    let (_, second_headers, second_body) =
        send(&router, json_request("POST", "/generate", body, None)).await;

    let first_cookie = session_cookie(&first_headers);
    let second_cookie = session_cookie(&second_headers);
    let first_id = parse_id(&first_body);
    let second_id = parse_id(&second_body);

    assert_ne!(first_id, second_id);

    let (status, _, _) = send(
        &router,
        get(&format!("/avatar/{first_id}"), Some(&first_cookie)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = send(
        &router,
        get(&format!("/avatar/{second_id}"), Some(&second_cookie)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = send(&router, del(&format!("/avatar/{first_id}"), &second_cookie)).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another session must not delete"
    );
}

// New tests for improved coverage

#[tokio::test]
async fn index_renders_english_when_lang_cookie_set() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let cookie = "lang=en";
    let (status, _headers, body) = send(&router, get("/", Some(cookie))).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("RoboHash Avatar Generator"));
    assert!(html.contains("Generate"));
    assert!(html.contains("Download all"));
}

#[tokio::test]
async fn static_file_serves_js_and_css() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, headers, body) = send(&router, get("/static/app.js", None)).await;
    assert_eq!(status, StatusCode::OK);
    let js_content_type = headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(
        js_content_type.contains("javascript"),
        "JS content type: {js_content_type}"
    );
    assert!(!body.is_empty());

    let (status, headers, body) = send(&router, get("/static/style.css", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "text/css");
    assert!(!body.is_empty());
}

#[tokio::test]
async fn static_file_rejects_missing_file() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, _, _) = send(&router, get("/static/nonexistent.js", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn generate_rejects_upstream_error() {
    let server = MockServer::start().await;
    // Mock returns 500
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let store = Arc::new(Store::new(dir.path().to_path_buf()).unwrap());
    let state = build_state(config(server.uri(), dir.path().to_path_buf(), 100), store);
    let router = app(state);

    let body = json!({"count": 1, "seed": "err", "set": "set1", "background": "none", "size": 100});
    let (status, _, _) = send(&router, json_request("POST", "/generate", body, None)).await;
    // Should return BAD_GATEWAY when no avatars could be generated
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn download_returns_not_found_for_missing_avatar() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, _, _) = send(
        &router,
        get("/download/00000000-0000-0000-0000-000000000000", None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_returns_not_found_for_missing_avatar() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, _, _) = send(
        &router,
        del(
            "/avatar/00000000-0000-0000-0000-000000000000",
            "avatar_session_id=test",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn clear_all_returns_ok_for_empty_session() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let cookie = "avatar_session_id=empty-session";
    let (status, _, body) = send(&router, post("/clear-all", cookie)).await;
    assert_eq!(status, StatusCode::OK);
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["removed"], 0);
    assert!(payload["success"].as_bool().unwrap());
}

#[tokio::test]
async fn avatar_endpoint_returns_not_found_for_invalid_uuid() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (status, _, _) = send(&router, get("/avatar/not-a-uuid", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_headers_present_on_all_responses() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    // Test healthz endpoint
    let (_, headers, _) = send(&router, get("/healthz", None)).await;
    assert!(headers.contains_key("content-security-policy"));
    assert!(headers.contains_key("strict-transport-security"));
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("referrer-policy").unwrap(), "same-origin");
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert!(headers.contains_key("permissions-policy"));

    // Test index endpoint
    let (_, headers, _) = send(&router, get("/", None)).await;
    assert!(headers.contains_key("content-security-policy"));
}

#[tokio::test]
async fn generate_returns_too_many_requests_when_rate_limited() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 1).await;

    let body = json!({"count": 1, "seed": "rl", "set": "set1", "background": "none", "size": 100});
    let (first, _, _) = send(
        &router,
        json_request("POST", "/generate", body.clone(), None),
    )
    .await;
    assert_eq!(first, StatusCode::OK);

    let (second, _, _) = send(&router, json_request("POST", "/generate", body, None)).await;
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn download_all_returns_not_found_for_empty_session() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let cookie = "avatar_session_id=empty-session";
    let (status, _, _) = send(&router, get("/download-all", Some(cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn index_sets_session_cookie() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (_, headers, _) = send(&router, get("/", None)).await;
    let cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cookie.starts_with("avatar_session_id="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
}

#[tokio::test]
async fn index_defaults_to_pt_br_when_lang_cookie_missing() {
    let server = MockServer::start().await;
    let (router, _dir) = harness(&server, 100).await;

    let (_, _headers, body) = send(&router, get("/", None)).await;
    let html = String::from_utf8(body).unwrap();
    // Default language is pt-BR
    assert!(html.contains("Gerador de Avatares RoboHash"));
    assert!(html.contains("Gerar"));
    assert!(html.contains("Baixar tudo"));
}

#[tokio::test]
async fn generate_respects_trust_forwarded_header() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let store = Arc::new(Store::new(dir.path().to_path_buf()).unwrap());
    let mut cfg = config(server.uri(), dir.path().to_path_buf(), 100);
    cfg.trust_forwarded_headers = true;
    let state = build_state(cfg, store);
    let router = app(state);

    // Mock upstream
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(PNG),
        )
        .mount(&server)
        .await;

    let builder = Request::builder()
        .method("POST")
        .uri("/generate")
        .extension(peer())
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "1.2.3.4");
    let req = builder
        .body(Body::from(
            json!({"count": 1, "set": "set1", "background": "none", "size": 100}).to_string(),
        ))
        .unwrap();

    let (status, _, _) = send(&router, req).await;
    assert_eq!(status, StatusCode::OK);
}

fn parse_id(body: &[u8]) -> String {
    let payload: Value = serde_json::from_slice(body).unwrap();
    payload["avatars"][0]["id"].as_str().unwrap().to_string()
}
