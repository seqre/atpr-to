use axum::response::{IntoResponse, Redirect};
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;

/// Clear the session cookie and redirect to the home page.
///
/// No authentication required — clearing a non-existent cookie is harmless.
pub async fn logout(jar: CookieJar) -> impl IntoResponse {
    let jar = jar.remove(Cookie::from("session"));
    (jar, Redirect::temporary("/"))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;

    #[tokio::test]
    async fn test_logout_clears_cookie_and_redirects() {
        let app = router();
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

        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);

        let set_cookie = response
            .headers()
            .get("set-cookie")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");
        // The session cookie should be removed (empty value or max-age=0)
        assert!(
            set_cookie.contains("session=")
                && (set_cookie.contains("Max-Age=0")
                    || set_cookie.is_empty()
                    || set_cookie.contains("session=;")),
            "expected session cookie to be cleared, got: {set_cookie}"
        );

        let location = response.headers().get("location").unwrap();
        assert_eq!(location, "/");
    }

    #[tokio::test]
    async fn test_logout_without_cookie() {
        let app = router();
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

        // Should succeed even without a session cookie
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        let location = response.headers().get("location").unwrap();
        assert_eq!(location, "/");
    }
}
