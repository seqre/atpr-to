use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse, Response};
use qrcode::render::svg;
use qrcode::QrCode;

use crate::error;
use crate::resolve::resolve_via_slingshot;
use crate::AppState;

#[derive(Template)]
#[template(path = "info.html")]
struct InfoTemplate {
    url: String,
    updated_at: Option<String>,
    handle: String,
    code: String,
    qr_svg: String,
}

/// Show a preview page for a short link: destination URL, creation date, optional expiry, and QR code.
#[tracing::instrument(skip(state), fields(handle, code))]
pub async fn info(
    State(state): State<Arc<AppState>>,
    Path((handle, code)): Path<(String, String)>,
) -> Response {
    let link = match resolve_via_slingshot(&state.http, &state.config.slingshot_url, &handle, &code)
        .await
    {
        Ok(l) => l,
        Err(e) => {
            if e.to_string().contains("404") {
                return error::not_found("Link not found");
            }
            return error::bad_gateway(&format!("Could not resolve link: {e}"));
        }
    };

    let short_url = format!("{}/@{}/{}", state.config.base_url, handle, code);
    let qr_svg = QrCode::new(short_url.as_bytes())
        .map(|qr| qr.render::<svg::Color>().min_dimensions(200, 200).build())
        .unwrap_or_default();

    let tmpl = InfoTemplate {
        url: link.url,
        updated_at: link.updated_at,
        handle,
        code,
        qr_svg,
    };
    match tmpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => error::internal_error(&format!("Template error: {e}")),
    }
}
