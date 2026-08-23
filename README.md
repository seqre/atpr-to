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
| `ATPR__BIND_ADDR` | `127.0.0.1:8080` | Socket the standalone redirect server binds *(redirect server only)* |

Nested keys use `__` as separator. A `Config.toml` in the working directory is loaded if present.

`ATPR__REDIRECT_CACHE_MAX_AGE` is a real trade-off, not a tuning detail: edits and deletes take up to that
long to reach anyone who already followed the link. Errors are always `no-store`, so a 404 is never cached
past the moment someone creates the code.

---

## Self-hosting a redirect server

The repo builds a second binary, `atpr-redirect`, that serves **only** the public read path:

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/@{handle}/{code}` | Resolve and redirect (307, with `Cache-Control: public, max-age=…`) |
| `GET` | `/health` | Health check — pings Slingshot; **503** when degraded |

It has no OAuth, no session store and no PDS-write dependency: it reads `to.atpr.link` records off the AT
Protocol network exactly as atpr.to does (Slingshot first, direct PDS resolution as fallback), so links
created on atpr.to resolve identically on your instance — and vice versa. Point your own short links at
your domain by setting `ATPR__BASE_URL` when *creating* them; existing atpr.to links keep resolving either
way because the records live in their owners' repos, not in this server.

Run it with:

```sh
cargo run -p atpr-redirect        # or: just run-redirect
```

Configuration is the same chain as the main server (defaults → `Config.toml` → `ATPR__*`). The keys that
matter here are `ATPR__SLINGSHOT_URL`, `ATPR__REDIRECT_CACHE_MAX_AGE`, the timeout budgets, the rate-limit
pair, and `ATPR__BIND_ADDR`. Auth-related keys (`session_file`, `appview_url`) are accepted but unused.

It speaks plain HTTP and binds loopback by default. Put a reverse proxy in front of it that terminates TLS,
and **make sure the proxy overwrites — not appends to — `X-Forwarded-For`**: the rate limiter keys on that
header, and an appending proxy lets any caller spoof their way past the per-IP limit. A minimal nginx site:

```nginx
server {
    listen 443 ssl;
    server_name go.example.com;
    # ssl_certificate …

    location / {
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_pass http://127.0.0.1:8080;
    }
}
```

A systemd unit is equally small (`ExecStart=/usr/local/bin/atpr-redirect`, `Environment=ATPR__BIND_ADDR=127.0.0.1:8080`,
`DynamicUser=yes`, `NoNewPrivileges=yes`).

---

## Development

**Prerequisites:** Rust stable, [cargo-lambda](https://www.cargo-lambda.info/),
[cargo-nextest](https://nexte.st/), [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).
A `flake.nix` providing all of these is available but not tracked in this repository.

| Command | Description |
|---------|-------------|
| `just run` | Run the main server as a plain HTTP server on `127.0.0.1:9000` — no cargo-lambda needed |
| `just run-redirect` | Run the standalone redirect server on `127.0.0.1:8080` |
| `just local` | Run via the Lambda runtime emulator |
| `just test` | Run tests |
| `just lint` | Run Clippy with `-D warnings` |
| `just fmt` / `just fmt-check` | Format / check formatting |
| `just coverage` | HTML coverage report, gated at 80% lines |
| `just codegen` | Regenerate `crates/atpr-server/src/generated/` from `lexicons/` |
| `just deny` | Audit dependencies and licences |
| `just build` | Build release binary for Lambda (arm64) |

The code is a Cargo workspace: [`crates/atpr-core`](crates/atpr-core) holds the read path shared by both
servers (config, domain types, resolver chain, error pages, the public router), [`crates/atpr-server`](crates/atpr-server)
holds everything authenticated, and [`crates/atpr-redirect`](crates/atpr-redirect) is the standalone binary
built on top of the core router.

`crates/atpr-server/src/generated/` is checked into git. Edit `lexicons/`, run `just codegen`, and commit
the result — CI fails if the two disagree.

---

## Deployment

See [DEPLOYMENT.md](DEPLOYMENT.md).

---

## Acknowledgements

Thanks to [**Jacquard**](https://github.com/fatfingers23/jacquard) for the AT Protocol OAuth and XRPC client library, and to [**Microcosm**](https://microcosm.blue) for running [Slingshot](https://github.com/microcosm-blue/slingshot), the AT Protocol relay that powers fast link resolution.
