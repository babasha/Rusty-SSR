//! Brotli compression middleware for Axum

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use brotli::enc::BrotliEncoderParams;
use std::io::Write;
use tokio::fs;

/// Largest response body this middleware will buffer in order to compress it.
///
/// Compression needs the whole body in memory, so without a ceiling a single
/// oversized response is an unbounded allocation. 32 MiB is far above any
/// server-rendered HTML document and far below anything worth worrying about;
/// a body past it is refused rather than silently buffered.
const MAX_COMPRESSIBLE_BYTES: usize = 32 * 1024 * 1024;

/// Whether the client said it can take Brotli.
///
/// Both middlewares start by asking this, and each used to ask it in its own
/// five copied lines.
fn accepts_brotli(request: &Request) -> bool {
    request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("br"))
}

/// Middleware to serve pre-compressed .br files
///
/// If the client supports Brotli (Accept-Encoding: br) and a .br file exists,
/// serves it with Content-Encoding: br header.
///
/// # Example
/// ```rust,no_run
/// use axum::{Router, middleware, routing::get};
/// use rusty_ssr::middleware::brotli_static;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "hello" }))
///     .layer(middleware::from_fn(brotli_static));
/// ```
pub async fn brotli_static(request: Request, next: Next) -> Result<Response, StatusCode> {
    if !accepts_brotli(&request) {
        return Ok(next.run(request).await);
    }

    // Get path from URI
    let path = request.uri().path();

    // Skip if path contains ..
    if path.contains("..") {
        return Ok(next.run(request).await);
    }

    // Look for .br file in current directory
    let br_path = format!(".{}.br", path);

    // Straight to the read: it answers "is there one?" and "what is in it?" in
    // a single syscall. The `Path::exists()` that used to guard this was a
    // *blocking* stat on the async runtime's thread, and it asked a question
    // the read below was about to ask again — with a window in between where
    // the answer could change.
    let content = match fs::read(&br_path).await {
        Ok(c) => c,
        Err(_) => return Ok(next.run(request).await),
    };

    // Determine Content-Type
    let content_type = guess_content_type(path);

    tracing::debug!("Serving Brotli: {} ({} bytes)", path, content.len());

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (header::CONTENT_ENCODING, HeaderValue::from_static("br")),
            (header::VARY, HeaderValue::from_static("Accept-Encoding")),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000"),
            ),
        ],
        Body::from(content),
    )
        .into_response())
}

/// Middleware for dynamic Brotli compression of HTML responses
///
/// Compresses HTML responses on-the-fly if the client supports Brotli.
///
/// # Example
/// ```rust,no_run
/// use axum::{Router, middleware, routing::get};
/// use rusty_ssr::middleware::brotli_compress;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "hello" }))
///     .layer(middleware::from_fn(brotli_compress));
/// ```
pub async fn brotli_compress(request: Request, next: Next) -> Result<Response, StatusCode> {
    let wanted = accepts_brotli(&request);

    let response = next.run(request).await;

    if !wanted {
        return Ok(response);
    }

    // Already encoded by something else — a `tower-http` compression layer, or
    // a handler that served a pre-compressed file. Compressing it again
    // produces a body the client cannot read: the header would still say `br`
    // once while the bytes had been through it twice.
    if response.headers().contains_key(header::CONTENT_ENCODING) {
        return Ok(response);
    }

    // Only compress HTML
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("text/html") {
        return Ok(response);
    }

    // Extract body
    let (parts, body) = response.into_parts();
    let body_bytes = match axum::body::to_bytes(body, MAX_COMPRESSIBLE_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Compress off the runtime.
    //
    // Brotli is CPU-bound and this ran inline on the async worker thread, where
    // for the whole of a large page it blocked every other task that thread was
    // driving — including, on a single-threaded runtime, the accept loop.
    // `spawn_blocking` puts it where blocking work belongs.
    let compressed = tokio::task::spawn_blocking(move || {
        // Quality 4: the knee of the curve for HTML. Higher settings cost
        // several times the CPU for a few percent of size.
        let params = BrotliEncoderParams {
            quality: 4,
            ..Default::default()
        };
        let mut out = Vec::with_capacity(body_bytes.len() / 3);
        let mut compressor = brotli::CompressorWriter::with_params(&mut out, 4096, &params);
        compressor.write_all(&body_bytes).map(|_| ())?;
        drop(compressor);
        Ok::<Vec<u8>, std::io::Error>(out)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create response with compressed data
    let mut response = Response::from_parts(parts, Body::from(compressed));
    response
        .headers_mut()
        .insert(header::CONTENT_ENCODING, HeaderValue::from_static("br"));
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));

    Ok(response)
}

/// Guess content type from file extension
fn guess_content_type(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "application/javascript; charset=UTF-8"
    } else if path.ends_with(".css") {
        "text/css; charset=UTF-8"
    } else if path.ends_with(".html") {
        "text/html; charset=UTF-8"
    } else if path.ends_with(".json") {
        "application/json; charset=UTF-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use tower::ServiceExt; // for `oneshot`

    /// Undo whatever a `br` fixture wrote, however the test ends.
    ///
    /// `brotli_static` resolves `.br` files relative to the process's working
    /// directory, so a test of the hit path has to put one there. Cleaning up
    /// in `Drop` means a failing assertion still leaves the tree as it found it.
    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn write(name: &str, body: &[u8]) -> Self {
            let path = std::path::PathBuf::from(format!(".{}", name));
            std::fs::write(&path, body).expect("write fixture");
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn brotli_bytes(input: &[u8]) -> Vec<u8> {
        let params = BrotliEncoderParams {
            quality: 4,
            ..Default::default()
        };
        let mut out = Vec::new();
        let mut w = brotli::CompressorWriter::with_params(&mut out, 4096, &params);
        w.write_all(input).unwrap();
        drop(w);
        out
    }

    fn unbrotli(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut w = brotli::DecompressorWriter::new(&mut out, 4096);
        w.write_all(input).unwrap();
        drop(w);
        out
    }

    const PAGE: &str = "<!doctype html><html><body>the page, repeated: \
                        aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</body></html>";

    fn html_app() -> Router {
        Router::new()
            .route(
                "/page",
                get(|| async { ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], PAGE) }),
            )
            .route(
                "/data.json",
                get(|| async { ([(header::CONTENT_TYPE, "application/json")], "{\"a\":1}") }),
            )
            .route(
                "/already",
                get(|| async {
                    (
                        [
                            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                            (header::CONTENT_ENCODING, "br"),
                        ],
                        "pre-compressed bytes",
                    )
                }),
            )
            .layer(axum::middleware::from_fn(brotli_compress))
    }

    async fn body_of(response: Response) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    // ── dynamic compression ─────────────────────────────────────────────────

    #[tokio::test]
    async fn html_is_compressed_when_the_client_asks_for_brotli() {
        let response = html_app()
            .oneshot(
                Request::builder()
                    .uri("/page")
                    .header(header::ACCEPT_ENCODING, "gzip, deflate, br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING).unwrap(),
            "br"
        );
        assert_eq!(
            response.headers().get(header::VARY).unwrap(),
            "Accept-Encoding",
            "a cache must be told the body varies by encoding"
        );

        let body = body_of(response).await;
        assert!(body.len() < PAGE.len(), "compression must actually shrink it");
        assert_eq!(
            unbrotli(&body),
            PAGE.as_bytes(),
            "and it must decompress back to exactly the page"
        );
    }

    #[tokio::test]
    async fn nothing_is_compressed_without_accept_encoding() {
        let response = html_app()
            .oneshot(Request::builder().uri("/page").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        assert_eq!(body_of(response).await, PAGE.as_bytes());
    }

    #[tokio::test]
    async fn a_client_asking_only_for_gzip_gets_plain_bytes() {
        let response = html_app()
            .oneshot(
                Request::builder()
                    .uri("/page")
                    .header(header::ACCEPT_ENCODING, "gzip, deflate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        assert_eq!(body_of(response).await, PAGE.as_bytes());
    }

    #[tokio::test]
    async fn only_html_is_compressed() {
        let response = html_app()
            .oneshot(
                Request::builder()
                    .uri("/data.json")
                    .header(header::ACCEPT_ENCODING, "br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(
            response.headers().get(header::CONTENT_ENCODING).is_none(),
            "this middleware is for documents, not for every response"
        );
    }

    /// An already-encoded body must be left alone. Compressing it again yields
    /// bytes that have been through brotli twice under a header claiming once,
    /// which no client can undo — and it is exactly what happens when this sits
    /// above another compression layer.
    #[tokio::test]
    async fn an_already_encoded_response_is_not_compressed_again() {
        let response = html_app()
            .oneshot(
                Request::builder()
                    .uri("/already")
                    .header(header::ACCEPT_ENCODING, "br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING).unwrap(),
            "br"
        );
        assert_eq!(
            body_of(response).await,
            b"pre-compressed bytes",
            "the body must be passed through untouched"
        );
    }

    // ── pre-compressed files ────────────────────────────────────────────────

    fn static_app() -> Router {
        Router::new()
            .route("/*path", get(|| async { "from the handler" }))
            .layer(axum::middleware::from_fn(brotli_static))
    }

    #[tokio::test]
    async fn a_precompressed_file_is_served_when_one_exists() {
        let name = "/rusty_ssr_brotli_fixture.js";
        let payload = b"console.log('hello from the .br file');";
        let _fixture = Fixture::write(&format!("{name}.br"), &brotli_bytes(payload));

        let response = static_app()
            .oneshot(
                Request::builder()
                    .uri(name)
                    .header(header::ACCEPT_ENCODING, "br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING).unwrap(),
            "br"
        );
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/javascript; charset=UTF-8",
            "the type comes from the extension, not from the .br suffix"
        );
        assert_eq!(unbrotli(&body_of(response).await), payload);
    }

    #[tokio::test]
    async fn without_a_precompressed_file_the_handler_answers() {
        let response = static_app()
            .oneshot(
                Request::builder()
                    .uri("/definitely_not_on_disk_12345.js")
                    .header(header::ACCEPT_ENCODING, "br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        assert_eq!(body_of(response).await, b"from the handler");
    }

    /// Even with a `.br` file sitting right there, a client that did not ask
    /// for brotli must get the handler's answer.
    #[tokio::test]
    async fn a_precompressed_file_is_not_served_to_a_client_that_cannot_read_it() {
        let name = "/rusty_ssr_brotli_fixture_unwanted.js";
        let _fixture = Fixture::write(&format!("{name}.br"), &brotli_bytes(b"x"));

        let response = static_app()
            .oneshot(Request::builder().uri(name).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        assert_eq!(body_of(response).await, b"from the handler");
    }

    /// `..` in the path must never be turned into a filesystem lookup.
    #[tokio::test]
    async fn a_traversing_path_is_not_looked_up_on_disk() {
        let response = static_app()
            .oneshot(
                Request::builder()
                    .uri("/a/../../etc/passwd")
                    .header(header::ACCEPT_ENCODING, "br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        assert_eq!(body_of(response).await, b"from the handler");
    }

    // ── content types ───────────────────────────────────────────────────────

    #[test]
    fn content_types_come_from_the_extension() {
        assert_eq!(
            guess_content_type("/a/b.js"),
            "application/javascript; charset=UTF-8"
        );
        assert_eq!(guess_content_type("/a/b.css"), "text/css; charset=UTF-8");
        assert_eq!(guess_content_type("/a/b.html"), "text/html; charset=UTF-8");
        assert_eq!(
            guess_content_type("/a/b.json"),
            "application/json; charset=UTF-8"
        );
        assert_eq!(guess_content_type("/a/b.svg"), "image/svg+xml");
        assert_eq!(guess_content_type("/a/b.woff2"), "font/woff2");
        assert_eq!(guess_content_type("/a/b.woff"), "font/woff");
        assert_eq!(guess_content_type("/a/b.bin"), "application/octet-stream");
        assert_eq!(
            guess_content_type("/no-extension"),
            "application/octet-stream"
        );
    }

    #[test]
    fn accept_encoding_is_read_by_token() {
        let with = |v: Option<&str>| {
            let mut b = Request::builder().uri("/");
            if let Some(v) = v {
                b = b.header(header::ACCEPT_ENCODING, v);
            }
            accepts_brotli(&b.body(Body::empty()).unwrap())
        };

        assert!(with(Some("br")));
        assert!(with(Some("gzip, deflate, br")));
        assert!(with(Some("br;q=1.0, gzip;q=0.8")));
        assert!(!with(Some("gzip, deflate")));
        assert!(!with(Some("")));
        assert!(!with(None));
    }
}
