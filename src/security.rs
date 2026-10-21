use axum::{
    http::{header, HeaderMap, HeaderValue},
    response::Response,
};
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Security headers middleware layer
#[derive(Clone)]
pub struct SecurityHeadersLayer;

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersService { inner }
    }
}

/// Security headers middleware service
#[derive(Clone)]
pub struct SecurityHeadersService<S> {
    inner: S,
}

impl<S, B> Service<axum::http::Request<B>> for SecurityHeadersService<S>
where
    S: Service<axum::http::Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<B>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut response = inner.call(req).await?;
            add_security_headers(response.headers_mut());
            Ok(response)
        })
    }
}

/// Add security headers to the response
fn add_security_headers(headers: &mut HeaderMap) {
    // Content Security Policy
    // Restrict sources to self for scripts, styles, images, fonts, and connect
    // Allow inline styles for askama-rendered HTML (style attributes)
    // Allow data: for images (avatar PNGs served from our domain)
    let csp = "default-src 'self'; \
               script-src 'self'; \
               style-src 'self' 'unsafe-inline'; \
               img-src 'self' data:; \
               font-src 'self'; \
               connect-src 'self'; \
               frame-ancestors 'none'; \
               base-uri 'self'; \
               form-action 'self'";

    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(csp),
    );

    // HTTP Strict Transport Security
    // 1 year, include subdomains, preload
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
    );

    // X-Content-Type-Options
    // Prevent MIME type sniffing
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );

    // Referrer-Policy
    // Only send referrer for same-origin requests
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );

    // X-Frame-Options (additional protection alongside frame-ancestors)
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));

    // Permissions-Policy
    // Disable unnecessary browser features
    let permissions_policy = "accelerometer=(), camera=(), geolocation=(), gyroscope=(), \
                              magnetometer=(), microphone=(), payment=(), usb=()";
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static(permissions_policy),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    async fn test_app() -> Router {
        Router::new().route("/", get(|| async { "ok" }).layer(SecurityHeadersLayer))
    }

    #[tokio::test]
    async fn security_headers_present_on_response() {
        let app = test_app().await;

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let headers = response.headers();

        // CSP
        let csp = headers
            .get(header::CONTENT_SECURITY_POLICY)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains("script-src 'self'"));
        assert!(csp.contains("style-src 'self' 'unsafe-inline'"));

        // HSTS
        let hsts = headers
            .get(header::STRICT_TRANSPORT_SECURITY)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(hsts.contains("max-age=31536000"));
        assert!(hsts.contains("includeSubDomains"));
        assert!(hsts.contains("preload"));

        // X-Content-Type-Options
        assert_eq!(
            headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );

        // Referrer-Policy
        assert_eq!(headers.get(header::REFERRER_POLICY).unwrap(), "same-origin");

        // X-Frame-Options
        assert_eq!(headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");

        // Permissions-Policy
        let perm = headers.get("permissions-policy").unwrap().to_str().unwrap();
        assert!(perm.contains("camera=()"));
        assert!(perm.contains("geolocation=()"));
        assert!(perm.contains("microphone=()"));
    }
}
