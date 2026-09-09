<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Commit conventions

Status: active — 2026-09-09. Confirmed through the commit-convention design
interview and linked from the contributor and agent guidance.

## Scope

These conventions apply to human and agent contributions to the Yydra
repository. Product Workspace contribution policies are owned by their product
teams; this document is not added to generated Workspaces.

Use [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).
Write commit subjects and explanatory bodies in English. Preserve exact names,
quoted source text, and identifiers when translation would change their meaning.

## Applicability and enforcement

Every ordinary commit submitted for review or intended to land on the default
branch must follow this convention. PR titles use the same subject format.
Temporary local `fixup!` or `squash!` commits must be consolidated before review.
Do not rewrite existing shared history just to retrofit this convention.

Merge commits may retain Git or GitHub's generated subject. This exception
does not extend to an ordinary commit produced by squashing a PR, and it does
not exempt any submitted commit from the existing DCO requirement.

Enforce the convention through contributor guidance, agent instructions,
and review. This policy adds no commit-message CI job or local hook, and
does not select a merge strategy or configure automated versioning or releases.

## Commit boundaries

One commit should express one logical change that can be reviewed and reverted
as a unit. Keep the implementation, its relevant tests, documentation, and
required lockfile updates together. A coherent change may span multiple crates
and the Product Workspace template.

Split independent fixes, features, or preparatory refactors when each forms a
coherent change. One Issue may need several commits. Do not split merely by
directory or file type, and do not include unrelated cleanup or pre-existing
work in the same commit.

Select and report validation using the
[local development workflow](development-workflow.md). Review the final diff
and the exact staged changes before committing. Commit boundaries do not
require repeating expensive checks already valid for the relevant inputs.

## Message details

```text
<type>[(scope)][!]: <description>

[body explaining why and any non-obvious effect]

[BREAKING CHANGE: impact and migration guidance]
[Refs: #123]
Signed-off-by: Your Name <your.email@example.com>
```

Square brackets above mean optional content, not literal syntax. The sign-off
is required by the existing [contribution policy](../../CONTRIBUTING.md).
Separate the subject, body, and footer block with blank lines; omit an unused
body or optional footer.

Use a lowercase type from this set:

| Type | Use when |
| --- | --- |
| `feat` | Adding a user- or developer-facing capability |
| `fix` | Correcting unintended behavior |
| `refactor` | Restructuring code without changing its observable behavior |
| `perf` | Improving performance while preserving behavior |
| `docs` | Changing documentation only |
| `test` | Adding or changing tests without production behavior changes |
| `build` | Changing build tooling, packaging, or dependencies |
| `ci` | Changing CI workflows or their execution configuration |
| `style` | Changing formatting without changing behavior |
| `chore` | Repository maintenance that fits none of the above |
| `revert` | Reverting an earlier change |

Choose the type by the commit's primary intent. A feature with accompanying
tests and docs is still `feat`; a build fix is `fix` when its intent is to
correct a defect. Do not classify a behavior change as `chore` or `refactor`
merely because it touches internal files.

Scope is optional. Use a short lowercase area name, with hyphens if needed,
when it makes the subject clearer, such as `cli`, `api`, `template`, `build`,
or `quality`. These examples are not a closed registry. Omit the scope when
the change spans areas and no single scope is helpful.

Describe the concrete outcome with an imperative verb, such as `add`, `fix`,
or `remove`. Prefer a subject of at most 72 characters, including the prefix;
this is a readability target rather than a machine-enforced limit. Omit a
trailing period. Avoid vague subjects such as `update`, `fix bugs`, or `WIP`.

A trivial change may omit the body. Otherwise explain the problem, why the
chosen change addresses it, and any non-obvious consequence. Validation detail
normally belongs in the task or PR report; do not require a duplicate test log
in each commit message or claim checks that were not run.

## References and breaking changes

If the work has a relevant Issue, reference it in a `Refs: #123` footer. An
untracked small change does not need a new Issue solely to satisfy this format.
Prefer Issue-closing keywords in the PR description when the PR actually
completes that Issue; an intermediate commit should use a reference rather
than claim completion. Verify the reference identifies the intended Issue,
since GitHub Issues and PRs share a number space.

Mark a known incompatible change with both `!` in the subject and a
`BREAKING CHANGE:` footer explaining the affected contract and what consumers
must change. This applies to supported CLI usage, configuration, Framework
APIs, and generated-workspace contracts, including during `0.x` development.
A purely internal refactor is not breaking solely because source files move.

This is an author-supplied description of known impact. It does not add
historical API comparison or a breaking-change approval gate to generation,
consistent with [ADR 0002](../adr/0002-simplify-api-generation.md). Message
annotations do not determine or publish the next Distribution version.

For a revert, use `revert: <description>` and identify the reverted commit SHA
and the reason in the body. Preserve the DCO sign-off on the revert commit.

## Existing DCO requirement

Every submitted commit must contain a `Signed-off-by:` line matching its
author's name and email, including merge commits in the submitted range.
Use `git commit --signoff` with the correct author identity. A different
committer's sign-off alone does not satisfy the author-matching requirement.
See [CONTRIBUTING.md](../../CONTRIBUTING.md) for the certification itself.

## Examples

The references and identities below are illustrative.

```text
fix(cli): report an unavailable Java installation

Explain which required tool could not be resolved so that users can repair
the environment before starting a build.

Refs: #123
Signed-off-by: Your Name <your.email@example.com>
```

```text
feat(cli)!: replace the legacy API generation command

BREAKING CHANGE: use `yydra build` instead of the removed generation command.
Signed-off-by: Your Name <your.email@example.com>
```

```text
docs: clarify commit boundaries

Signed-off-by: Your Name <your.email@example.com>
```
