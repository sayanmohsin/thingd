# Security

thingd's security model and hardening options.

## Authentication

thingd uses Bearer token authentication via the `Authorization` header.

Hosted thingd Cloud app backends additionally use a project publishable key via
`X-Thingd-Publishable-Key`. Publishable keys are intended for browser and
mobile bundles; never embed a Cloud secret API key or engine runtime token in
an app. Project-user access tokens are scoped to one Cloud project and should
be stored with the platform's secure storage facilities.

**Configuring a token:**

```yaml
# config.yaml
auth:
  token: "your-secure-token-here-min-16-chars"
  allow_unauthenticated: false
```

Or via environment variable:

```bash
export THINGD_AUTH_TOKEN="your-secure-token-here-min-16-chars"
```

**Token requirements:**
- Minimum 16 characters when `allow_unauthenticated` is `false`
- Empty token = no auth (allowed when `allow_unauthenticated` is `true`)
- The auth middleware is only wired when a non-empty token is configured **and** `allow_unauthenticated` is `false`

**Unauthenticated endpoints:**
- `/healthz`, `/v1/health` — health checks
- `/cluster/status`, `/cluster/peers` — cluster status (non-sensitive)

## TLS / HTTPS

thingd-server does not serve HTTPS directly. The recommended deployment pattern uses a reverse proxy:

**Option 1: nginx (recommended)**

```nginx
server {
    listen 443 ssl;
    server_name thingd.example.com;

    ssl_certificate /etc/letsencrypt/live/thingd.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/thingd.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8757;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

**Option 2: Caddy**

```caddyfile
thingd.example.com {
    reverse_proxy 127.0.0.1:8757
}
```

## CORS

CORS is configurable via `hardening.cors_allowed_origins`:

```yaml
hardening:
  cors_allowed_origins:
    - "http://localhost:8757"
    - "https://thingd.example.com"
  cors_max_age_secs: 86400
```

- Empty list = permissive (`*`) — backward compatible
- Only specified origins are allowed in the `Access-Control-Allow-Origin` header
- Methods: `GET, POST, PUT, DELETE, OPTIONS`
- Headers: `Authorization, Content-Type, MCP-Protocol-Version`

## Rate Limiting

Per-IP token bucket rate limiting, disabled by default:

```yaml
hardening:
  rate_limit_enabled: true
  rate_limit_requests_per_minute: 60
```

- Returns `429 Too Many Requests` when exceeded
- Bucket refills over 60-second window
- Keyed by client IP (`X-Forwarded-For` or connection address)

## Error Sanitization

In production mode, internal error details are stripped from API responses:

```yaml
server:
  production_mode: true
```

- Storage errors map to generic `"Internal server error"` messages
- Full error details are still logged server-side
- In development mode (default), all error details are returned to the client
- When `production_mode` is `true`, an auth token is required

## Input Validation

- All SQL queries use parameterized bindings (no string interpolation for user values)
- Filter keys for `json_extract` are validated against alphanumeric, underscore, and dot characters only
- LIMIT and OFFSET use bound parameters (not string formatting)
- Payload size is limited via `hardening.max_payload_bytes` (default 512KB)

## CI Security Scanning

The CI pipeline runs the following security checks on every push and PR:

- **`cargo audit`** — Rust dependency vulnerability scanning
- **`cargo deny`** — License compliance and duplicate crate detection
- **`pnpm audit`** — Node.js dependency vulnerability scanning
- **CodeQL** — Static application security testing for Rust and JavaScript

## Security Checklist for Production

- [ ] Set `THINGD_AUTH_TOKEN` to a strong, unique token (minimum 16 characters)
- [ ] Deploy behind a reverse proxy with TLS (nginx or Caddy)
- [ ] Configure `hardening.cors_allowed_origins` to your specific origins
- [ ] Enable rate limiting: `hardening.rate_limit_enabled: true`
- [ ] Enable production mode: `server.production_mode: true`
- [ ] Set `hardening.max_payload_bytes` to an appropriate limit for your use case
- [ ] Use filesystem-level encryption for the SQLite database directory
- [ ] Regularly run `thingd db integrity` to check for corruption
- [ ] Schedule regular backups with `thingd backup --out <path>`

## Vulnerability Reporting

If you discover a security vulnerability in thingd, please report it by opening an issue at https://github.com/sayanmohsin/thingd/issues with the label `security`.
