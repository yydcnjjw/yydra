# Product Workspace map

Use the current tree as the final authority; these are the V0 ownership seams.

- `crates/domain/`: Product Domain types, invariants, and state transitions.
- `crates/application/`: typed use cases, ports, transaction boundaries, and
  orchestration of domain behavior.
- `crates/persistence-postgres/`: SQLx implementations and migration-state
  checks; no product policy.
- `crates/transport-http/`: public Axum routes, wire DTOs, stable Problems, and
  code-first OpenAPI declarations.
- `crates/server/`: composition root and runtime wiring.
- `migrations/`: one Product Workspace-owned forward-only history.
- `contracts/openapi.json`: committed generated API contract; never hand-edit.
- `frontend/src/generated/public-api/`: committed Orval/Zod output; never
  hand-edit or import directly from Product Presentation.
- `frontend/src/framework/api/`: validation, transport/error classification,
  and the stable façade over generated code.
- `frontend/src/product/` and `frontend/app/`: Product Presentation and routing.
- `frontend/e2e/product-presentation.accessibility.spec.ts`: Product-owned,
  visible role/name/state assertions executed by the Distribution-owned
  `h5.product-presentation-accessibility` check node.
- `frontend/android/` and `frontend/ios/`: ephemeral CNG output, not authored
  source.

Inspect `Cargo.toml`, frontend package scripts, and existing tests before using
a command or assuming a binary name. Optional local infrastructure such as
`compose.yaml` is Product Workspace-owned and may not exist in a blank Workspace.
