//! `GET /@{handle}/{code}/info` — public preview page for a short link.

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse, Response};
use jacquard_common::types::string::Handle;

use crate::api::shortlink::qr_svg_inline;
use crate::auth::Authenticator;
use crate::domain::ShortCode;
use crate::error::AppError;
use crate::resolver::LinkResolver;
use crate::AppState;

#[derive(Template)]
#[template(path = "info.html")]
struct InfoTemplate {
    url: String,
    updated_at: Option<String>,
    handle: String,
    code: String,
    /// The canonical short link, built from the configured base URL.
    ///
    /// The template used to assemble `atpr.to/@{handle}/{code}` itself, which
    /// is a lie on any instance not served from that domain.
    short_url: String,
    qr_svg: String,
}

/// Show a preview page for a short link: destination, last-modified date, QR code.
///
/// Uses the same resolver as the redirect, rather than calling Slingshot
/// directly — so the two can no longer disagree about whether a link exists or
/// what it points at.
#[tracing::instrument(skip(state), fields(handle = %handle, code = %code))]
pub async fn info<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    Path((handle, code)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let parsed_handle: Handle =
        Handle::new_owned(&handle).map_err(|_| AppError::BadRequest("Invalid handle"))?;
    let parsed_code = ShortCode::parse(&code)?;

    let link = state.links.resolve(&parsed_handle, &parsed_code).await?;

    let short_url = state.config.base_url.short_url(&handle, &code);
    let tmpl = InfoTemplate {
        // Already scheme-checked by `TargetUrl`, so this `<a href>` cannot
        // receive a `javascript:` destination. Askama escapes characters, not
        // schemes, so it was never the guard here.
        url: link.target.to_string(),
        updated_at: link.updated_at,
        handle,
        code,
        qr_svg: qr_svg_inline(&short_url)?,
        short_url,
    };
    let html = tmpl.render().map_err(AppError::internal)?;
    Ok(Html(html).into_response())
}
