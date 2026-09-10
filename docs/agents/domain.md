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

Research and standalone validation records live in the
[Yydra Wiki](https://github.com/yydcnjjw/yydra/wiki/Home), under
[ADR 0007](../adr/0007-store-research-and-validation-in-wiki.md).
When consulting research, start with its
[index](https://github.com/yydcnjjw/yydra/wiki/Research-Index). These dated notes
preserve earlier facts, alternatives, and recommendations. Use their subsequent
decision and implementation pointers to establish the applicable contract;
a research recommendation alone does not establish one.

Write new research and standalone validation records in the Wiki. Keep current
ADRs, context definitions, agent workflows, and necessary references in this
repository. Do not recreate local copies of the Wiki collection or an automatic
sync. A task or PR can summarize its work without creating a separate Wiki page
for every routine check.

Preserve the original outcomes of date/version-specific validation records.
Identify subsequent corrections separately with their date and reason. ADR
evidence citations must identify a fixed Wiki revision; ordinary navigation can
link to the current page. Editing or publishing the Wiki follows the task's
authorization and does not amend the code repository's PR or validation rules.

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
