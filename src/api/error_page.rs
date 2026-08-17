//! Content negotiation for error responses.
//!
//! [`AppError::into_response`](crate::error::AppError) takes `self` and nothing
//! else, so the `Accept` header is structurally unreachable from inside it.
//! Negotiation has to happen somewhere the request still exists, which is what
//! this middleware is for.
//!
//! It keys on the *status code* rather than on `AppError`, and that is the
//! point: axum's own rejections (405, 415, a malformed `Form` body), the rate
//! limiter's 429 and the timeout's 504 never pass through `AppError` at all.
//! Keying on status means every one of them gets the same page, including the
//! ones nobody has written yet.

use askama::Template;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::API_PREFIX;

/// Cap on the error body read back before rendering.
///
/// Every error body this app produces is a few hundred bytes. The cap bounds a
/// hostile or broken upstream, and is not a real limit on anything we send.
const MAX_ERROR_BODY: usize = 8 * 1024;

/// Set by current browsers on a top-level navigation, including a form POST.
///
/// Checked before the path, because `POST /api/login` and `POST /api/logout`
/// are browser form targets that happen to live under the JSON prefix.
const SEC_FETCH_DEST: &str = "sec-fetch-dest";

/// The body shape [`crate::error::AppError`] renders.
#[derive(serde::Deserialize)]
struct JsonError {
    error: String,
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage {
    /// HTTP status, for the title and the page's own labelling.
    status: u16,
    /// Server-authored headline.
    heading: &'static str,
    /// Server-authored explanation of what to do next.
    message: &'static str,
    /// The machine message from the JSON body, when there was one.
    detail: Option<String>,
}

/// Whether this request should be answered with a page rather than JSON.
///
/// `*/*` is deliberately not HTML: curl and every JSON client send it, and a
/// page is the wrong answer for them.
fn wants_html(headers: &HeaderMap, path: &str) -> bool {
    if headers
        .get(SEC_FETCH_DEST)
        .is_some_and(|v| v.as_bytes() == b"document")
    {
        return true;
    }
    if under_api(path) {
        return false;
    }
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept
                .split(',')
                .any(|m| m.trim_start().starts_with("text/html"))
        })
}

/// Whether a path is inside the JSON API.
///
/// Prefix matching alone would claim `/apiary`, so the next character has to be
/// a separator.
fn under_api(path: &str) -> bool {
    path == API_PREFIX
        || path
            .strip_prefix(API_PREFIX)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Server-authored copy per status.
///
/// Exhaustive by construction: the fallthrough arms mean a status nobody
/// anticipated still gets a page rather than a blank one.
fn copy_for(status: StatusCode) -> (&'static str, &'static str) {
    match status {
        StatusCode::NOT_FOUND => (
            "No link here",
            "This short link doesn't exist, or the person who made it deleted it. \
             Links live in their owner's own repository, so only they can bring it back.",
        ),
        StatusCode::BAD_REQUEST => (
            "That address isn't valid",
            "Check the handle and the code in the address bar for a typo.",
        ),
        StatusCode::UNAUTHORIZED => (
            "You're signed out",
            "Your session ended or was signed out elsewhere. Sign in again to carry on.",
        ),
        StatusCode::CONFLICT => (
            "That short code is taken",
            "You already have a link using this code. Pick another, \
             or edit the one you have.",
        ),
        StatusCode::TOO_MANY_REQUESTS => (
            "Too fast",
            "You've made a lot of requests in a short time. Wait a few seconds and try again.",
        ),
        StatusCode::METHOD_NOT_ALLOWED | StatusCode::UNSUPPORTED_MEDIA_TYPE => (
            "That didn't arrive in a form we can read",
            "Something sent this request the wrong way. Try again from the page it came from.",
        ),
        StatusCode::BAD_GATEWAY | StatusCode::GATEWAY_TIMEOUT | StatusCode::SERVICE_UNAVAILABLE => {
            (
                "The network isn't answering",
                "Resolving a link means asking the wider AT Protocol network, and that \
                 didn't answer in time. This is usually brief — try again in a moment.",
            )
        }
        s if s.is_server_error() => (
            "Something broke on our side",
            "That's a bug here, not anything you did. Try again in a moment.",
        ),
        _ => (
            "That request didn't work",
            "Try again, or go back and start over.",
        ),
    }
}

/// Rewrite error responses as HTML when the caller is a browser navigating.
pub async fn html_errors(req: Request, next: Next) -> Response {
    let wants = wants_html(req.headers(), req.uri().path());
    let response = next.run(req).await;

    let status = response.status();
    if !wants || !(status.is_client_error() || status.is_server_error()) {
        return response;
    }
    // Nothing renders an HTML error today. The guard is so that the first
    // thing that does is not rendered twice.
    if response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"text/html"))
    {
        return response;
    }
    render(status, response).await
}

/// Swap an error response's body for a rendered page.
async fn render(status: StatusCode, response: Response) -> Response {
    // Mutate the existing parts rather than building a fresh response: the
    // security headers and the rate limiter's `Retry-After` are already on
    // here, and rebuilding would silently drop them.
    let (mut parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_ERROR_BODY)
        .await
        .unwrap_or_default();

    // Only a body that parses as our own `{"error": …}` contributes detail.
    // axum's rejections are text/plain and the limiter's 429 is a bare string;
    // those fall through to the server-authored copy alone.
    let detail = serde_json::from_slice::<JsonError>(&bytes)
        .ok()
        .map(|b| b.error);

    let (heading, message) = copy_for(status);
    let page = ErrorPage {
        status: status.as_u16(),
        heading,
        message,
        detail,
    };
    let Ok(html) = page.render() else {
        return Response::from_parts(parts, Body::from(bytes));
    };

    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    // Same reasoning as the JSON path: a cached 404 outlives the link that
    // would fix it. The 405, 429 and 504 paths had no cache policy at all.
    parts
        .headers
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    // The JSON body's length is still on `parts` and is now wrong. Behind API
    // Gateway a stale Content-Length is forwarded to the client verbatim.
    parts
        .headers
        .insert(header::CONTENT_LENGTH, HeaderValue::from(html.len()));
    Response::from_parts(parts, Body::from(html))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    /// The login and logout forms POST to `/api`, so the path test alone would
    /// answer a navigating browser with JSON.
    #[test]
    fn test_navigation_wants_html_even_under_the_api_prefix() {
        let h = headers(&[("sec-fetch-dest", "document")]);
        assert!(wants_html(&h, "/api/login"));
    }

    #[test]
    fn test_browser_accept_wants_html() {
        let h = headers(&[(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )]);
        assert!(wants_html(&h, "/"));
    }

    #[test]
    fn test_api_path_stays_json_under_a_browser_accept() {
        let h = headers(&[("accept", "text/html,*/*;q=0.8")]);
        assert!(!wants_html(&h, "/api/links"));
    }

    /// curl and every JSON client send `*/*`; a page is the wrong answer.
    #[test]
    fn test_wildcard_accept_is_json() {
        let h = headers(&[("accept", "*/*")]);
        assert!(!wants_html(&h, "/"));
    }

    /// Two existing tests drive the router with no `Accept` at all and assert
    /// on the JSON body. They must keep getting JSON.
    #[test]
    fn test_missing_accept_is_json() {
        assert!(!wants_html(&HeaderMap::new(), "/"));
        assert!(!wants_html(&HeaderMap::new(), "/favicon.ico"));
    }

    #[test]
    fn test_api_prefix_does_not_claim_a_lookalike_path() {
        assert!(under_api("/api"));
        assert!(under_api("/api/links"));
        assert!(!under_api("/apiary"));
        assert!(!under_api("/"));
    }

    #[test]
    fn test_every_error_status_has_a_page() {
        for code in [400, 401, 404, 405, 409, 415, 418, 429, 500, 502, 503, 504] {
            let status = StatusCode::from_u16(code).unwrap();
            let (heading, message) = copy_for(status);
            let html = ErrorPage {
                status: code,
                heading,
                message,
                detail: None,
            }
            .render()
            .unwrap_or_else(|e| panic!("{code} failed to render: {e}"));
            assert!(html.contains(&code.to_string()), "{code} lost its status");
            assert!(
                !heading.is_empty() && !message.is_empty(),
                "{code} has no copy"
            );
        }
    }

    /// `detail` is server-authored today — `AppError::BadRequest` takes
    /// `&'static str` precisely so upstream text cannot reach a client — but
    /// it is the one field on this page that carries a runtime string, so it
    /// is worth pinning that the template escapes it.
    ///
    /// Asserted against the raw form rather than a particular entity, since
    /// which one the escaper emits is its business.
    #[test]
    fn test_detail_is_escaped() {
        let html = ErrorPage {
            status: 400,
            heading: "h",
            message: "m",
            detail: Some("<script>alert(1)</script>".to_string()),
        }
        .render()
        .unwrap();
        // Scoped to the injected string: the shell legitimately carries a
        // `<script src>` of its own, so a bare `<script` check would pass or
        // fail for reasons that have nothing to do with escaping.
        assert!(
            !html.contains("<script>alert"),
            "raw markup reached the page: {html}"
        );
        assert!(
            html.contains("alert(1)"),
            "the message should still be readable"
        );
    }
}
