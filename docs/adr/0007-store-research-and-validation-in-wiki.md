<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Store research and validation in the Wiki

Status: accepted — 2026-09-10. Wiki publication and independent-clone content
verification completed at revision `6a878e624ef5fba3d3069e3daf38a7169d798982`.
Repository removal follows its separate PR and merge workflow.

## Decision

Use the [Yydra Wiki](https://github.com/yydcnjjw/yydra/wiki/Home) for the existing
research collection and dated validation records, and for new records of those
kinds. Keep current ADRs, context definitions, agent workflows, and necessary
references in the code repository. Do not maintain duplicate collections or an
automatic sync. This separates historical supporting material from the current
rules that agents read with the code; reading the full collection requires the
Wiki or a separate local Wiki clone.

The initial migration carries the eleven files under `docs/research/` and
`docs/validation/` from source commit
`05a66d0b463844a0082653fa9e7497320368df57` into eleven content pages and one
navigation home. Preserve their original wording, dates, outcomes, failures,
coverage limits, evidence identities, and SPDX notices. Adapt relative links and
append migration provenance. The source commit identifies the copied snapshot,
not necessarily the repository version on each record's original date.

Research recommendations remain decision inputs. Use accepted ADRs and current
implementation to establish the applicable contract. Date/version-specific
validation pages retain their original outcomes; identify later corrections
separately with a date and reason. ADR evidence references must use a fixed Wiki
revision, while navigation links may follow the current page.

Task and PR reports still summarize their checks; not every routine run needs a
standalone Wiki page. Detailed records belong in the Wiki and identify the exact
candidate and executor, checks actually run, outcomes, evidence locations, and
coverage limits. Wiki edits follow their own Git or web workflow and the task's
publication authorization. They do not acquire the code repository's PR/CI
coverage or alter its validation and merge requirements.

## Preservation and migration

This decision changes the storage location of the dated research and validation
records preserved by ADRs [0001](0001-remove-supply-chain-from-current-workflow.md),
[0003](0003-use-rolling-rust-nightly.md), and
[0006](0006-limit-repository-ci-to-cli-build-and-tests.md). Their original claims
remain intact in the Wiki and code-repository history. Historical decisions,
release records, Issues, tags, packages, and raw evidence remain in their existing
locations. A local evidence path is not a public artifact link, and copying the
record does not reverify that artifact or its current availability.

Before merging the repository removal, publish and read back all twelve Wiki
pages, independently clone the Wiki, and compare its contents with the reviewed
candidate. Record the published revision and replace pending evidence references
with that exact Wiki revision. Verify the rewritten repository references and
Wiki navigation. If publication or verification fails, retain the source
collection in the default branch and keep the migration pending.

Validate this documentation-only change through content-preservation checks,
link checks, SPDX notices, and diff review. It does not trigger an Android build
or change the Product Workspace Mechanical Quality Contract or release gates.
