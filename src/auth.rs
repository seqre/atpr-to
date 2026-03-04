use axum::Json;

/// Serve OAuth client metadata for atproto OAuth discovery.
pub async fn client_metadata() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "client_id": "https://atpr.to/.well-known/oauth-client-metadata.json",
        "client_name": "atpr.to URL Shortener",
        "client_uri": "https://atpr.to",
        "redirect_uris": ["https://atpr.to/oauth/callback"],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "scope": "atproto transition:generic",
        "application_type": "web",
        "dpop_bound_access_tokens": true
    }))
}

/// Handle OAuth callback after user authorizes.
pub async fn oauth_callback() -> &'static str {
    // TODO: exchange auth code for tokens, create session cookie
    "OAuth callback — not yet implemented"
}
