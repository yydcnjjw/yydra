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

## GitHub development workflow

Follow the [GitHub development workflow](docs/agents/github-workflow.md) for the
PR, review, and merge policy. Every change to `main` goes through a PR,
including documentation and small fixes. Require DCO and CLI build/test CI success
before an authorized merge and use Squash and merge by default. A request to
merge a specific PR includes its verified task cleanup unless retention is
requested. Local completion, PR merge, and cleanup remain distinct reported
states; Distribution publication requires its own authorization.

Write PR bodies in English using the [PR template](.github/pull_request_template.md):
`Motivation`, `Solution`, `Validation`, and `User impact and compatibility` are
required; `Reviewer notes` and `References` are included when useful. Follow the
GitHub workflow's content guidance for small changes, evidence, and optional fields.

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
