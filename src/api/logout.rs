//! `POST /api/logout` — revoke the session and clear its cookie.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;

use crate::auth::{clear_session, Authenticator};
use crate::AppState;

/// Revoke the session server-side, then clear the cookie.
///
/// Bug #6: this used to only clear the cookie. The access token, refresh token
/// and DPoP key stayed live in the store forever, so a cookie captured before
/// "sign out" kept working afterwards — the user was told they had logged out
/// and had not.
///
/// Still requires no authentication: clearing a cookie that names no live
/// session is harmless, and refusing would leave a user with a stale cookie no
/// way to get rid of it.
pub async fn logout<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    jar: CookieJar,
) -> Response {
    state.auth.revoke(&jar).await;
    let jar = clear_session(jar, &state.config.base_url);
    (jar, Redirect::to("/")).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_logout_clears_cookie_and_redirects() {
        let app = crate::test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/logout")
                    .header("cookie", "session=did:plc:test|sess123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let set_cookie = response
            .headers()
            .get("set-cookie")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");
        assert!(
            set_cookie.contains("Max-Age=0") || set_cookie.contains("session=;"),
            "expected the session cookie to be cleared, got: {set_cookie}"
        );
        assert!(
            set_cookie.contains("Path=/"),
            "expected Path=/ in Set-Cookie, got: {set_cookie}"
        );

        assert_eq!(response.headers().get("location").unwrap(), "/");
    }

    #[tokio::test]
    async fn test_logout_without_cookie() {
        let app = crate::test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/logout")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get("location").unwrap(), "/");
    }
}
