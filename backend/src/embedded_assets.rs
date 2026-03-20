use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../frontend/dist"]
#[allow(dead_code)]
struct FrontendAssets;

pub async fn serve_embedded_frontend(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try the exact path first
    if let Some(file) = <FrontendAssets as Embed>::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data.to_vec(),
        )
            .into_response();
    }

    // SPA fallback: serve index.html for all non-file routes
    match <FrontendAssets as Embed>::get("index.html") {
        Some(file) => Html(String::from_utf8_lossy(&file.data).to_string()).into_response(),
        None => (StatusCode::NOT_FOUND, "Frontend not built").into_response(),
    }
}
