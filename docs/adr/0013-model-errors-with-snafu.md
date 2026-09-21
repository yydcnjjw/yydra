<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Model failures at their owning boundary with SNAFU

Status: accepted — 2026-09-20.

The maintainer selected all Rust code (CLI, build library, authentication and
Product Workspace templates), explicit module/use-case errors, and permission to
redesign all error interfaces. Use exact SNAFU 0.9.2 to construct typed errors
and preserve causes and operation context. A Product Domain rejection expresses
a product rule; unavailable dependencies and damaged persisted state are technical
failures. Their public treatment is decided at the application/transport boundary,
not by assigning an HTTP status to a domain error.

Business errors expose matchable rule variants. Reusable libraries expose an
opaque `Error` and a non-exhaustive `ErrorKind`; their source chain is diagnostic,
not an invitation to branch on downstream types or message text. Context selectors
remain private. Use concrete sources except at genuinely polymorphic boundaries.
Remove direct `anyhow` use from Yydra-owned runtime code; third-party transitive
dependencies are not part of that promise. Programming defects remain distinct
from recoverable operation failures.

## Public boundaries

Provide `yydra-http` independently of the optional Authentication Capability.
It owns RFC 9457 Problem formatting, server-generated request identity and final
response reporting. Authentication and product transports own their mappings.
Neither Product Domain nor the build/CLI tools depend on this HTTP mechanism.
HTTP `type` identifies the problem, `requestId` connects the response to logs,
and input failures carry structured fields and rule codes. Human `title`/`detail`
are not machine identifiers. Preserve 401/403 semantics, authentication challenges,
and the client's separation of problems, transport, cancellation and malformed
contracts. Client mutations do not gain automatic retries.

Upgrade CLI JSONL to schema 2, separating execution-step codes from typed failure
codes. Keep stage start/completion events and Doctor's required/optional policy.
Stdout remains machine-readable in JSON mode; child output belongs on stderr.
Use types to select codes, never parse a rendered error. Diagnostic prose may
change without changing failure identity.

Technical errors retain their source and context but are reported once at the
handling boundary using explicitly safe metadata. Raw credentials, request/response
bodies, SQL values and unsanitized external error strings are not automatically
logged or serialized. Capture backtraces only when requested; expected business
rejections do not capture stacks. Request identity covers authentication as well
as product routes. The template subscriber suppresses axum-login/tower-sessions
logging targets because those dependencies format raw backend errors themselves.
If a third-party middleware consumes its error and returns only a status, the
adapter records the known session boundary failure; it does not invent a lost
cause. Other hosts must apply the same dependency-log policy.

## Transaction failures

Retain both the original operation error and a failed rollback. Report that
combination as a technical failure instead of replacing the original cause or
returning an ordinary business rejection. A commit failure whose outcome cannot
be established is represented as outcome unknown; do not retry writes implicitly.
Restoring invalid stored data is a technical failure even when the same rule
would reject new input as a client error.

## Delivery and validation

Implement this decision in an isolated task worktree with affected template,
package, source-consumer, frontend and agent-guidance changes. Validate module
classification, preserved causes, double failures, HTTP redaction and correlation,
CLI schema 2, PostgreSQL transactions, generated contracts, H5 and affected Android
build integration. Package publication remains a separate authorized action;
source-consumer results do not establish registry delivery.

Alternatives rejected: a global error enum, broad string errors outside the
domain, exposing every library implementation field, replacing RFC 9457 with a
custom envelope, and duplicating HTTP mechanisms in Auth and product code.

References: [SNAFU](https://docs.rs/snafu/0.9.2/snafu/),
[opaque errors](https://docs.rs/snafu/0.9.2/snafu/guide/opaque/index.html),
[RFC 9457](https://www.rfc-editor.org/rfc/rfc9457.html).
