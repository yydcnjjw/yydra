---
# SPDX-License-Identifier: MIT OR Apache-2.0
name: yydra-diagnose
description: Interpret Yydra Workspace and environment diagnostics and route build, setup, database, and frontend failures to focused repairs.
---
# Diagnose a Yydra Product Workspace

1. Run `yydra --message-format=json doctor .`; select `--target server`,
   `--target h5`, or `--target android` when diagnosing one application target.
2. Read [the diagnostic contract](references/diagnostic-contract.md). Preserve
   phase, code, severity, status, message, location, and remediation.
3. Follow [safe repair routes](references/repair-routes.md). Fix the concrete
   missing tool, configuration, or Workspace authority named by the diagnostic.
4. Run setup explicitly if dependency installation is needed. Investigate build
   or test failures through their owning command and its actual output.
5. Rerun the affected command after repair; report what ran and what it proves.

## Repair boundaries

- Do not hand-edit snapshots such as Baseline Skills, licenses, or the Workspace
  Origin Record.
- Do not hand-edit generated authorities such as OpenAPI, Generated Client,
  or generated native hosts.
- Do not weaken required rules or tests to hide a failure.
- Do not silently retry semantic failures; inspect and fix their reported cause.
- Do not invent an exception, approval, credential, or successful test result.

## Snapshot authority

This Skill is an exact Yydra Distribution snapshot. Its bytes and digest in
`.yydra/distribution-inventory.json` are authoritative. It has no independent
semantic version, compatibility resolver, upgrade path, or lifecycle. Discovery
does not establish identical activation, permissions, or behavior across Coding
Agents, and this Skill makes no Agent Eval or effectiveness claim.
