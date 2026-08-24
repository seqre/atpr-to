//! `GET /@{handle}/{code}/info` — public preview page for a short link.

use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse, Response};
use jacquard_common::types::string::Handle;

use crate::api::shortlink::qr_svg_inline;
use crate::domain::ShortCode;
use crate::error::AppError;
use crate::resolver::LinkResolver;
use atpr_core::redirect::ResolveState;

/// When a link was last changed, in both the forms the page needs.
///
/// Kept together so the template cannot accidentally print one and mark up the
/// other: the machine form belongs in `datetime`, the human form on the page.
struct Stamp {
    /// The record's value, verbatim, for `<time datetime>`.
    raw: String,
    /// The same instant for a person to read.
    human: String,
}

#[derive(Template)]
#[template(path = "info.html")]
struct InfoTemplate {
    url: String,
    updated_at: Option<Stamp>,
    handle: String,
    code: String,
    /// The canonical short link, built from the configured base URL.
    ///
    /// The template used to assemble `atpr.to/@{handle}/{code}` itself, which
    /// is a lie on any instance not served from that domain.
    short_url: String,
    qr_svg: String,
}

/// Render an ISO-8601 instant as a date a person would say out loud.
///
/// This page is read by a stranger deciding whether to follow a link, and it
/// was showing them `2026-08-18T17:05:20.042538Z` — microseconds included —
/// because the value goes straight from the record to the page. The raw string
/// still has to appear in `datetime`, which is where it belongs and what the
/// integration test actually asserts.
///
/// UTC rather than a local time: the server has no idea where the reader is,
/// and guessing wrong by a day is worse than being plainly universal. The day
/// is unpadded, because "8 August" is what people write.
///
/// Anything unparseable falls back to the raw string. A record's `updatedAt` is
/// written by whatever client made it, so it is not ours to assume well-formed,
/// and showing an odd date beats hiding when the link last changed.
fn human_date(iso: &str) -> String {
    const HUMAN: &[time::format_description::FormatItem<'_>] =
        time::macros::format_description!("[day padding:none] [month repr:long] [year]");

    time::OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339)
        .ok()
        // Normalise before formatting. A record written as `02:30+05:00` names
        // an instant on the previous UTC day, and printing the calendar date it
        // happens to carry would report the wrong day for anyone east of here.
        .map(|t| t.to_offset(time::UtcOffset::UTC))
        .and_then(|t| t.format(HUMAN).ok())
        .unwrap_or_else(|| iso.to_string())
}

/// Show a preview page for a short link: destination, last-modified date, QR code.
///
/// Uses the same resolver as the redirect, rather than calling Slingshot
/// directly — so the two can no longer disagree about whether a link exists or
/// what it points at.
#[tracing::instrument(skip(state), fields(handle = %handle, code = %code))]
pub async fn info(
    State(state): State<Arc<ResolveState>>,
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
        updated_at: link.updated_at.map(|raw| Stamp {
            human: human_date(&raw),
            raw,
        }),
        handle,
        code,
        qr_svg: qr_svg_inline(&short_url)?,
        short_url,
    };
    let html = tmpl.render().map_err(AppError::internal)?;
    Ok(Html(html).into_response())
}

#[cfg(test)]
mod tests {
    use super::human_date;

    #[test]
    fn test_a_record_timestamp_becomes_a_date() {
        assert_eq!(human_date("2024-01-15T10:00:00Z"), "15 January 2024");
        // The shape a real PDS writes, microseconds and all.
        assert_eq!(human_date("2026-08-18T17:05:20.042538Z"), "18 August 2026");
        // Unpadded: "8 August", not "08 August".
        assert_eq!(human_date("2026-08-08T00:00:00Z"), "8 August 2026");
    }

    /// An offset is normalised to the instant it names, not printed as written.
    #[test]
    fn test_an_offset_is_respected() {
        // 02:30 on the 18th at +05:00 is 21:30 on the 17th in UTC.
        assert_eq!(human_date("2026-08-18T02:30:00+05:00"), "17 August 2026");
    }

    /// `updatedAt` is written by whatever client made the record, so it is not
    /// ours to assume well-formed. Showing an odd date beats hiding the field.
    #[test]
    fn test_an_unparseable_value_is_shown_as_it_arrived() {
        for raw in ["", "last tuesday", "2024-13-45T99:99:99Z"] {
            assert_eq!(human_date(raw), raw);
        }
    }
}
