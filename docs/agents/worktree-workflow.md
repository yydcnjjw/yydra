<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Worktree development workflow

Status: active — 2026-09-10. The 2026-09-09 worktree workflow is amended by
the confirmed task-artifact cleanup decision, including outputs outside the
worktree, and the merge-request default that includes verified task cleanup.
Linked from the contributor and agent guidance.

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

The [GitHub development workflow](github-workflow.md) requires every change to
`main` to use a PR and selects Squash and merge by default. It defines PR review
and required-CI requirements in addition to this document's local workflow.

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

## Track task artifacts and caches

Keep temporary outputs whose location can be selected under
`<task-worktree>/.task/tmp/`. Maintain an inventory at
`<task-worktree>/.task/artifacts.md`; `/.task/` is ignored by Git. Initialize it
once and read it when resuming a task. The inventory is an agent-maintained
cleanup record, not an automatic cleanup command or a standalone validation
report. Keep source changes and deliverable documentation outside `.task/`.

Record the task branch and absolute worktree path, then each artifact's absolute
path, purpose, creating command/tool, ownership evidence, disposition, and
status. A task-owned directory may cover its contents as one entry; identify
any retained exceptions. Register planned paths before creating them and add
tool-selected paths as soon as they are known. Include temporary consumer
Workspaces, build outputs, logs, downloads made only for the task, PR bodies,
API request/response files, and tool-generated caches. Do not put credentials
or raw authenticated request contents in the inventory.

Assign a disposition to each entry:

| Disposition | Lifetime and cleanup |
| --- | --- |
| Disposable task artifact | Remove when no longer needed for execution, review, or diagnosis, and by authorized task cleanup. This includes task-private `target`, `node_modules`, and generated caches. |
| Required evidence | Record why it must remain, its destination, and the condition or date for reconsidering retention. Retain the bytes needed by the evidence claim. |
| Reusable shared cache | Preserve established dependency/download caches and supported reusable build caches. Record the reuse purpose for any task-created cache retained beyond the task. |

Before running tools, inspect their output/cache settings and inherited
overrides. Use supported command-scoped temporary/cache-directory options to
place disposable outputs under `.task/tmp/`, including `TMPDIR` when the tool
honors it. Keep ordinary shared dependency caches available and preserve the
validation runner's isolation requirements. If a tool requires an external
location, register its actual path and reason instead of silently leaving an
untracked output there. A registered external path does not require another
permission round when the task already authorizes its cleanup.

Inventory coverage follows task ownership, not a directory prefix. Include
paths under `~/.cache`, `/tmp`, `/var/tmp`, overridden temporary roots, and
other external locations. Tools may create generic names such as `metro-cache`
without `yydra` in the name. For mixed/shared directories, record only entries
whose task ownership can be established; a task path in one cache entry does
not establish ownership of its siblings. Do not delete a shared cache root to
remove a task's entries.

When a command succeeds or fails, update the inventory with discovered outputs
and files already removed. A failed or paused task keeps artifacts still needed
for diagnosis or continuation. Moving a directory into `~/.cache` does not make
it a reusable cache, and moving evidence outside a worktree does not finish its
lifecycle: update its recorded destination and retention condition. A Wiki
summary alone does not preserve local artifact bytes.

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
branch for continuation. Include the artifact inventory path and summarize
remaining external artifacts, retention reasons, and unresolved ownership in
the handoff. Report residual artifacts even when Git status is clean.

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

A user request to merge a specific PR includes this post-merge cleanup for that
task, unless the user asks to retain its worktree, branch, or particular
artifacts. Apply the same scope when resuming that merge request after the PR
has already merged. Explicit retention and narrower task instructions take
precedence. Skill selection, a status query, or a request to create a PR does
not supply merge or cleanup authorization.

When the task already authorizes cleanup after merge, carry out the following
checks and cleanup without another permission round. Scope cleanup to that
task's exact worktree, local/remote branch, and registered disposable artifacts,
including those outside the worktree. Preserve the primary worktree, other
tasks and branches, shared caches, and required evidence under the
[artifact inventory rules](#track-task-artifacts-and-caches). A closed, unmerged
PR or an abandoned task needs its own scoped disposal decision.

Before removing anything:

1. Query the intended PR with `gh pr view` and confirm it is `MERGED` into the
   intended `main`. Verify repository, branch names, reviewed head, and resulting
   merge commit; branch names alone do not identify the submitted work.
2. Fetch `origin` and verify the merged result is included in current
   `origin/main`. Inspect the local branch and any surviving remote task branch
   for changes beyond the reviewed head. Preserve and report any additional work.
3. Check the task worktree for staged, unstaged, and untracked changes. Inspect
   ignored files and `.task/artifacts.md` for local-only work and required
   evidence. Reconcile the inventory with the task's actual commands and tool
   outputs, including external paths. For older tasks without an inventory,
   reconstruct it from creation records and current files; a name-only search
   is not sufficient. Verify exact path identities, symlinks/mount boundaries,
   and process use before deletion. Preserve and report uncertain entries.
   Retain required evidence and reusable shared caches outside the worktree
   when necessary, recording the destination, reason, and retention condition.
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
the primary root. First remove verified disposable external entries using their
exact recorded paths and update the inventory. Stop and report failed deletions;
do not discard the inventory while external cleanup remains unresolved. Keep
the reviewed inventory available in the task report before removing the worktree
that contains it. Temporary cleanup scripts and inventories created outside the
worktree are themselves disposable entries and must also be accounted for.
Do not use wildcard deletion across `~/.cache`, `/tmp`, or other shared roots.
For the ordinary ancestry-preserving case, set `remote_task_head` to the
surviving remote task branch's inspected commit SHA after proving it contains
no additional work. The deletion lease must use that explicit SHA; if the
remote branch moves, preserve it and report the changed state:

```sh
git worktree remove "$worktree_dir"
git branch -d "$task_branch"
git push --force-with-lease="refs/heads/$task_branch:$remote_task_head" origin ":refs/heads/$task_branch"
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

Verify that the intended task path, disposable external entries, and refs
disappeared, while retained paths remain available. Report removed artifacts,
retained locations and reasons, and any pending cleanup; a removed worktree or
clean Git status alone does not establish complete artifact cleanup. Use
`git worktree remove` for registered worktrees, not directory deletion or
wildcard cleanup; `git worktree prune` only removes stale Git administrative
records and is not a substitute for task cleanup.
