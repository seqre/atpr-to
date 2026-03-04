# atpr.to Progress

## Step 1: Lexicon & Generated Types ✅

**Implemented:**
- `lexicons/to.atpr.link.json` — lexicon definition for shortened URL records
- `build.rs` — uses `jacquard-lexicon` (corpus + codegen) to generate Rust types from lexicons
  - Runs `cargo:rerun-if-changed=lexicons/` so regeneration only happens when schemas change
  - Post-processes output: renames `lib.rs` → `mod.rs`, strips feature gates, fixes `crate::builder_types` paths
- `src/generated/` — auto-generated types: `Link<'a>` struct with builder, serde, collection impl
- Updated `Cargo.toml` with `jacquard-lexicon` as build-dependency (codegen feature)
- Added `jacquard-derive`, `rustversion`, `chrono` dependencies
- Removed old exploratory `http_handler.rs`

**Decisions:**
- Used `jacquard-lexicon` directly (not a separate `jacquard-lexgen` crate) — the codegen lives in `jacquard-lexicon` behind the `codegen` feature
- Generated code uses `jacquard_common::types::string::Uri` and `Datetime` wrappers (not plain strings)
- `Uri` normalizes URLs (adds trailing slash to bare domains)
- Builder pattern requires `Uri::new()` (returns Result) and `chrono::DateTime` for `created_at`

**Tests:** 3 passing
- Serde roundtrip (JSON → Link → JSON → Link)
- Builder API (construct Link with builder)
- Collection NSID constant = "to.atpr.link"

## Step 2: Project Skeleton ✅

**Implemented:**
- `src/lib.rs` — library crate with `router()` function, all routes wired up
- `src/main.rs` — thin Lambda entry point, calls `atpr_to::router()`
- `src/auth.rs` — OAuth client metadata endpoint (serves JSON), callback stub
- `src/shorten.rs` — POST /shorten stub (returns 401, needs auth)
- `src/resolve.rs` — GET /@{handle}/{code} stub (returns 502, needs resolution logic)

**Routes:**
- `GET /` → landing page
- `GET /.well-known/oauth-client-metadata.json` → OAuth client metadata
- `GET /oauth/callback` → OAuth callback (stub)
- `POST /shorten` → create short URL (stub, needs auth)
- `GET /@{handle}/{code}` → resolve & redirect (stub)

**Dependencies added:** `axum-extra` (cookies), `rand`, `tower` (test utility)

**Tests:** 4 passing
- Index route returns 200
- Shorten rejects GET (405 Method Not Allowed)
- Resolve route exists (not 404)
- OAuth metadata returns 200

## Step 3: OAuth ✅

**Implemented:**
- `src/auth.rs` — full OAuth module:
  - `build_oauth_client()` — constructs `OAuthClient<JacquardResolver, MemoryAuthStore>` with atpr.to metadata
  - `client_metadata()` — serves `/.well-known/oauth-client-metadata.json`
  - `login()` — POST /login: accepts handle, calls `start_auth()`, redirects to auth URL
  - `oauth_callback()` — GET /oauth/callback: exchanges code for session, stores DID|session_id in cookie
  - `parse_session_cookie()` — extracts (DID, session_id) from cookie for session restoration
- `src/lib.rs` — `AppState` struct holding `OAuthClientType`, shared via `Arc<AppState>`
- Added `/login` POST route

**Decisions:**
- `keyset: None` — jacquard auto-generates ES256 keypair (sufficient for `token_endpoint_auth_method: "none"`)
- Cookie format: `did:plc:xyz|session_id` (pipe separator since DIDs contain colons)
- `MemoryAuthStore` for now (sessions lost on restart, but fine for Lambda cold starts during dev)
- Session restoration via `oauth.restore(did, session_id)` — will be used in shorten handler

**Dependencies added:** `time`, `url`

**Tests:** 9 passing (4 new)
- Session cookie parsing (valid, missing, malformed)
- OAuth client construction doesn't panic
- Login route requires POST
