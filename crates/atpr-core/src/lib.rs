//! Shared read path for atpr.to.
//!
//! Everything both servers need to turn `@handle/code` into a redirect:
//! configuration, domain types, the error type, the resolver chain, and a
//! ready-made public router ([`redirect::router_with_state`]).
//!
//! [`crate::redirect::ResolveState`] is deliberately free of authentication:
//! resolving a link is anonymous, so the standalone `atpr-redirect` binary can
//! serve this router with no OAuth, session store or PDS-write dependency at
//! all, while `atpr-server` embeds the same state inside its own
//! authenticator-carrying `AppState`.

pub mod config;
pub mod domain;
pub mod error;
/// Content negotiation for error responses.
pub mod error_page;
pub mod identity;
/// The public read side: resolving `@handle/code` to a destination.
pub mod redirect;
pub mod resolver;

/// Where the JSON API is mounted.
///
/// Named rather than spelled twice: anything that needs to know where JSON
/// territory begins — content negotiation, most obviously — has to agree with
/// the router, and two string literals that must match is exactly the pair
/// that drifts.
pub const API_PREFIX: &str = "/api";

/// Content Security Policy sent on every response.
///
/// No inline script or style, and the only third-party asset is the dashboard
/// avatar — `img-src https:` is what keeps that working. Tighten it to the
/// AppView's CDN once the rebrand settles on a stack; whatever that stack is,
/// it has to be same-origin or this needs revisiting, which is the point of
/// setting the header before the rebrand rather than after.
///
/// `form-action` allows `https:` because sign-in depends on it. `POST /login`
/// answers a browser form with a 303 to the user's *own* PDS authorize URL,
/// and Chromium enforces `form-action` against the redirect target, not just
/// the form's action — so `'self'` alone blocks the navigation and the login
/// button appears to do nothing. The PDS origin is per-user and unknowable
/// here, so an exact allowlist is not available. `https:` still refuses
/// `javascript:` and `data:` form targets, which is the attack this directive
/// is actually for.
/// `connect-src` enumerates the origins the vendored client JS in `static/`
/// talks to directly, and the two are edited together. It is deliberately not
/// derived from config: `appview_url` is the *server-side* avatar fetch, so
/// deriving from it would produce a header that looks configurable and is not,
/// while the endpoint the browser actually calls stayed hardcoded in the JS.
pub const CSP: &str = "default-src 'self'; \
     img-src 'self' https: data:; \
     style-src 'self'; \
     script-src 'self'; \
     font-src 'self'; \
     connect-src 'self' https://public.api.bsky.app https://plc.directory \
       wss://jetstream1.us-east.bsky.network wss://jetstream2.us-east.bsky.network; \
     form-action 'self' https:; \
     frame-ancestors 'none'; \
     base-uri 'none'";

#[cfg(test)]
mod tests {
    use super::CSP;
    /// The policy is a constant, so what it permits is worth asserting
    /// directly rather than only through whatever a handler happens to do.
    ///
    /// The `unsafe-*` assertions are the load-bearing ones: the client side is
    /// vanilla JS with every stylesheet and script same-origin, so nothing
    /// here needs to evaluate a string or parse an inline style. Both are easy
    /// to add under deadline and effectively impossible to remove afterwards,
    /// so a future loosening should fail a test that says why.
    #[test]
    fn test_csp_permits_what_the_app_needs_and_nothing_looser() {
        assert!(CSP.contains("default-src 'self'"));

        // Sign-in 303s to the user's own PDS, and Chromium checks
        // `form-action` against redirect targets.
        assert!(
            CSP.contains("form-action 'self' https:"),
            "cross-origin sign-in redirect must be permitted"
        );

        // Every origin the vendored client JS in static/ calls directly. The
        // header and that JS are edited together, so a new endpoint that
        // nobody allowed here shows up as a failing test rather than as a
        // console error in production.
        for origin in [
            "https://public.api.bsky.app", // handle autocomplete
            "https://plc.directory",       // DID -> handle on the live wall
            "wss://jetstream1.us-east.bsky.network",
            "wss://jetstream2.us-east.bsky.network",
        ] {
            assert!(
                CSP.contains(origin),
                "the client JS calls {origin} and the policy must allow it"
            );
        }

        assert!(!CSP.contains("unsafe-eval"), "the client JS is vanilla");
        assert!(
            !CSP.contains("unsafe-inline"),
            "all CSS and JS is same-origin and external"
        );
    }
}
