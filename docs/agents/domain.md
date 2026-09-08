<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

**Selected layout: multi-context.**

## Before exploring

- Read `CONTEXT-MAP.md` at the repository root.
- Follow it to every context-local `CONTEXT.md` relevant to the work.
- Read relevant system-wide ADRs under `docs/adr/`.
- Read context-local ADRs identified by `CONTEXT-MAP.md`.

If these files do not exist yet, proceed silently. `/domain-modeling` creates them lazily as terms and decisions are resolved.

When consulting `docs/research/`, start with its [research index](../research/README.md).
These dated notes preserve earlier facts, alternatives, and recommendations.
Use the index's subsequent decision and implementation pointers to establish the
applicable contract; a research recommendation alone does not establish one.

## File structure

```text
/
├── CONTEXT-MAP.md
├── docs/
│   └── adr/                 # System-wide decisions
└── <context-root>/
    ├── CONTEXT.md
    └── docs/
        └── adr/             # Context-specific decisions
```

`CONTEXT-MAP.md` is the authoritative directory of context names and locations. Do not infer context boundaries solely from package or directory boundaries.

## Use the glossary's vocabulary

When output names a domain concept—in an issue title, design, hypothesis, test, or implementation—use the term defined by the relevant `CONTEXT.md`.

If a necessary concept is absent, reconsider whether it belongs to the project or record the gap for `/domain-modeling`.

## Flag ADR conflicts

If proposed work contradicts an existing ADR, surface the conflict explicitly instead of silently overriding it.
