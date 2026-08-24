# AGENTS.md

Guidance for coding agents working on this repository. See also `README.md` for API/config reference
and `DEPLOYMENT.md` for the deploy flow.

## What this is

Rust/Axum URL shortener for AT Protocol, deployed as one AWS Lambda function (`provided.al2023`, arm64)
behind HTTP API Gateway. A Cargo workspace with three members:

- `crates/atpr-core` — the read path both servers share: config, domain types, error, the
  Slingshot/Direct resolver chain, the HTML error-page middleware, and `redirect::router_with_state`
  (the public `/@{handle}/{code}` + `/health` router with its middleware stack).
- `crates/atpr-server` — everything authenticated: OAuth, session stores, PDS link store, dashboard,
  static assets, generated lexicon types. Lambda entry point; `main.rs` picks its runtime from
  `AWS_LAMBDA_FUNCTION_NAME`: Lambda runtime when set, plain `axum::serve` otherwise — so
  `just run` needs no flags.
- `crates/atpr-redirect` — standalone self-hostable binary serving only the core public router via
  plain `axum::serve`; binds `ATPR__BIND_ADDR` (default loopback) for use behind a TLS-terminating proxy.

## Commands

Verification order for changes: `cargo fmt --all` → `just lint` → `cargo nextest run`.
Coverage gate runs in CI; run `just coverage` if you touched much code.

## Generated code

`crates/atpr-server/src/generated/` holds lexicon types generated from `lexicons/` by
`crates/atpr-server/src/bin/codegen.rs` (feature-gated as the `codegen` binary). **Checked into git;
never edit by hand.** After editing `lexicons/`, run `just codegen` and commit the result — a dedicated
CI job regenerates and fails if they disagree.

## Architecture rules

- Dependency direction: `api` → `domain`/`store`/`resolver`; `domain` depends on nothing.
  `atpr-server` depends on `atpr-core`, never the reverse. The `jacquard` AT Protocol crate must stay
  confined to `atpr-core/src/identity.rs` + `resolver/` (identity resolution only), and in the server to
  `store.rs`, `session/`, and `auth.rs` — do not let it leak into handlers.
- The public read path is generic-free: `redirect::ResolveState` (links, http, config) carries
  everything resolution needs, and `atpr-server`'s `AppState<A>` nests it as `Arc<ResolveState>` with
  one-line adapters delegating the shared handlers. Do not reintroduce authentication into that state.
- `AppState<A>` is generic over the authenticator; that generic parameter *is* the test seam:
  production wires `OAuthAuthenticator`, tests wire `FakeAuthenticator` + `InMemoryLinkStore`.
  Keep new functionality testable through it rather than adding mocks elsewhere.
- Status codes are decided only in `error.rs` (`AppError::IntoResponse`). Handlers return typed errors.
- `AppError::BadRequest` deliberately takes `&'static str` — no interpolating upstream text into
  client-visible bodies.
- Traits use AFIT with explicit `-> impl Future<...> + Send`. No `#[async_trait]`, no `Box<dyn>`.
  Sole exception: the `session::AuthStore` enum (jacquard's `ClientAuthStore` isn't object-safe).
- Middleware for the public routes (timeout→504, security headers/HSTS, error-page negotiation,
  per-IP rate limit) lives in `atpr-core::redirect` as shared helpers; build routers from those rather
  than restating layer configuration — two copies of a header policy *will* drift.
- `missing_docs = "warn"` via `[workspace.lints.rust]` in the root `Cargo.toml` — every new public item
  needs `///` docs, and `-D warnings` in CI turns that into an error.

## Conventions and gotchas

- Make invalid states unrepresentable (`ShortCode`, `TargetUrl`, `NonZeroU64` limits). Prefer deleting
  a bug's preconditions over catching it.
- Config loading order: compiled defaults → `Config.toml` → `ATPR__` env vars (`__` nests).
  Invalid config aborts startup; there is no fallback.
- Session cookie: named `__Host-session` in production, `session` on loopback (the `__Host-` prefix
  forces `Secure`). It is unsigned and treated purely as an untrusted lookup key — revalidate against
  the session store every request.
- Sessions persist via `ATPR__SESSION_FILE`: `""` = memory, `dynamodb://{table}` = DynamoDB,
  anything else = file path. Deployed stacks use DynamoDB; per-instance storage breaks OAuth callbacks.
- Tests are hermetic: integration tests use `wiremock`, and the test harnesses aim Slingshot at an
  unreachable address (server) or mount a mock (core) so nothing silently depends on real upstreams.
  Don't add tests that hit the network. Resolution/redirect tests live in `atpr-core/tests/`;
  authenticated and page-rendering tests in `crates/atpr-server/tests/`.
- `#[tracing::instrument]` args need `Debug`; prefer `skip_all` on request types, and record any field
  you declare or it logs nothing.
- Rate limiting is per-Lambda-instance; global throttling lives at API Gateway in `template.yaml`.
- Release profile intentionally keeps unwinding (no `panic = "abort"`): `lambda_runtime` catches
  panics per invocation; aborting would force cold starts.

## Deployment

`just deploy` (guided first time) / `just deploy-fast` via SAM. Custom domain requires passing both
`DomainName` and regional `CertificateArn`; otherwise the stack serves from the `execute-api` URL.
