// Brotli middleware и утилиты для сжатия
use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use brotli::enc::BrotliEncoderParams;
use std::io::Write;
use std::path::PathBuf;
use tokio::fs;

/// Middleware для раздачи предварительно сжатых .br файлов
///
/// Если клиент поддерживает Brotli (Accept-Encoding: br) и существует .br файл,
/// отдаём его с заголовком Content-Encoding: br
pub async fn brotli_static(request: Request, next: Next) -> Result<Response, StatusCode> {
    // Проверяем Accept-Encoding
    let accepts_brotli = request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("br"))
        .unwrap_or(false);

    if !accepts_brotli {
        return Ok(next.run(request).await);
    }

    // Получаем путь из URI
    let path = request.uri().path();

    // Путь к .br файлу
    let br_path = PathBuf::from(format!("../dist/client{}.br", path));

    // Проверяем существует ли .br файл
    if !br_path.exists() || !br_path.is_file() {
        return Ok(next.run(request).await);
    }

    // Читаем .br файл
    let content = match fs::read(&br_path).await {
        Ok(c) => c,
        Err(_) => return Ok(next.run(request).await),
    };

    // Определяем Content-Type по расширению оригинального файла
    let content_type = if path.ends_with(".js") {
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
    };

    tracing::debug!("Serving Brotli: {} ({} bytes)", path, content.len());

    // Возвращаем сжатый файл с правильными заголовками
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

/// Middleware для динамического Brotli сжатия HTML ответов от SSR
///
/// Проверяет Accept-Encoding: br и сжимает HTML на лету
pub async fn brotli_compress_html(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Проверяем поддержку Brotli
    let accepts_brotli = request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("br"))
        .unwrap_or(false);

    let response = next.run(request).await;

    // Если клиент не поддерживает brotli или это не HTML - возвращаем как есть
    if !accepts_brotli {
        return Ok(response);
    }

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("text/html") {
        return Ok(response);
    }

    // Извлекаем body
    let (parts, body) = response.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Сжимаем с помощью Brotli (качество 4 для скорости)
    let mut compressed = Vec::new();
    let params = BrotliEncoderParams {
        quality: 4, // Быстрее чем 6, но хорошая компрессия для HTML
        ..Default::default()
    };

    let mut compressor = brotli::CompressorWriter::with_params(&mut compressed, 4096, &params);
    if compressor.write_all(&body_bytes).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    drop(compressor);

    // Создаём новый response с сжатыми данными
    let mut response = Response::from_parts(parts, Body::from(compressed));
    response
        .headers_mut()
        .insert(header::CONTENT_ENCODING, HeaderValue::from_static("br"));
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));

    Ok(response)
}
