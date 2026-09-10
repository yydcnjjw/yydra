<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# GitHub development workflow

Status: active — 2026-09-10. [ADR 0006](../adr/0006-limit-repository-ci-to-cli-build-and-tests.md)
amends the CI requirement to CLI build/tests plus DCO. The original ruleset was
applied and verified on 2026-09-09; replacing its old required checks is pending
an authorized GitHub integration of this change. Recheck live configuration.

## Scope and responsibilities

This workflow applies to human and agent contributions to the Yydra repository.
Product Workspace contribution policies remain owned by their product teams.
Every change to `main`, including documentation, configuration, and small fixes,
must go through a PR from a task branch. Use one PR per independently reviewable
and mergeable change; that PR may contain several logical commits.

The maintainer decides requirements and authorizes integration. The agent
implements the agreed change, reviews its work, selects and runs validation,
and reports the result. Maintainer review and authorization may happen in the
development task; this workflow does not require a second GitHub account to
approve the maintainer's own PR.

The default development endpoint and external-action authorization remain those
of the [worktree workflow](worktree-workflow.md): reviewed, validated,
cryptographically signed local commits with author-matching DCO sign-offs.
Push, PR creation, merge, and publication follow the task's explicit
authorization. A request to merge a specific PR includes its verified
post-merge task cleanup unless the user asks to retain the task or particular
artifacts. Continue within authorization already given without asking again.
A request to create a PR does not by itself authorize merging or cleanup.
Issue writes also follow the task's authorization, including automatic closure
through PR keywords. Existing authorization does not need to be requested again.

## Prepare and develop a change

Use [GitHub Issues](issue-tracker.md) for tracked requirements and specifications,
with the existing [triage labels](triage-labels.md). Resolve unclear behavior
and acceptance conditions before implementing dependent work. Link an existing
Issue when relevant; a small change does not need a new Issue solely to open a
PR. A readiness label describes the Issue's readiness, not merge authorization.

Use the worktree workflow to start from refreshed `origin/main` or resume the
existing task branch. Keep the primary worktree on `main`; use the default
`.worktree/<slug>` and `codex/<slug>` for manually created tasks. Preserve the
existing synchronization, build isolation, cache reuse, and cleanup rules.

Follow the [commit conventions](commits.md) for logical commit boundaries,
English Conventional Commits, cryptographic signatures, and DCO sign-offs.
Select local checks under the [local development workflow](development-workflow.md).
Local completion does not claim that the PR has passed required CI or merged.

## Open and review the PR

After an authorized push, verify GitHub's signature status for every submitted
commit. When PR creation or update is authorized for the task, create or update
its PR targeting `main`; push authorization alone does not authorize creating a
PR. Use a draft PR when the change still needs implementation or review work.

Use an English Conventional Commit title under the commit conventions. Write
the body in English using the [repository PR template](../../.github/pull_request_template.md).
Preserve exact names, identifiers, and quoted source text when translation would
change their meaning. The required sections are:

| Section | Required information |
| --- | --- |
| `Motivation` | The concrete problem or existing behavior, why the change is needed, and relevant goals or constraints |
| `Solution` | The resulting behavior, meaningful changes, and key approach or trade-off; organize by behavior rather than listing files |
| `Validation` | Checks actually performed, outcomes, relevant coverage and evidence, and material unverified behavior or pending results |
| `User impact and compatibility` | Effects on users, interfaces, configuration, data, or development workflows; applicable migration needs and remaining risks |

Add `Reviewer notes` for useful review questions, critical paths, or remaining
work, and `References` for relevant Issues, PRs, or decisions. Remove these
optional sections when they add no information. Put separately tracked follow-up
work in reviewer notes with its reference and distinguish it from merge blockers.
Use `Closes #<issue>` only when merging this PR completes that Issue and closure
is authorized; use a non-closing reference for partial work.

Scale detail to the change. A small change can use short prose under each
required heading. Validation may use a check/result/coverage table or concise
bullets. Explain why materially relevant checks were not run and distinguish
local results from PR CI and release evidence. When no compatibility effect is
expected, state its concrete basis; do not invent risks to fill the section.
Keep necessary context in the body even when linking a design record. Remove
unfilled placeholders and instructional comments, and avoid empty optional
sections or repetitive certification checklists.

This is Yydra's project convention, informed by Google's change-description
guidance and the compared open-source templates, including Tokio's separation
of motivation and solution and Kubernetes's user-impact and reviewer prompts.
It is not a universal PR-body standard. The
[research note](https://github.com/yydcnjjw/yydra/wiki/Research-2026-09-09-PR-Description-Conventions) records the
sources and decision. Apply the same format when creating or updating a PR
through `gh` or an API. The default GitHub template becomes available after it
lands on `main`; an explicit CLI/API body must still follow the format. Enforce
the writing convention through guidance and review, without a new body-lint CI
job or changes to the existing merge checks.

Review the full diff against the intended base, including the submitted commit
range. Address actionable review findings before merge. Record the PR, target
branch, and reviewed head commit in the handoff. After review edits, refresh
that record, validate affected behavior, and verify new signatures and CI
results for the updated candidate. Authorization for earlier work does not
cover unrelated scope added later.

## Require CLI CI and DCO before merging

Both DCO and the CLI build/test workflow must finish successfully before any
PR merges, including a documentation-only PR. Local selected checks do not
replace this gate. The required jobs are:

| Workflow | Required job name |
| --- | --- |
| DCO | `Every submitted commit is signed off` |
| Yydra CLI CI | `Build and test yydra-cli` |

Inspect results for the current PR candidate. A missing, pending, cancelled,
failed, skipped, or neutral job does not establish the required successful run.
GitHub's required-status-check mechanism can accept skipped or neutral results;
the merge operator must still require actual success from both jobs.

If a check fails, fix the cause or verify a successful rerun after resolving an
execution problem. An infrastructure failure remains a failure. Do not bypass
the gate because local checks passed, the change is small, or the merge was
already authorized. Report any remaining failure or waiting state accurately.

CLI CI builds the release CLI and runs its default unit and command-behavior
tests on Ubuntu with rolling Rust nightly. Real consumer integration tests are
retained for explicit invocation under the [local validation workflow](development-workflow.md).
PRs, pushes to `main`, and manual runs keep the same CI scope. This narrows
routine CI coverage; it no longer runs clean/Reading Queue complete checks or
aggregates Product Workspace Conformance Evidence. Release validation retains
its full requirements, including Android.

## Merge and finish

Once merge is authorized and review and validation conditions are met, refresh
the PR state and confirm the intended repository, `main` target, reviewed head,
lack of conflicts, and current successful checks. Synchronize and revalidate
when needed under the worktree workflow. If the head changes, review the updated
candidate and its results before merging it.

Use GitHub **Squash and merge** by default. A PR's independently mergeable change
becomes one logical commit on `main`; its original commits and discussion remain
available through the PR. Use a compliant title and a final message containing
the resulting author's matching DCO sign-off. Preserve applicable breaking
change and migration notes. Check the resulting commit's GitHub-verified
cryptographic signature as well as its DCO trailer. GitHub's Rebase and merge
rewrites commit objects and does not preserve their original signatures; do not
assume it is interchangeable with a signed local rebase.

Verify that the intended PR is `MERGED` into `main` and that the resulting commit
is included in refreshed `origin/main`. Do not use the original branch head's
ancestry alone to judge a squash merge. Confirm linked implementation Issues
are complete before closing them; parent or tracking Issues may still have
other acceptance conditions.

Use the project-local [yydra-merge-mr skill](../../.agents/skills/yydra-merge-mr/SKILL.md)
to carry a merge request through verification and cleanup. Here, MR means a
GitHub pull request in `yydcnjjw/yydra`. Loading the skill, inspecting a PR,
and creating a PR do not themselves authorize merging it.

After an authorized merge, continue directly through the worktree workflow's
integration, ownership, and cleanup checks. Remove only that task's worktree,
local/remote branch, and registered disposable artifacts, including external
ones. Honor explicit retention requests. Preserve and report additional work,
unresolved ownership, or failed cleanup steps without repeating the general
cleanup-permission question. Report merge and cleanup outcomes separately.

This is task execution, not a background monitor. If the user merges on GitHub
while the task is inactive, inspect the PR when the original task resumes;
continue any previously authorized cleanup. An already merged PR does not need
another merge attempt, and verified absence of a cleanup target is successful
completion for that target. PR merge and cleanup do not authorize
publication. A Distribution release still requires its existing candidate-specific
complete validation and publication authorization; a PR check result does not
automatically establish release readiness.

## GitHub enforcement

The [`main` ruleset](https://github.com/yydcnjjw/yydra/rules/22613063)
prohibits deletion and non-fast-forward updates, requires linear history and
verified signatures, and has an empty bypass list. The required configuration is:

- A pull-request requirement with zero required GitHub approving reviews. The
  maintainer's task-level review and merge authorization remain required.
- Required status checks for both jobs above, from the GitHub Actions app
  (integration ID `15368`). Keep job names aligned with the actual workflows.

The previous configuration required DCO plus four complete-quality jobs:
`Build the exact Distribution executor once`, `Complete clean evidence`,
`Complete reading-queue evidence`, and
`Aggregate clean and Reading Queue conformance`. Before integrating this CI
change, replace those four quality requirements with `Build and test yydra-cli`
in a scoped, authorized ruleset update. Retain DCO, its Actions app identity,
and all other rules. Verify the reviewed candidate's new CLI and DCO jobs,
then read back the ruleset before merge. Do not leave obsolete missing checks
as requirements or treat removal of a requirement as a successful test.
This document records the intended configuration; it does not establish that
the remote update has already happened.

The selected status-check mode (`strict_required_status_checks_policy: false`)
does not require a branch to be up to date with `main` merely because another
PR merged. It retains the worktree workflow's synchronization when needed and
avoids automatic repeated CI runs. This accepts that successful checks may predate the newest `main`
combination. Strict mode would instead require updating and revalidating before
merge when the base advances.

Default Squash and merge is a workflow choice; it does not by itself authorize
disabling other repository merge options. Keep automatic merging and automatic
branch deletion disabled. Subsequent configuration changes require scoped remote
authorization and a read-back to verify the effective rules. Recheck live rules
when assessing whether a PR meets the repository's current requirements.

## References

- [Worktree and verified-commit baseline, PR #46](https://github.com/yydcnjjw/yydra/pull/46)
- [GitHub flow](https://docs.github.com/en/get-started/using-github/github-flow)
- [Available rules for rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)
- [Merge methods on GitHub](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/about-merge-methods-on-github)
