<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Structured diagnostic contract

## Doctor

`yydra --message-format=json doctor .` emits JSON Lines with a stable phase and
code, severity, status, message, optional location, and remediation. Doctor is
read-only. Resolve origin and exact-Distribution mismatch before interpreting
downstream package or build symptoms.

## Check

`yydra --message-format=json check .` emits run and node results and writes a
manifest, the exact check catalog, structured diagnostics, and raw logs outside
the Product Workspace. For a failing node, retain:

- `nodeId`, `outcome`, and stable `cause.code`;
- the catalog entry's prerequisites, remediation, `proves`, and
  `doesNotProve` boundary;
- every recorded attempt and raw-log artifact;
- run completeness, scope, Distribution/catalog/executor identities, and
  evidence location.

Interpret outcomes literally:

- `fail` is a required non-pass, but its exact cause may be semantic, policy,
  configuration, or authority drift. Inspect `cause.code` and remediation before deciding
  whether to edit source. Never retry it without an identified state change.
- `infrastructure-error` means required infrastructure or a tool could not
  establish the result; it is not a product pass or failure.
- `skipped` after `CHECK_PREREQUISITE_FAILED` routes to the failing prerequisite,
  not to the skipped node.
- `not-run` or `CHECK_NOT_SELECTED` means the node was outside this focused run.
- A focused node run is incomplete diagnostic evidence even when it passes.

Never infer success from a downstream skip, a raw command succeeding outside
the graph, or an older evidence tree with different identities.
