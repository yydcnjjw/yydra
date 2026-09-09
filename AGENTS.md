<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
## Agent skills

### Issue tracker

Issues and specs are tracked in GitHub Issues via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Use the five default canonical triage labels. See `docs/agents/triage-labels.md`.

### Commit conventions

Follow `docs/agents/commits.md` for commit boundaries, English Conventional Commit messages, and PR titles. Keep each commit focused on one logical change, require GitHub-verified cryptographic signatures on submitted commits, and retain the separate author-matching DCO sign-off required by `CONTRIBUTING.md`.

### Domain docs

Use a multi-context layout with a root `CONTEXT-MAP.md` pointing to context-local `CONTEXT.md` files and ADRs. See `docs/agents/domain.md`.

### Worktree development workflow

Follow `docs/agents/worktree-workflow.md` for task isolation, creation, synchronization, handoff, and authorized cleanup. Put every change intended for a commit in a task worktree, defaulting to `.worktree/<slug>` and `codex/<slug>`. Reuse an existing tool-created task worktree when appropriate. Ordinary development ends with reviewed, validated, cryptographically signed local commits carrying DCO sign-offs; continue external actions within the current task's explicit authorization.

### Local development workflow

Follow `docs/agents/development-workflow.md` when selecting local validation and reporting task completion. Select checks by changed behavior; require Android builds for native or Android build-chain changes and full validation before release. Honor explicit task-specific acceptance conditions.
