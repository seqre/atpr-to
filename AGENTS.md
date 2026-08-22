# AGENTS.md

Guidance for coding agents working on this repository. See also `README.md` for API/config reference
and `DEPLOYMENT.md` for the deploy flow.

## What this is

Single-binary Rust/Axum URL shortener for AT Protocol, deployed as one AWS Lambda function
(`provided.al2023`, arm64) behind HTTP API Gateway. `src/main.rs` picks its runtime from
`AWS_LAMBDA_FUNCTION_NAME`: Lambda runtime when set, plain `axum::serve` otherwise — so
`just run` needs no flags.

## Commands

Verification order for changes: `cargo fmt --all` → `just lint` → `cargo nextest run`.
Coverage gate runs in CI; run `just coverage` if you touched much code.

## Generated code

`src/generated/` holds lexicon types generated from `lexicons/` by `src/bin/codegen.rs`
(feature-gated as the `codegen` binary). **Checked into git; never edit by hand.** After editing
`lexicons/`, run `just codegen` and commit the result — a dedicated CI job regenerates and fails
if they disagree.

## Architecture rules

- Dependency direction: `api` → `domain`/`store`/`resolver`; `domain` depends on nothing. The
  `jacquard` AT Protocol crate must stay confined to `store.rs`, `session/`, `resolver/direct.rs`,
  and `auth.rs` — do not let it leak into handlers.
- `AppState<A>` is generic over the authenticator; that generic parameter *is* the test seam:
  production wires `OAuthAuthenticator`, tests wire `FakeAuthenticator` + `InMemoryLinkStore`.
  Keep new functionality testable through it rather than adding mocks elsewhere.
- Status codes are decided only in `error.rs` (`AppError::IntoResponse`). Handlers return typed errors.
- `AppError::BadRequest` deliberately takes `&'static str` — no interpolating upstream text into
  client-visible bodies.
- Traits use AFIT with explicit `-> impl Future<...> + Send`. No `#[async_trait]`, no `Box<dyn>`.
  Sole exception: the `session::AuthStore` enum (jacquard's `ClientAuthStore` isn't object-safe).
- `missing_docs = "warn"` via `[lints.rust]` in `Cargo.toml` — every new public item needs `///` docs,
  and `-D warnings` in CI turns that into an error.

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
- Tests are hermetic: `tests/` uses `wiremock`, and `test_router()` aims Slingshot at an unreachable
  address so nothing silently depends on real upstreams. Don't add tests that hit the network.
- `#[tracing::instrument]` args need `Debug`; prefer `skip_all` on request types, and record any field
  you declare or it logs nothing.
- Rate limiting is per-Lambda-instance; global throttling lives at API Gateway in `template.yaml`.
- Release profile intentionally keeps unwinding (no `panic = "abort"`): `lambda_runtime` catches
  panics per invocation; aborting would force cold starts.

## Deployment

`just deploy` (guided first time) / `just deploy-fast` via SAM. Custom domain requires passing both
`DomainName` and regional `CertificateArn`; otherwise the stack serves from the `execute-api` URL.
