<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# yydra-auth

Reusable GitHub sign-in for an Axum/PostgreSQL product. Compose `AuthService`
around the product router and consume the validated `Principal` request
extension. Product code owns resource authorization.

Copy the library's migration SQL into the product's single forward
migration history. Run that history explicitly before starting the server.
`AuthService::cleanup` removes expired records; the server owns its schedule.

Configure a GitHub App with the backend's `/auth/github/callback` URL and no
additional repository or private-email permissions. Keep the client secret
server-side. Each product owns independent accounts and sessions; GitHub tokens
are discarded after identity verification. GitHub authorization revocation does
not immediately revoke an existing product session.

Browser entrypoints `/auth/github` and `/auth/github/callback` perform redirects.
JSON endpoints are described by `openapi()`. Native login uses a PKCE-bound,
single-use handoff, then the same revocable session system with Bearer transport.

Production requires HTTPS and same-site H5/API deployment. Explicit development
configuration supports local HTTP. Cookie mutations require the configured web
origin and the session CSRF header. Configuration and API errors never disclose
provider tokens or client secrets.

Give each product its own PostgreSQL database. The bundled schema and cookie
names belong to that product; independent browser products use separate API
hosts. Default sessions have a configurable seven-day absolute lifetime.

The `test-provider` feature supplies a loopback-only controlled provider for
explicit validation builds. Production assembly leaves this feature disabled.
