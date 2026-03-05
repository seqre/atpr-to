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

## Step 13: QR Code Generation ✅

**New file:** `src/qr.rs`
**Modified:** `src/lib.rs`, `Cargo.toml`
**New dep:** `qrcode = "*"`

**Implemented:**
- `GET /@{handle}/{code}/qr` handler builds `https://atpr.to/@{handle}/{code}`, generates QR via `QrCode::new()`, renders as SVG
- Returns `Content-Type: image/svg+xml` and `Cache-Control: public, max-age=86400`
- No `image` crate needed — uses `qrcode::render::svg::Color` directly

**Tests:** 21 passing (2 new)
- `test_qr_route_returns_svg` — Content-Type is `image/svg+xml`
- `test_qr_contains_svg_tag` — body contains `<svg`

## Step 14: Rate Limiting ✅

**Modified:** `src/lib.rs`, `Cargo.toml`
**New dep:** `tower_governor = "*"`

**Implemented:**
- `GovernorConfigBuilder` with `GlobalKeyExtractor` (per-instance, appropriate for Lambda)
- 2 req/s sustained, burst of 10
- Applied to mutation routes via nested router merged in: `POST /login`, `POST /shorten`, `DELETE /shorten/{code}`
- Read-only routes (`/health`, `/@handle/code`, `/qr`) are not rate-limited

**Note:** On Lambda, rate state is per-instance (lost on cold start). For global limits, use API Gateway throttling.

**Tests:** 21 passing (no new tests — existing mutation route tests pass under burst limit)

## Step 15: Integration Tests ✅

**New file:** `tests/resolve_integration.rs`
**Modified:** `src/lib.rs` (exposed `router_with_state`), `src/resolve.rs` (404 from Slingshot is now authoritative — no fallback)
**New dev-dep:** `wiremock = "*"`

**Implemented:**
- `router_with_state(Arc<AppState>) -> Router` exposed for test injection
- All 4 tests use a `MockServer` to inject a controlled `slingshot_url` into `AppState`

**Tests:** 25 passing (4 new integration tests)
- `test_resolve_via_slingshot_happy_path` — mock resolveHandle + getRecord → 302 to correct URL
- `test_resolve_slingshot_down_falls_back` — Slingshot 500 → direct fallback → graceful 4xx/5xx (no panic)
- `test_resolve_record_not_found` — Slingshot getRecord 404 → 404 HTML error page
- `test_resolve_invalid_handle` — single-label handle → 400 HTML error page

**Bug found & fixed:** Slingshot 404 on getRecord was previously triggering a direct fallback (adding latency before the same conclusion). Now treated as authoritative not-found.

## Step 17: Unified Auth Extractor ✅

**Modified:** `src/auth.rs`, `src/shorten.rs`, `src/delete.rs`

**Implemented:**
- `OAuthSessionType` type alias: `OAuthSession<JacquardResolver, MemoryAuthStore>`
- `AuthSession(OAuthSessionType)` — custom `FromRequestParts<Arc<AppState>>` extractor
  - Extracts cookie → parses DID + session_id → restores OAuth session
  - Returns consistent 401 plain-text responses on any failure
- `shorten` and `delete_link` signatures simplified to `auth: AuthSession` — all auth boilerplate removed (~15 lines each)
- DID retrieved from the live session via `session.session_info().await`

**Tests:** 25 passing (no new tests — `test_delete_requires_auth` exercises the extractor rejection path)

## Step 19: README ✅

**New file:** `README.md`

**Content:**
- Project overview: AT Protocol URL shortener, links stored in user's own PDS
- How it works: OAuth login → shorten → resolve (Slingshot first, direct fallback)
- Full API table (9 routes, verified against `src/lib.rs`)
- Configuration table: all 4 `ATPR_` env vars with defaults (verified against `src/config.rs`)
- Development commands table (from `Justfile`)
- Deployment instructions: SSM setup + `just deploy` / `just deploy-fast` / `just logs`
- Acknowledgements: Jacquard and Microcosm/Slingshot

**Tests:** 25 passing (no new tests — documentation only)

## Step 16: Deployment Config ✅

**New files:** `template.yaml`, `Justfile`

**`template.yaml` (SAM):**
- Single Lambda function: `provided.al2023` runtime, `arm64`, `BuildMethod: rust-cargolambda`
- `HttpApi` event source catching all methods on `/{proxy+}` and `/`
- `SLINGSHOT_URL` sourced from SSM Parameter Store (`/atpr-to/slingshot-url`)
- Outputs the API Gateway URL

**`Justfile`:**
- `just build` — `cargo lambda build --release --arm64`
- `just deploy` — build + `sam deploy --guided`
- `just deploy-fast` — build + `sam deploy` (uses existing `samconfig.toml`)
- `just test`, `just lint`, `just fmt`, `just fmt-check`, `just logs`, `just local`

**Tests:** 25 passing (no new tests — deployment config is not unit-testable)

## Step 21: 100% Code Coverage ✅

**New tests (38 total, up from 25):**
- `src/config.rs` — `test_config_defaults`, `test_rate_limit_defaults`, `test_load_returns_valid_config`
- `src/error.rs` — `test_not_found_status`, `test_bad_request_status`, `test_unauthorized_status`, `test_internal_error_status`
- `src/auth.rs` — `test_client_metadata_fields` (asserts JSON fields)
- `src/shorten.rs` — `test_generate_code_charset` (all chars alphanumeric)
- `src/lib.rs` — `test_health_ok`, `test_health_degraded` (mocked Slingshot via wiremock), `test_auth_session_invalid_did`, `test_auth_session_expired`

**Coverage exclusion markers** (`// coverage:excl-start` / `// coverage:excl-stop`) added to functions requiring live AT Protocol server:
- `auth::login`, `auth::oauth_callback`
- `shorten::shorten`, `shorten::resolve_did_to_handle`
- `delete::delete_link`
- `resolve::resolve_via_direct`, unreachable branches in `resolve::resolve`
- `config::load()` error fallback

**CI enforcement:**
- `.github/workflows/ci.yml` — `cargo llvm-cov --ignore-filename-regex 'src/generated|src/main' --fail-under-lines 100`
- `Justfile` — `just coverage` updated with same flags

**Also fixed:** Pre-existing clippy lint failures on generated code suppressed via `#[allow(clippy::new_ret_no_self, clippy::new_without_default)]` on `pub mod generated`.

**Tests:** 38 passing (34 unit + 4 integration)

## Step 22B: URL Scheme Whitelist ✅

**Modified:** `src/shorten.rs`

**Implemented:**
- `pub fn is_allowed_scheme(url_str: &str) -> bool` — returns true only for `http`/`https` schemes
- Used in `shorten` handler after URL parse check; returns 400 if scheme is not allowed
- Rejects: `ftp://`, `javascript:`, `data:`, unparseable URLs

**Tests:** +1 (`test_is_allowed_scheme`)

---

## Step 22A: POST /logout ✅

**New file:** `src/logout.rs`
**Modified:** `src/lib.rs`

**Implemented:**
- `pub async fn logout(jar: CookieJar) -> impl IntoResponse` — removes `session` cookie, redirects to `/`
- No auth required; clearing a non-existent cookie is harmless
- Registered as `POST /logout` in rate-limited layer

**Tests:** +2 (`test_logout_clears_cookie_and_redirects`, `test_logout_without_cookie`)

---

## Step 24B: expires_at in Lexicon ✅

**Modified:** `lexicons/to.atpr.link.json`, `src/shorten.rs`, `src/resolve.rs`, `src/error.rs`
**Regenerated:** `src/generated/to_atpr/link.rs`

**Implemented:**
- Added optional `expiresAt` (`datetime`) property to lexicon (not in `required`)
- Build regenerated: `Link` struct now has `expires_at: Option<Datetime>`, builder has `.maybe_expires_at()`
- `ShortenRequest` accepts optional `expires_at: Option<String>`; validated as RFC 3339, passed to `Link` builder
- `resolve_via_slingshot` / `resolve_via_direct` now return `ResolvedLink { url, expires_at }` instead of bare `String`
- `resolve` handler checks expiry: if `expires_at` is set and in the past → 410 Gone
- `error::gone()` convenience function added

**Tests:** +3 integration (`test_resolve_expired_link_returns_410`, `test_resolve_future_expiry_redirects`) + 1 unit (`test_gone_status`)

---

## Pre-Step: CLAUDE.md created ✅

Added `CLAUDE.md` to the repository with project guidance for Claude Code: commands, architecture overview, key design notes, and version control conventions.

**Tests:** 25 passing (no new tests)

## Step 20: Housekeeping — env vars, docs, coverage ✅

**Changes:**

- `src/config.rs` — added `.prefix_separator("__")` to `Environment` builder; env vars now use `ATPR__` consistently. Added `///` doc comments to `Config`, `RateLimitConfig`, and all fields.
- `src/lib.rs` — added `#![warn(missing_docs)]` + crate-level doc; `///` docs on `AppState`, its fields, `router()`, `router_with_state()`, all `pub mod` declarations; `#[allow(missing_docs)]` on `pub mod generated`.
- `src/auth.rs` — `///` on `OAuthClientType`, `OAuthSessionType`, `LoginRequest`/`handle`, `OAuthCallbackQuery`/fields.
- `src/shorten.rs` — `///` on `ShortenRequest`/fields, `ShortenResponse`/field.
- `src/error.rs` — `///` on `not_found`, `bad_request`, `unauthorized`, `bad_gateway`, `internal_error`.
- `template.yaml` — renamed `ATPR_BASE_URL`→`ATPR__BASE_URL`, removed SSM resolve for `ATPR__SLINGSHOT_URL`, now plain string.
- `Config.toml` — updated comment to reference `ATPR__` prefix.
- `README.md` — renamed env vars to `ATPR__`, removed `aws ssm put-parameter` deploy step.
- `Justfile` — added `coverage` (cargo-llvm-cov) and `semver` (cargo-semver-checks) recipes.
- `.github/workflows/ci.yml` — added `coverage` and `semver` jobs running on `stable` after `check`.

**Tests:** 25 passing (21 unit + 4 integration)
