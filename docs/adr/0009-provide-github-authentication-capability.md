<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Provide reusable authentication with GitHub sign-in

Status: accepted — 2026-09-11. Implemented in the local Distribution 0.6.0
candidate. Controlled PostgreSQL, H5, Android release build, and Android emulator
acceptance have passed. Live GitHub registration/consent and complete release
conformance are not established by these selected checks.

## Confirmed capability scope

Provide authentication as a selectable Yydra Capability, with GitHub as the first
external identity provider. Shared libraries own the reusable implementation;
the Product Workspace template owns assembly and product pages. New Workspaces
enable the login flow by default on both H5 and Android. Authentication stays
inside the existing Rust backend, preserving the product's deployment shape.

GitHub verifies the external identity; each product owns independent Product
Accounts and Product Sessions. Any GitHub user may sign in. The first successful
sign-in automatically creates an account; subsequent sign-ins resolve it through
GitHub's durable user ID. Handles and email addresses are not identity keys.
Signing out of a product revokes only the current product session. Product
sessions support concurrent devices, survive application restart while valid,
and expire at most seven days after login, configurable by the product. This is
an absolute lifetime; activity does not extend it.

The template isolates Reading Queue data per Product Account. Product Domain
and application code own resource authorization; the capability owns identity
verification and session behavior. User profiles, organizations, roles,
permission management, local passwords, and coordinated cross-product sign-out
are outside this first integration.

Prioritize reuse of open-source authentication and session libraries, with
crates.io-published upstream dependencies only. The maintainer permits downgrading
SQLx 0.9 to a compatible published release so the PostgreSQL session store can
be reused. An extra TypeScript authentication runtime and a separately deployed
identity server were considered but would change the selected backend and
deployment boundary.

## Implementation contract

### Shared implementation and delivery

Maintain a canonical `yydra-auth` Rust crate and a canonical frontend authentication
package. The Rust crate integrates OAuth, provider identity mapping, durable
sessions, and H5/native credential transport. The frontend package integrates
session state, credential access, and login/logout lifecycle with Expo. Product
pages, navigation, and Reading Queue ownership rules stay in the template.

Amended on 2026-09-12 after the maintainer selected package dependencies and then
local registries for the development phase. Maintain the canonical packages in
this repository and publish `yydra-auth` to a local Kellnr registry and
`@yydra/auth-client` to a local Verdaccio registry. Generated products use exact
development versions and ordinary Cargo/npm resolution. Authentication library
source files are no longer embedded in the CLI or copied into product templates.
The earlier source-snapshot implementation is retained in the historical
2026-09-11 acceptance evidence; `yydra-build` keeps its existing snapshot delivery.

Reuse the standard registry servers and `cargo publish`/`npm publish` clients.
Registry containers bind only to the host loopback interface. Only Yydra-owned
packages use these registries; external Rust dependencies still come from
crates.io, and other npm packages retain the selected public mirror. Credentials
and persistent server data are local development state, outside product source
and source control. A stopped registry retains its packages for the next start.

Each changed publication needs a new `-dev.N` package version. Products lock the
specific versions and package checksums supported by their Distribution.
`doctor` and `check` verify those declarations and locked identities, replacing
the former authentication snapshot-byte checks. Explicit package updates also
update the template locks and undergo consumer validation; automatic product
upgrades and a general plugin resolver remain outside this change.

Local registry addresses are part of the development dependency sources. Linux
container builds use the documented local-registry Compose override to reach the
same loopback URLs through BuildKit's host network. Registry availability is a
dependency-fetch/build prerequisite, not a product runtime service. Public
crates.io/npm publication is a later explicit release action; switching to those
sources requires regenerated locks and consumer validation.

Use the following exact initial Rust dependency combination:

| Responsibility | Published package | Version |
| --- | --- | --- |
| OAuth protocol | `oauth2` | `5.0.0` |
| Axum authentication integration | `axum-login` | `0.18.0` |
| Session lifecycle | `tower-sessions` | `0.14.0` |
| PostgreSQL session persistence | `tower-sessions-sqlx-store` | `0.15.0` |
| Product and session database access | `sqlx` | `0.8.6` |

Use one SQLx version across the product and session store. This selects the
published store's compatible dependency family over SQLx 0.9 with unpublished
upstream commits or a replacement store implementation. The locked dependency
graph, authentication-layer assembly, and product backend compile together. Real PostgreSQL regressions cover migrations, account ownership,
transaction rollback, pagination, and durable authentication sessions after the
downgrade.

### GitHub identity and product sessions

Document GitHub App user authorization as the default registration path, using
only the identity information needed for sign-in. A user can authorize a GitHub
App without installing it. Each product/environment configures its client ID,
server-only client secret, and exact backend callback URL. The backend performs
authorization-code exchange, validates state and PKCE S256, then obtains the
stable GitHub user ID. Product accounts are created atomically under a unique
provider/subject constraint. Missing configuration leaves protected access
closed and produces a clear sign-in configuration error.

These mechanics follow GitHub's documented
[user authorization flow](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app)
and [authorization without installation](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-with-a-github-app-on-behalf-of-a-user).
No repository or private-email permission is needed for the proposed identity
contract. GitHub access/refresh tokens are not product session credentials and
are not retained after identity resolution.

Product sessions remain independently valid until product logout or their
absolute expiry. Signing out of GitHub or revoking the GitHub application's
authorization does not immediately revoke an existing product session in this
first version. Immediate provider-revocation synchronization would require an
additional verified webhook or provider revalidation contract; it is not implied
by independent product sessions.

Reuse the published store for session persistence and implement only the
provider, account mapping, and transport integration missing from these libraries.
Preserve one explicit, append-only root migration sequence for accounts, provider
identities, sessions, and product tables. Server startup verifies migrations;
it does not run an independent library migrator. Session expiry is enforced on
access, and obsolete session/login-attempt/handoff records have a bounded cleanup
lifecycle. Failed dependency or database access never admits an unauthenticated
request.

### H5, Android, and product integration

H5 uses an HttpOnly session cookie, Secure in production, with a restrictive
SameSite policy. Support the template's explicit local development origins;
require configured credentialed CORS and CSRF protection for browser mutations.
The production deployment contract uses HTTPS and same-site frontend/API hosts.
The frontend never stores a product credential in browser localStorage.

Android uses Expo's system-browser integration and SecureStore-backed credential
storage. After the GitHub backend callback, a short-lived, single-use handoff
bound to the initiating app's verifier returns to an allowlisted application
link. The app redeems it over the product API for an opaque product-session
credential. Neither a GitHub token nor a reusable session credential appears in
the redirect URL. A narrow native Bearer adapter feeds the same session system;
the published Cookie session middleware does not itself provide this handoff.
Use dependencies compatible with the template's pinned Expo SDK and verify them
through native compilation and runtime checks.

Clear and isolate authenticated request/query state on logout or account change.
Cancel applicable in-flight work and reject late responses from the previous
session. A failed or offline logout may clear the local view but must not report
successful server revocation until that revocation is confirmed.

Protect Reading Queue reads, writes, progress state, and pagination cursors by
the authenticated Product Account. Resource ownership comes from the validated
session, never a caller-supplied account ID. The template's fixed-token example
does not remain a production authentication path. Test identities and fake
providers are confined to controlled validation fixtures.

Apply the change to a new Distribution and newly generated Workspaces. Preserve
the bytes of existing migrations and add forward migrations. Existing anonymous
data must not be silently assigned to the first GitHub user; an existing product
with such data needs an explicit ownership migration before adoption.

### Acceptance and evidence

Completion requires the packaged CLI to create and validate a consumer using
the shared libraries, the selected published dependencies, and the template
login integration. Validate the changed architecture and snapshot rules, public
API/client contracts, and the following observable behavior:

- Real PostgreSQL migration and transaction regressions after the SQLx downgrade;
  concurrent first sign-in creates one account, and data is isolated for two
  accounts across all Reading Queue operations and progress state.
- Login/callback replay, invalid state/verifier, denied authorization, expired
  handoffs, session fixation, expiry, current-session logout, and continued
  access from another valid device have explicit integration coverage.
- H5 login, reload, logout, account switching, CSRF/CORS enforcement, and stale
  response/cache isolation work against a controlled provider fixture.
- Android generation and build succeed. Exercise browser return, handoff
  redemption, restart persistence, and logout on an emulator or device; an APK
  build alone is not runtime acceptance.

Use controlled provider fixtures for reproducible automated checks and keep
them distinct from a live GitHub smoke test. A live test requires a configured
GitHub application and user authorization; do not claim it ran without evidence.
Report missing live/runtime evidence explicitly. Selected checks retain
`pass-selected` / `complete: false` semantics and do not substitute for the full
release validation required by the repository workflow.
