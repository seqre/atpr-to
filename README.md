# atpr.to

An AT Protocol URL shortener. Short URLs are stored as records in the user's own PDS and take the form:

```
https://atpr.to/@alice.bsky.social/abc123
```

Anyone with a Bluesky (or any atproto) account can create short links — no central database, your links live in your repo.

---

## How it works

1. **Login** — OAuth via AT Protocol. Your session lives in a signed cookie.
2. **Shorten** — `POST /shorten` writes a `to.atpr.link` record to your PDS via `com.atproto.repo.putRecord`. Only `http`/`https` URLs are accepted; an optional `expires_at` (ISO 8601) can be set.
3. **Resolve** — `GET /@handle/code` looks up the record and redirects. Resolution tries [Slingshot](https://github.com/microcosm-blue/slingshot) first, falling back to direct PDS resolution if unavailable. Returns **410 Gone** if the link has an `expires_at` in the past.
4. **UI** — `GET /` serves a login form; `GET /dashboard` shows your links with a shorten form and delete buttons.

---

## API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Home page — login form (redirects to `/dashboard` if logged in) |
| `GET` | `/dashboard` | Dashboard — link list, shorten form, logout (auth required) |
| `GET` | `/.well-known/oauth-client-metadata.json` | OAuth client metadata |
| `POST` | `/login` | Start OAuth flow |
| `POST` | `/logout` | Clear session cookie and redirect to `/` |
| `GET` | `/oauth/callback` | OAuth callback |
| `GET` | `/links` | List authenticated user's short links as JSON (auth required) |
| `POST` | `/shorten` | Create short URL — `{ url, code?, expires_at? }` (auth required) |
| `DELETE` | `/shorten/{code}` | Delete short URL (auth required) |
| `GET` | `/@{handle}/{code}` | Resolve and redirect (410 if expired) |
| `GET` | `/@{handle}/{code}/info` | Preview page — destination, creation date, QR code |
| `GET` | `/@{handle}/{code}/qr` | QR code as SVG |
| `GET` | `/health` | Health check — pings Slingshot |

---

## Configuration

Loading priority (last wins): compiled defaults → `Config.toml` → `ATPR__` environment variables.

| Env var | Default | Description |
|---------|---------|-------------|
| `ATPR__BASE_URL` | `https://atpr.to` | Base URL for short links and OAuth metadata |
| `ATPR__SLINGSHOT_URL` | `https://slingshot.microcosm.blue/` | Slingshot instance for fast resolution |
| `ATPR__RATE_LIMIT__PER_SECOND` | `2` | Sustained request rate on mutation routes |
| `ATPR__RATE_LIMIT__BURST_SIZE` | `10` | Burst allowance on mutation routes |

Nested keys use `__` as separator. A `Config.toml` in the working directory is loaded if present (see the committed example).

---

## Development

**Prerequisites:** Rust stable, [cargo-lambda](https://www.cargo-lambda.info/), [cargo-nextest](https://nexte.st/)

| Command | Description |
|---------|-------------|
| `just test` | Run tests |
| `just fmt` | Format code |
| `just fmt-check` | Check formatting |
| `just lint` | Run Clippy |
| `just local` | Run locally via Lambda runtime emulator |
| `just build` | Build release binary for Lambda (arm64) |

---

## Deployment

**Prerequisites:** AWS SAM CLI, `cargo-lambda`, ARM64 cross-compilation target.

```sh
just deploy        # guided (first time)
just deploy-fast   # subsequent deploys
just logs          # tail Lambda logs
```

The SAM template deploys a single `provided.al2023` Lambda function on arm64, fronted by an HTTP API Gateway. Override `ATPR__BASE_URL` or `ATPR__SLINGSHOT_URL` via SAM parameter overrides or the Lambda console.

---

## Acknowledgements

Thanks to [**Jacquard**](https://github.com/fatfingers23/jacquard) for the AT Protocol OAuth and XRPC client library, and to [**Microcosm**](https://microcosm.blue) for running [Slingshot](https://github.com/microcosm-blue/slingshot), the AT Protocol relay that powers fast link resolution.
