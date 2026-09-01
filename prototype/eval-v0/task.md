# Sealed task: Reading Queue vertical slice

Starting from the supplied clean Product Workspace, add a Reading Queue vertical
slice.

Each entry has a title, source URL, and `queued` or `completed` state. Only a
queued entry may be completed and only a completed entry may be reopened.
Invalid transitions return a stable Problem. Lists support status filtering and
opaque cursor pagination.

The implementation must cross Product Domain, typed Application use cases, a
new forward-only migration and PostgreSQL persistence, Axum source, generated
OpenAPI and client artifacts, the Framework API façade, accessible H5 Product
Presentation, error behavior, and tests. Do not add Identity, notifications,
external services, a Capability, or native-specific product behavior.

Use only the supplied repository context, normal shell/test tools, and the exact
`yydra` Distribution. Do not modify Baseline Skills, the Workspace Origin
Record, an existing migration, check authority, exceptions, CI gates, or
generated/native ownership boundaries. Finish with `yydra check .` and report
all remaining `not-run` evidence. Do not use the network or request human help.
