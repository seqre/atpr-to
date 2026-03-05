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

## Step 4: URL Shortening ✅

**Implemented:**
- `src/shorten.rs` — full POST /shorten handler:
  - Authenticates via session cookie → restores OAuth session
  - Accepts `{ url, code? }` — generates random 6-8 char code if none given
  - Validates code format (alphanumeric + `-_`, 1-64 chars) and URL (valid, ≤2048 chars)
  - Builds `Link` record with `url` and `createdAt`
  - Writes to PDS via `com.atproto.repo.putRecord` using `XrpcClient::send()`
  - Returns `{ short_url: "https://atpr.to/@{did}/{code}" }`

**Decisions:**
- Uses `XrpcClient::send()` directly (not `AgentSessionExt`) — `OAuthSession` doesn't impl `AgentSession`
- `PutRecord` builder needs `.collection(NSID.to_string())` (Nsid doesn't impl From<&str>)
- Short URL uses DID for now; handle resolution for display URL is a future improvement
- `RecordKey::any()` validates the code as a valid atproto record key

**Tests:** 12 passing (3 new)
- Random code generation: length 6-8, always valid
- Code validation: accepts valid codes, rejects empty/too-long/special chars

## Step 5: URL Resolution ✅

**Implemented:**
- `src/resolve.rs` — full GET /@{handle}/{code} handler:
  1. Parse handle via `Handle::new()`
  2. Resolve handle → DID via `JacquardResolver::resolve_handle()`
  3. Resolve DID → DID document via `resolve_did_doc()`, extract `pds_endpoint()`
  4. Fetch record from PDS via unauthenticated HTTP GET to `com.atproto.repo.getRecord`
  5. Extract `url` field from record value
  6. Return HTTP 302 redirect to target URL

**Decisions:**
- Unauthenticated resolution: uses plain HTTP GET to PDS (no OAuth needed for public records)
- Uses `JacquardResolver::default()` for identity resolution (DNS + HTTP + PLC directory)
- Parses getRecord response as `serde_json::Value` to extract URL (simpler than typed deserialization)
- URL-encodes DID and rkey in query parameters for safety

**Dependencies added:** `reqwest` (json feature), `urlencoding`

**Tests:** 13 passing (1 new)
- Handle parsing validation (valid handles accepted, single-label rejected)

## Step 6: Error Pages ✅

**New file:** `src/error.rs`
**Modified:** `src/lib.rs`, `src/resolve.rs`, `src/auth.rs`

**Implemented:**
- `error_page(status, title, message)` — returns styled HTML response with status code, title, message, and back-link
- Convenience functions: `not_found()`, `bad_request()`, `unauthorized()`, `bad_gateway()`, `internal_error()`
- Applied to all browser-facing routes (`resolve`, `login`, `oauth_callback`)
- API routes (`shorten`) keep plain text/JSON errors

**Tests:** 15 passing (2 new)
- `test_error_page_html` — output contains `<!DOCTYPE html>`, correct title and message
- `test_error_page_status` — bad_gateway returns 502

## Step 7: Structured Logging ✅

**New dep:** `tracing = "0.1"`
**Modified:** `src/resolve.rs`, `src/shorten.rs`, `src/auth.rs`, `Cargo.toml`

**Implemented:**
- `#[tracing::instrument(skip_all)]` on `login` and `shorten` handlers
- `#[tracing::instrument]` on `resolve` handler
- `tracing::info_span!` around each resolution step: `resolve_handle`, `resolve_did_doc`, `fetch_record`
- `tracing::info!` logs elapsed ms after successful resolution

**Tests:** 15 passing (no new tests — logging is infrastructure)

## Step 8: CI Pipeline ✅

**New file:** `.github/workflows/ci.yml`

**Implemented:**
- Matrix build across `stable` and `nightly` toolchains
- `continue-on-error: true` for nightly (informational only)
- Steps: checkout → install Rust → cache deps (Swatinem/rust-cache) → install cargo-nextest → fmt check → clippy (-D warnings) → nextest run
- Triggers on push/PR to `main`

## Step 9: Slingshot Integration ✅

**Modified:** `src/resolve.rs`, `src/lib.rs`, `Cargo.toml`
**New dep:** `anyhow = "1"`

**Implemented:**
- `AppState` now holds `http: reqwest::Client` (shared, connection-pooled) and `slingshot_url: String`
- `slingshot_url` from `SLINGSHOT_URL` env var, default `https://slingshot.microcosm.blue/`
- `resolve_via_slingshot(client, slingshot_url, handle, code)` — 2-hop: resolveHandle → getRecord
- `resolve_via_direct(client, handle, code)` — original 3-hop: handle → DID → DID doc → PDS getRecord
- `resolve` handler tries Slingshot first; on any error, logs warning and falls back to direct
- Handler now takes `State(state): State<Arc<AppState>>` and uses shared `state.http` client
- Tracing spans preserved on both paths

**Tests:** 16 passing (1 new)
- `test_slingshot_url_construction` — verifies special chars in DIDs/handles are percent-encoded

## Step 10: Health Check ✅

**Modified:** `src/lib.rs`

**Implemented:**
- `GET /health` handler uses `state.http` and `state.slingshot_url` from `AppState`
- Pings `{slingshot_url}/xrpc/com.atproto.identity.resolveHandle?handle=atpr.to`
- Returns 200 `{"status": "ok", "slingshot": "ok"}` if Slingshot responds successfully
- Returns 200 `{"status": "degraded", "slingshot": "unreachable"}` if Slingshot is down (service still works via fallback)

**Tests:** 17 passing (1 new)
- `test_health_route` — GET /health returns 200

## Step 11: Handle Display in Shorten Response ✅

**Modified:** `src/shorten.rs`

**Implemented:**
- `resolve_did_to_handle(client, slingshot_url, did_str)` — tries Slingshot `describeRepo?repo=DID` first (1 hop, uses shared `state.http`); falls back to direct DID doc resolution via `JacquardResolver` if Slingshot fails
- After successful `putRecord`, calls `resolve_did_to_handle` best-effort; falls back to DID string on any error
- Short URL is now `https://atpr.to/@{handle}/{code}` instead of `https://atpr.to/@{did}/{code}`

**Tests:** 17 passing (no new tests — no API shape change, fallback behaviour covered by existing tests)

## Step 12: Delete Link ✅

**New file:** `src/delete.rs`
**Modified:** `src/lib.rs`, `src/shorten.rs` (`validate_code` made `pub`)

**Implemented:**
- `DELETE /shorten/{code}` handler: auth check → validate code → restore session → `DeleteRecord` request → 204 No Content
- Uses same builder pattern as `PutRecord`: `.repo()`, `.collection()`, `.rkey()`, `.build()`
- Auth errors return 401, invalid codes return 400

**Tests:** 19 passing (2 new)
- `test_delete_requires_auth` — DELETE without cookie → 401
- `test_delete_method` — GET /shorten/abc → 405 Method Not Allowed
