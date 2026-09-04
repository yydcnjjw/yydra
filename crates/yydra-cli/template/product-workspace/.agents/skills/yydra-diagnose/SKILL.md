---
# SPDX-License-Identifier: MIT OR Apache-2.0
name: yydra-diagnose
description: Interpret structured Yydra doctor and check results and route stable diagnostic codes to safe focused repairs. Use when setup, generation, validation, H5, database, or Android evidence fails in a Product Workspace.
---
# Diagnose a Yydra Product Workspace

Use the structured Distribution result as the starting evidence. For `doctor`,
preserve the final phase, code, status, message, location, and remediation. For
`check`, preserve the first failing node and cause plus its prerequisites,
attempts, proof boundary, and raw-log reference before changing anything.

## Read the result

1. Run `yydra --message-format=json doctor .` for read-only origin, exact
   Distribution, and toolchain diagnosis.
2. Run `yydra --message-format=json check . --node <stable-id>` only when the
   failing node or its smallest relevant parent is known. If it is not known,
   run `yydra --message-format=json check .` once and inspect the first stable
   failing cause plus independent node results.
3. Read [the diagnostic contract](references/diagnostic-contract.md) before
   interpreting failure, infrastructure-error, skipped, not-run, or incomplete
   evidence.
4. Read [safe repair routes](references/repair-routes.md) for the exact reported
   code before considering its broader family. Inspect the named authority or
   external condition and make only the repair that the concrete result calls
   for.
5. Rerun the exact discriminating command once. Widen to the final supported
   `yydra check .` path only after the focused evidence passes.

## Non-negotiable safety rules

- Do not hand-edit snapshots such as Baseline Skills, licenses, or the Workspace
  Origin Record.
- Do not hand-edit generated authorities such as OpenAPI, Generated Client,
  generation records, or generated native hosts.
- Do not weaken required rules, catalogs, tests, CI gates, or proof boundaries.
- Do not silently retry semantic failures; inspect and fix their reported cause.
- Do not invent an exception, waiver, approval, evidence reference, or service
  result.

Use the supported generator or clean native generation path when a derived
authority must change. Stop and ask for the missing credential, approval,
specification choice, or external service when the evidence says it is required.

## Snapshot authority

This Skill is an exact Yydra Distribution snapshot. Its bytes and digest in
`.yydra/distribution-inventory.json` are authoritative. It has no independent
semantic version, compatibility resolver, upgrade path, or lifecycle. A client
may discover this portable `.agents/skills` package, but discovery does not
establish identical activation, tools, permissions, or behavior across Coding
Agents, and this Skill makes no Agent Eval or effectiveness claim.
