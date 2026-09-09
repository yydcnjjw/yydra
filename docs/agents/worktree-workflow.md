<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Worktree development workflow

Status: active — 2026-09-09. Confirmed through the worktree workflow design
interview and linked from the contributor and agent guidance.

## Scope and task boundaries

This workflow applies to human and agent development of the Yydra repository.
Every code, configuration, or documentation change intended for a commit belongs
in a task worktree. Read-only investigation may use an existing checkout.
Product Workspace contribution policies remain owned by their product teams.
A Git worktree is a checkout of this repository, not a Product Workspace.

One independently reviewable and mergeable change owns one task branch and one
worktree. It may contain several logical commits. Associate an existing Issue
when relevant; a small change does not require a new Issue. Split independent
changes, and resume an existing task in its own worktree instead of creating a
second checkout for the same branch.

Keep the primary worktree on `main` for inspection and integration. Preserve
existing modifications and local commits; do not reset, clean, or stash unrelated
work to start a task. If relevant edits already exist elsewhere, identify the
task's exact changes and verify their transfer before removing the originals.

## Start or resume a task

Read the repository instructions and relevant domain documents, inspect
`git status --short --branch` and `git worktree list --porcelain`, and identify
the task's branch, worktree, and any existing PR before editing. Reuse an
appropriate task worktree created by Codex or another tool after verifying its
ownership and state; its path need not match the manual-creation default.

For a new task, fetch `origin` and branch from the refreshed `origin/main`.
If fetching fails, resolve it or report the failure rather than treating an old
remote-tracking ref as a fresh baseline. Use a different base only when the task
explicitly requires it.

The default location is `<primary-worktree>/.worktree/<slug>`, with branch
`codex/<slug>`. Use a short lowercase hyphenated slug; an associated Issue may
use `issue-<number>-<slug>`. The root `.gitignore` excludes `/.worktree/`.
Check that the chosen branch and path are unused. Inspect an existing directory
or symlink before using it; do not overwrite another task or nest new task
worktrees beneath an existing task checkout.

Run this example from the **primary worktree root**, replacing `example-change`
with the task slug. Run each step only after the previous one succeeds:

```sh
git status --short --branch
git worktree list --porcelain
git fetch origin
main_worktree=$(pwd -P)
task_slug=example-change
task_branch="codex/$task_slug"
worktree_dir="$main_worktree/.worktree/$task_slug"
git worktree add --no-track -b "$task_branch" "$worktree_dir" origin/main
cd "$worktree_dir"
git status --short --branch
```

`--no-track` leaves the new task branch without an upstream; `origin/main` is its
starting point. Use ordinary `-b`, not `-B` or `--force`, so creation does not
reset an existing branch or bypass another worktree's checkout. Primary-worktree
dirty state does not prevent creating an independent task from `origin/main`.
Fast-forward local `main` only when its worktree is clean and the update can
succeed without discarding local commits.

## Develop and validate

Independent tasks may edit and run lightweight checks concurrently. Coordinate
before starting Android or other resource-heavy builds and run those builds
serially across active tasks. Preflight disk space using the existing
[local development workflow](development-workflow.md).

Reuse the tools' ordinary dependency download caches, such as Cargo registry/git
and npm caches. Keep writable `target`, `node_modules`, generated clients, and
validation outputs owned by the individual task or its isolated consumer
Workspace. Do not point concurrent tasks at one writable build-output directory.
Check inherited output-directory overrides before building. Existing validation
runner isolation and supported cache-reuse mechanisms continue to apply.

Run commands from the task worktree and use the CLI built from that candidate
for affected consumer checks. Select checks by changed behavior under the local
development workflow; creating a worktree does not itself require Android or a
complete release validation. Preserve valid caches and evidence, and repeat
expensive checks only when their coverage is no longer valid.

## Synchronize with main when needed

Synchronize when a dependency update, conflict, or integration requirement makes
it necessary. Inspect and commit the task's current changes before synchronizing;
preserve unrelated work and resolve any conflicts before resuming validation.

For a task branch that has never been pushed or otherwise shared:

```sh
git fetch origin
git rebase --gpg-sign origin/main
```

For a task branch that has been pushed or shared:

```sh
git fetch origin
git merge --gpg-sign --signoff origin/main
```

Use the shared-branch rule even if its upstream is missing or the remote branch
was subsequently deleted. Rewriting shared history requires explicit task
authorization. Review the resulting diff and rerun checks affected by upstream
changes or conflict resolution. Follow the [commit conventions](commits.md),
including cryptographic signatures and author-matching DCO sign-offs on any
new merge or replayed commits. These commands require a configured signing
identity; GitHub verification must be checked after an authorized push.

## Finish locally and hand off

The default development endpoint is an implemented, reviewed, appropriately
validated change with cryptographically signed local commits carrying DCO
sign-offs. Review the final diff and exact staged changes; use English
Conventional Commits and `git commit -S --signoff` under the
[commit conventions](commits.md). Include only the task's changes. A DCO trailer
does not provide the required cryptographic signature. If local signing is
unavailable, report that remaining step or use a supported GitHub-signed commit
path within the task's existing authorization for remote actions.

Report the branch, absolute worktree path, commit, checks and results, and any
coverage limits or remaining work. Keep local completion, PR merge, and cleanup
as distinct states. A paused or locally completed task retains its worktree and
branch for continuation.

Push, PR creation, merge, and publication follow the current task's explicit
authorization. Continue within authorization already given without asking again;
local validation or a local commit does not supply that authorization. Use an
explicit task branch when an authorized push is needed:

```sh
git push --set-upstream origin "$task_branch"
```

After pushing, require GitHub-verified signatures for every submitted commit;
local signing success or a passing DCO check alone is insufficient. Check again
after any commit rewrite, including rebase, amend, or squash.
Record the PR, its target branch, and reviewed head commit when handing off.
Refresh that record when review changes the PR head. A request to create a PR
does not by itself authorize merging it or deleting its branch and worktree.

## Clean up an authorized merged task

When the task already authorizes cleanup after merge, carry out the following
checks and cleanup without another permission round. Scope cleanup to that
task's exact worktree and local/remote branch. Preserve the primary worktree,
other tasks and branches, and reusable caches. A closed, unmerged PR or an
abandoned task needs its own scoped disposal decision.

Before removing anything:

1. Query the intended PR with `gh pr view` and confirm it is `MERGED` into the
   intended `main`. Verify repository, branch names, reviewed head, and resulting
   merge commit; branch names alone do not identify the submitted work.
2. Fetch `origin` and verify the merged result is included in current
   `origin/main`. Inspect the local branch and any surviving remote task branch
   for changes beyond the reviewed head. Preserve and report any additional work.
3. Check the task worktree for staged, unstaged, and untracked changes. Inspect
   ignored files for local-only work or evidence that must be retained, and make
   sure no task process is still using the directory. Retain required evidence
   and reusable caches outside the directory before removing it.
4. Fast-forward the clean primary `main` when possible. Preserve and report
   primary-worktree changes or divergence; do not reset it to permit cleanup.

These are inspection examples; set the variables to the verified task identities
in the shell performing cleanup:

```sh
gh pr view "$pr_number" --json number,url,state,baseRefName,headRefName,headRefOid,mergeCommit
git fetch origin
git -C "$worktree_dir" status --short --untracked-files=all
git rev-parse "$task_branch"
git ls-remote --heads origin "refs/heads/$task_branch"
git merge-base --is-ancestor "$task_branch" origin/main
```

For a merge preserving the original commits, the ancestry check must succeed.
After a squash or rebase merge it may return `1` even though the changes were
integrated. Verify the recorded PR head, the resulting commits on `origin/main`,
and the integrated change before proceeding; other nonzero statuses are command
errors. If integration or ownership is uncertain, retain the task and report
what remains unresolved.

Once the checks pass, run cleanup from outside the task worktree, normally from
the primary root. For the ordinary ancestry-preserving case:

```sh
git worktree remove "$worktree_dir"
git branch -d "$task_branch"
git push origin --delete "$task_branch"
git worktree list --porcelain
git status --short --branch
git branch --list "$task_branch"
git ls-remote --heads origin "refs/heads/$task_branch"
```

Skip remote deletion if the verified remote branch is already absent. Stop and
report a failed step rather than blindly running the remaining deletions.
`git worktree remove` does not check whether a branch was merged, and
`git branch -d` may accept containment in the task's upstream instead of `main`;
neither replaces the integration checks above. Do not automatically substitute
`--force` or `-D` after a failure. For a verified squash/rebase merge, a scoped
`git branch -D` can remove the original local branch only after its integrated
change and absence of additional work have been established under the existing
cleanup authorization.

Verify that only the intended task path and refs disappeared. Report any cleanup
still pending. Use `git worktree remove` for registered worktrees, not directory
deletion or wildcard cleanup; `git worktree prune` only removes stale Git
administrative records and is not a substitute for task cleanup.
