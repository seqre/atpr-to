# atpr.to

An AT Protocol URL shortener. Short URLs are stored as records in the user's own PDS and take the form:

```
https://atpr.to/@alice.bsky.social/abc123
```

Anyone with a Bluesky (or any atproto) account can create short links — no central database, your links live in your repo.

---

## How it works

1. **Login** — OAuth via AT Protocol. The session cookie holds `{did}|{session_id}` and is `HttpOnly`,
   `SameSite=Lax`, and `Secure` with the `__Host-` prefix outside local development. It is **not signed** —
   the DID half is client-supplied, so it is treated as an untrusted lookup key and every request revalidates
   it against the server-side session store. Nothing is authorised on the cookie's say-so.
2. **Shorten** — `POST /api/shorten` writes a `to.atpr.link` record to your PDS via
   `com.atproto.repo.putRecord`. Only `http`/`https` URLs up to 2048 characters are accepted. Re-using an
   existing code returns **409 Conflict** rather than overwriting it.
   `PUT /api/shorten/{code}` repoints an existing link instead, keeping the code. The asymmetry is
   deliberate: `POST` refuses to overwrite so a duplicate code cannot quietly destroy a link someone
   already has, while arriving at `PUT` is a statement that replacing it is the point.
3. **Resolve** — `GET /@handle/code` looks up the record and redirects. Resolution tries
   [Slingshot](https://github.com/microcosm-blue/slingshot) first, falling back to direct PDS resolution.
   The destination's scheme is validated on read as well as on write, so a record written outside this app
   cannot smuggle a `javascript:` URL into a redirect or a preview page.
4. **UI** — `GET /` serves a login form; `GET /dashboard` shows your links.

---

## API

Browser-facing routes are at the root; the JSON API is under `/api`.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Home page — login form (redirects to `/dashboard` if logged in) |
| `GET` | `/dashboard` | Dashboard (auth required) |
| `GET` | `/oauth-client-metadata.json` | OAuth client metadata |
| `GET` | `/oauth/callback` | OAuth callback |
| `GET` | `/@{handle}/{code}` | Resolve and redirect |
| `GET` | `/@{handle}/{code}/info` | Preview page — destination, last modified, QR code |
| `GET` | `/@{handle}/{code}/qr` | QR code as SVG |
| `GET` | `/static/{*path}` | Embedded static assets |
| `POST` | `/api/login` | Start OAuth flow |
| `POST` | `/api/logout` | Revoke the session server-side, clear the cookie, redirect to `/` |
| `GET` | `/api/links` | List your short links — `?limit=&cursor=` (auth required) |
| `POST` | `/api/shorten` | Create short URL — `{ url, code? }`; **409** if the code is taken (auth required) |
| `PUT` | `/api/shorten/{code}` | Repoint an existing short URL — `{ url }` → **204** (auth required) |
| `DELETE` | `/api/shorten/{code}` | Delete short URL (auth required) |
| `GET` | `/api/health` | Health check — pings Slingshot; **503** when degraded |

Errors are JSON on every route: `{"error": "..."}`. Bodies are opaque for upstream and internal failures —
detail goes to the logs, not to the client.

---

## Configuration

Loading priority (last wins): compiled defaults → `Config.toml` → `ATPR__` environment variables.
Loading is fail-fast: an unparseable value or an unknown key aborts startup rather than falling back to
defaults that would point your short links at someone else's domain.

| Env var | Default | Description |
|---------|---------|-------------|
| `ATPR__BASE_URL` | `https://atpr.to` | Base URL for short links and OAuth metadata |
| `ATPR__SLINGSHOT_URL` | `https://slingshot.microcosm.blue/` | Slingshot instance for fast resolution |
| `ATPR__APPVIEW_URL` | `https://public.api.bsky.app` | AppView used for the dashboard avatar |
| `ATPR__SESSION_FILE` | *(empty — in memory)* | Path to the session store; empty means memory-only |
| `ATPR__RATE_LIMIT__PER_SECOND` | `2` | Sustained request rate on mutation routes |
| `ATPR__RATE_LIMIT__BURST_SIZE` | `10` | Burst allowance on mutation routes |
| `ATPR__STATIC_CACHE_MAX_AGE` | `15` | `Cache-Control: max-age` on static assets |
| `ATPR__REDIRECT_CACHE_MAX_AGE` | `60` | `Cache-Control: max-age` on successful redirects; `0` disables |
| `ATPR__REQUEST_TIMEOUT_MS` | `25000` | Server-side budget for one inbound request |
| `ATPR__HTTP_TIMEOUT_MS` | `5000` | Total budget for one outbound request |
| `ATPR__HTTP_CONNECT_TIMEOUT_MS` | `2000` | Budget for establishing an outbound connection |

Nested keys use `__` as separator. A `Config.toml` in the working directory is loaded if present.

`ATPR__REDIRECT_CACHE_MAX_AGE` is a real trade-off, not a tuning detail: edits and deletes take up to that
long to reach anyone who already followed the link. Errors are always `no-store`, so a 404 is never cached
past the moment someone creates the code.

---

## Development

**Prerequisites:** Rust stable, [cargo-lambda](https://www.cargo-lambda.info/),
[cargo-nextest](https://nexte.st/), [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).
A `flake.nix` providing all of these is available but not tracked in this repository.

| Command | Description |
|---------|-------------|
| `just run` | Run as a plain HTTP server on `127.0.0.1:9000` — no cargo-lambda needed |
| `just local` | Run via the Lambda runtime emulator |
| `just test` | Run tests |
| `just lint` | Run Clippy with `-D warnings` |
| `just fmt` / `just fmt-check` | Format / check formatting |
| `just coverage` | HTML coverage report, gated at 80% lines |
| `just codegen` | Regenerate `src/generated/` from `lexicons/` |
| `just deny` | Audit dependencies and licences |
| `just build` | Build release binary for Lambda (arm64) |

`src/generated/` is checked into git. Edit `lexicons/`, run `just codegen`, and commit the result — CI fails
if the two disagree.

---

## Deployment

**Prerequisites:** AWS SAM CLI, `cargo-lambda`, ARM64 cross-compilation target.

```sh
just deploy        # guided (first time)
just deploy-fast   # subsequent deploys
just logs          # tail Lambda logs
```

The SAM template deploys a single `provided.al2023` Lambda function on arm64 behind an HTTP API Gateway,
with throttling set at the gateway — the in-process rate limiter is per execution environment and cannot
bound anything globally.

A custom domain is optional. Pass `DomainName` **and** a regional `CertificateArn` to have the stack create
the domain and its mapping; omit both and it serves from the `execute-api` URL in the stack outputs.

> **Known issue:** OAuth sessions are stored on the Lambda instance's `/tmp`, which is per execution
> environment. Login state written while handling the redirect can be missing when the callback lands on a
> different instance, so **logins fail intermittently under concurrency**. Reserved concurrency of 1 avoids
> it at the cost of throughput; a shared session backend is the real fix.

---

## Acknowledgements

Thanks to [**Jacquard**](https://github.com/fatfingers23/jacquard) for the AT Protocol OAuth and XRPC client library, and to [**Microcosm**](https://microcosm.blue) for running [Slingshot](https://github.com/microcosm-blue/slingshot), the AT Protocol relay that powers fast link resolution.
