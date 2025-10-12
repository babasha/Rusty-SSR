use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

/// API прокси handler - перенаправляет /api/* на https://enddel.com/api/*
pub async fn api_proxy_handler(Path(path): Path<String>) -> Result<Response, StatusCode> {
    let client = reqwest::Client::new();
    let url = format!("https://enddel.com/api/{}", path);

    tracing::debug!("Proxying API request: {}", url);

    match client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Rust SSR Server)")
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            Ok((
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
                [(header::CONTENT_TYPE, "application/json")],
                body,
            )
                .into_response())
        }
        Err(e) => {
            tracing::error!("API proxy error: {}", e);
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}
