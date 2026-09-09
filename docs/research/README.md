<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Historical research

These five notes preserve the research inputs to Yydra's early design decisions.
Their facts, alternatives, and recommendations belong to their stated research
dates. Archiving them does not adopt their proposals or refresh their external
sources. Their original bodies are preserved; only the repository's SPDX header
has been added.

This index was checked against repository revision
`dcc2e793bf9eaea5879e06956f83f9a038e1c858` on 2026-09-08. Start with the
[context map](../../CONTEXT-MAP.md) for context and ADR locations, and the
[Yydra Framework glossary](../../CONTEXT.md) for vocabulary. Use the subsequent
decisions and implementation below when determining the applicable contract.

| Research date | Note | Subsequent decision or implementation |
| --- | --- | --- |
| 2026-08-26 | [Distribution ownership patterns](yydra-distribution-dioxus-tauri.md) | The [CLI guide](../../crates/yydra-cli/README.md) describes exact Distribution identity, Workspace ownership, inventories, and create-once behavior. The proposed composition index and SBOM links in the research do not independently establish required components. |
| 2026-08-27 | [Public API and client contract](yydra-public-api-client-contract.md) | The [generation library](../../crates/yydra-build/src/lib.rs), rechecked on 2026-09-09, validates the source-derived OpenAPI supplied by its caller and generates the Orval client. The note's OpenAPI-first recommendations remain historical alternatives. |
| 2026-08-31 | [CLI and template-generation contract](yydra-cli-template-generation-contract.md) | The [CLI guide](../../crates/yydra-cli/README.md) documents the selected installation, create-once, Workspace Origin Record, and supported generation workflow. Researching Cargo aliases, cargo-generate, or just did not select them for Yydra. |
| 2026-08-31 | [Mechanical quality tooling baseline](yydra-v0-mechanical-quality-tooling-baseline.md) | The [check graph](../../crates/yydra-cli/src/check_graph.rs) and [supply-chain scope amendment](../adr/0001-remove-supply-chain-from-current-workflow.md) define the subsequent checks and claim boundaries. The research matrix includes iOS and supply-chain proposals beyond that contract. |
| 2026-09-01 | [Agent Skills and Eval contract](yydra-agent-skills-eval-contract.md) | The [CLI guide](../../crates/yydra-cli/README.md) records exactly two Distribution-owned Baseline Skill snapshots and excludes Agent performance and Skill-effect claims. Research on evaluation methods does not establish a successful Yydra Eval. |

## Contract and evidence boundaries

The accepted [2026-09-08 scope amendment](../adr/0001-remove-supply-chain-from-current-workflow.md)
preserves dated research and historical decisions while removing supply-chain
evaluation from the subsequent Distribution. Its
[validation record](../validation/2026-09-08-supply-chain-removal.md) describes
local acceptance of the `0.2.0` candidate. The historical
[0.1.0 release record](../releases/0.1.0.md) retains its own contract; existing
Product Workspaces require their original exact CLI.

The subsequent check graph bounds its claims to required, performed checks.
H5 end-to-end behavior and an Android release build do not establish macOS/iOS,
native runtime, physical-device behavior, native accessibility, Agent
performance, or Baseline Skill effect. Vocabulary such as Golden Stack,
Capability, Agent Eval Campaign, and Safe Completion defines concepts; a
glossary entry does not establish delivery, promotion, or passing evidence.

## Subsequent workflow research

These later notes record separately dated investigations. Their recommendations
need an explicit project decision before becoming repository policy.

| Research date | Note | Decision status |
| --- | --- | --- |
| 2026-09-09 | [PR description conventions](2026-09-09-pr-description-conventions.md) | Comparison of six repository templates and official writing guidance. The maintainer subsequently confirmed the four required and two optional sections recorded in the [GitHub workflow](../agents/github-workflow.md) and [PR template](../../.github/pull_request_template.md). |
