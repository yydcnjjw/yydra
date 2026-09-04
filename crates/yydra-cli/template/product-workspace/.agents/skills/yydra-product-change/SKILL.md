---
# SPDX-License-Identifier: MIT OR Apache-2.0
name: yydra-product-change
description: Guide an end-to-end Yydra Product Domain change through use cases, persistence, Public API generation, Product Presentation, accessibility, and supported validation. Use when adding or changing product behavior in a Product Workspace.
---
# Yydra product change

Use this Skill for one coherent Product Workspace behavior change. Keep product
rules in normal Product-owned source and use the Distribution commands as the
only supported orchestration path.

## Establish the boundary

1. Read `AGENTS.md`, `.yydra/origin.toml`, and the relevant existing tests and
   source before editing.
2. State the Product Domain invariant, valid and invalid transitions, Public API
   behavior, and visible Product Presentation outcome.
3. Keep Framework mechanisms reusable and keep distinguishing product rules in
   Product Domain source. Do not introduce a Domain DSL, generic repository, or
   duplicate rule in transport, persistence, or UI code.
4. Read [the vertical change path](references/product-change-path.md) before the
   change crosses a new technical surface.

## Implement one vertical path

1. Write or update a failing test at the narrowest authoritative surface.
2. Implement the Product Domain rule with domain types and explicit errors.
3. Add a typed use case. When correctness spans writes, the use case owns the
   transaction and makes commit or rollback explicit; persistence only performs
   Product Workspace database operations through the borrowed executor.
4. Add only a new forward migration with `yydra db migration add <name> .` when
   storage changes. Never edit an existing migration.
5. Define the Public API in the Rust Axum and utoipa source authority, including
   stable RFC 9457 Problem behavior. Do not hand-edit OpenAPI or Generated Client
   output.
6. Run `yydra generate api .` for the atomic generation path. A failure must
   leave the previous complete outputs intact. Review the Public API and client
   diff before continuing.
7. Implement the Product Presentation through the handwritten Framework client
   facade. Keep server state, URL state, and local component state in their
   declared owners.
8. Add visible accessibility assertions for the changed experience, including
   semantic roles, accessible names, state, and focus or recovery behavior that
   a user can observe.

## Validate before reporting completion

Read [the validation contract](references/validation.md), run focused tests while
iterating, and finish through `yydra check .`. Treat a focused node as diagnosis,
not a complete conformance result. Report what the evidence proves and every
applicable boundary it does not prove.

## Snapshot authority

This Skill is an exact Yydra Distribution snapshot. Its bytes and digest in
`.yydra/distribution-inventory.json` are authoritative. It has no independent
semantic version, compatibility resolver, upgrade path, or lifecycle. A client
may discover this portable `.agents/skills` package, but discovery does not
establish identical activation, tools, permissions, or behavior across Coding
Agents, and this Skill makes no Agent Eval or effectiveness claim.
