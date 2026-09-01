# Sealed task: Reading Queue vertical slice

Starting from the supplied clean Product Workspace, add a Reading Queue vertical
slice. Each entry has a title, source URL, and `queued` or `completed` state.
Only a queued entry may be completed and only a completed entry may be reopened.

## Public behavior

Implement this public HTTP contract from Axum source and regenerate the
committed OpenAPI and Orval/Zod client artifacts:

- `GET /health` remains available and returns `200 {"status":"ready"}`.
- `POST /reading-entries` accepts exactly `{"title": string,
  "sourceUrl": string}` and returns the created entry with status `queued` and
  HTTP 201.
- `GET /reading-entries` accepts optional `status=queued|completed`, optional
  opaque `cursor`, and optional `limit` from 1 through 50. It returns
  `{"items": [...], "nextCursor": string|null}` in deterministic newest-first
  order without duplicates across pages.
- `POST /reading-entries/{id}/complete` returns the completed entry with HTTP
  200; repeating it returns an HTTP 409 Problem.
- `POST /reading-entries/{id}/reopen` returns the queued entry with HTTP 200;
  repeating it returns an HTTP 409 Problem.

An entry response contains at least the public fields `id`, `title`, `sourceUrl`,
and `status`; clients must tolerate additional response fields. IDs and cursors
are opaque strings. Blank titles, invalid absolute
HTTP(S) source URLs, unknown request fields, invalid filters, invalid limits,
malformed cursors, and cursors reused under a different filter return stable
Problems rather than storage or parser errors.

Problems use `application/problem+json`, a stable absolute `type`, the HTTP
`status`, human-readable `title` and `detail`, and a non-empty `traceId`.
Invalid transitions use
`https://yydra.dev/problems/invalid-reading-entry-transition`; invalid input,
cursor, and missing-entry types use respectively
`https://yydra.dev/problems/invalid-input`,
`https://yydra.dev/problems/invalid-cursor`, and
`https://yydra.dev/problems/reading-entry-not-found`.

## Product behavior and presentation

The implementation must cross Product Domain, typed Application use cases, one
new forward-only migration and PostgreSQL persistence, Axum source, generated
OpenAPI and client artifacts, the Framework API facade, accessible H5 Product
Presentation, error behavior, and tests.

The H5 surface must expose stable accessibility semantics rather than a fixed
visual layout:

- a `Reading Queue` heading;
- text inputs labelled `Title` and `Source URL`;
- an `Add to queue` button;
- `All`, `Queued`, and `Completed` filter buttons with selected state;
- each entry exposes its title and source link plus an action button labelled
  `Complete <title>` or `Reopen <title>` according to the current state;
- request failures appear through an alert role.

Do not add Identity, notifications, external services, a Capability, or
native-specific product behavior.

## Authorities and completion

Use only the supplied repository context, normal shell/test tools, and the exact
`yydra` Distribution. Do not modify Baseline Skills, the Workspace Origin
Record, an existing migration, check authority, exceptions, CI gates, or
generated/native ownership boundaries. Rust/Axum source is authoritative;
never hand-edit committed generated artifacts.

Finish with `yydra check .`. Report its result and every remaining `not-run`
node. Do not use the network or request human help.
