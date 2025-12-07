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
use std::path::Path;
use tokio::fs;

/// Middleware to serve pre-compressed .br files
///
/// If the client supports Brotli (Accept-Encoding: br) and a .br file exists,
/// serves it with Content-Encoding: br header.
///
/// # Example
/// ```rust,no_run
/// use axum::{Router, middleware};
/// use rusty_ssr::middleware::brotli_static;
///
/// let app = Router::new()
///     .layer(middleware::from_fn(brotli_static));
/// ```
pub async fn brotli_static(request: Request, next: Next) -> Result<Response, StatusCode> {
    // Check Accept-Encoding
    let accepts_brotli = request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("br"))
        .unwrap_or(false);

    if !accepts_brotli {
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

    if !Path::new(&br_path).exists() {
        return Ok(next.run(request).await);
    }

    // Read .br file
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
/// use axum::{Router, middleware};
/// use rusty_ssr::middleware::brotli_compress;
///
/// let app = Router::new()
///     .layer(middleware::from_fn(brotli_compress));
/// ```
pub async fn brotli_compress(request: Request, next: Next) -> Result<Response, StatusCode> {
    let accepts_brotli = request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("br"))
        .unwrap_or(false);

    let response = next.run(request).await;

    if !accepts_brotli {
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
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Compress with Brotli (quality 4 for speed)
    let mut compressed = Vec::new();
    let params = BrotliEncoderParams {
        quality: 4,
        ..Default::default()
    };

    let mut compressor = brotli::CompressorWriter::with_params(&mut compressed, 4096, &params);
    if compressor.write_all(&body_bytes).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    drop(compressor);

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
