<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Contributing to Yydra

Yydra-authored contributions are accepted under the same exact dual-license
terms as the Distribution: `MIT OR Apache-2.0`. Copied third-party material
must retain its original license terms, notices, and provenance.

## Worktree development workflow

Follow the [worktree development workflow](docs/agents/worktree-workflow.md)
for all repository changes intended for a commit, including documentation and
configuration. Use one task branch and worktree per independently mergeable
change, defaulting to `.worktree/<slug>` and `codex/<slug>`, and preserve the
primary worktree on `main`. The workflow covers validation, synchronization,
local completion, PR handoff, and cleanup within the task's authorization.

## Commit conventions

Follow the [commit conventions](docs/agents/commits.md) for human and agent
contributions to this repository. Use English Conventional Commit messages,
keep each commit focused on one logical change, and use the same subject
format for PR titles. The policy defines type and scope, Issue references,
breaking-change notes, and the merge-subject exception. Apply it through
contributor guidance and review alongside the existing DCO check below.

## Verified commit signatures

Every submitted commit must have a cryptographic signature that GitHub verifies.
Use `git commit -S --signoff` with a configured signing identity and check GitHub's
verification status for the full PR commit range after an authorized push.
The DCO `Signed-off-by:` trailer is a separate requirement; it does not sign
the commit cryptographically. See the [commit conventions](docs/agents/commits.md)
for signing, rewritten commits, and supported GitHub-signed commits.

## Developer Certificate of Origin

Every submitted commit must certify the
[Developer Certificate of Origin 1.1](https://developercertificate.org/). Add
a sign-off matching the commit author identity:

```text
Signed-off-by: Your Name <your.email@example.com>
```

Git can add the trailer with `git commit -S --signoff` while also signing the
commit. The pull-request DCO check evaluates
every commit in the submitted base-to-head range and rejects a missing or
non-matching sign-off.

Yydra V0 does not use a Contributor License Agreement (CLA).
