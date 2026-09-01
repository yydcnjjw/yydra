---
name: yydra-product-change
description: Implement an end-to-end Product Domain change in a Yydra Product Workspace across domain rules, use cases, persistence, code-first API, generated client, presentation, and checks. Use for product behavior changes, not Distribution upgrades or Framework internals.
metadata:
  yydra-distribution: "0.0.2-prototype"
---

# Yydra Product Change

Make the smallest coherent Product-owned change while preserving the Workspace's
exact-Distribution authorities.

## Start safely

1. Run `yydra doctor .`. Stop on an origin or Distribution mismatch; never edit
   `.yydra/origin.toml` to make it pass.
2. Read the task and the nearby source/tests before choosing files. Keep product
   rules in normal Product Domain source.
3. Read [references/workspace-map.md](references/workspace-map.md) before a
   cross-layer change. Read [references/change-loop.md](references/change-loop.md)
   when the change reaches persistence, the public API, or presentation.

## Preserve authorities

- Do not edit `.agents/skills/`, `.yydra/origin.toml`, existing migrations,
  `contracts/openapi.json`, `frontend/src/generated/public-api/`, or generated
  `android/` and `ios/` hosts.
- Add a forward-only migration for a schema change. Do not rewrite history.
- Rust/Axum source is the public API authority. Regenerate OpenAPI and the
  Orval/Zod client with `yydra generate api .` after source changes.
- Product Presentation calls the generated client only through
  `frontend/src/framework/api/`. Do not copy domain rules into handlers or UI.
- Required deterministic behavior belongs in code and checks, not only in this
  Skill's prose.

## Finish

Add tests at the owning layer, run focused tests during development, then run
`yydra check .`. Treat every `not-run` node as unverified rather than passed and
report the environment needed to execute it. Stop instead of weakening a rule,
inventing an exception, or adding an upgrade/compatibility path.
