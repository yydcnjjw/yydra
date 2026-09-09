<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Pull request description conventions

Date: 2026-09-09
Status: research input; the subsequent Yydra format decision is recorded below.

## Question and scope

What do established open-source repositories ask authors to put in pull request
descriptions, and is there a common writing standard that Yydra can adopt?

This note examines six repositories' actual template files, the GitHub template
mechanism and review guidance, Google's change-description guidance, and
Conventional Commits 1.0.0. The sample establishes differences between these
projects; it is not a survey of all open-source practices. A missing field in a
template does not establish that the project's contribution policy omits that
requirement. No project-specific bot behavior was tested.

## Standards and guidance have different scopes

- **Conventional Commits** specifies commit-message syntax and meaning. Its body
  is free-form; it does not prescribe PR description headings. Applying its
  subject format to a PR title is a repository convention. Yydra already makes
  that choice in its [commit conventions](../agents/commits.md).
  [Specification](https://www.conventionalcommits.org/en/v1.0.0/).
- **GitHub PR templates** let repositories choose the information contributors
  should supply. GitHub describes where templates live and how they populate the
  body; this is a template mechanism, not a universal PR-body schema. Its
  documentation distinguishes PR templates from the structured fields available
  in Issue Forms.
  [Template documentation](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/about-issue-and-pull-request-templates).
- **GitHub's review guidance** recommends focused changes, a clear problem,
  approach and result, review pointers where useful, self-review, and related
  Issue links. It supplies writing goals rather than mandatory section names.
  [Review guidance](https://docs.github.com/en/pull-requests/concepts/helping-others-review-your-changes).
- **Google's CL guidance** treats the description as a lasting explanation of
  what changed and why. It recommends an independently useful first line,
  relevant background, decisions and limitations, and a description updated to
  match the final change. Its examples demonstrate different levels of detail;
  they do not establish a GitHub PR form.
  [CL descriptions](https://google.github.io/eng-practices/review/developer/cl-descriptions.html).

The bounded conclusion is that these primary sources do not establish one
universal PR-body standard. They support a project-specific content contract
grounded in review needs.

## Source snapshots

All sources were retrieved on 2026-09-09. Repository metadata was read through
`gh api repos/<owner>/<repo>`. Each default branch's commit was resolved through
`gh api repos/<owner>/<repo>/commits/<branch>`, and template contents were then
read with `gh api repos/<owner>/<repo>/contents/<path>?ref=<commit>`. The links
below pin the template content to those observed commits; branch names are
reported observations and may change later.

| Repository | Observed default branch | Observed commit | Template source |
| --- | --- | --- | --- |
| Kubernetes | `master` | `82b6b6d3b2fc1b9882ea500b54fca32c8d862e68` | [`.github/PULL_REQUEST_TEMPLATE.md`][kubernetes] |
| VS Code | `main` | `0af2bfdddee61954b27fdb831f7a8b20a139126b` | [`.github/pull_request_template.md`][vscode] |
| PyTorch | `main` | `b51534ee807ffed6b56c79afccdd4f67a65be1b0` | [`.github/PULL_REQUEST_TEMPLATE/fix_issue.md`][pytorch-fix], [`preapproved.md`][pytorch-preapproved], [`docs_typo.md`][pytorch-docs] |
| Rust | `main` | `1edd55dcfcd573872c727fa3e086369a71661ee0` | [`.github/pull_request_template.md`][rust] |
| Tauri | `dev` | `7f2b7cb7921caf7d9370b7ebe99e3cf47dbcfed6` | [`.github/PULL_REQUEST_TEMPLATE.md`][tauri] |
| Tokio | `master` | `8388d34f89c61c601651d036eaaac4fb6a24a292` | [`.github/PULL_REQUEST_TEMPLATE.md`][tokio] |

## What the templates actually request

| Repository | Problem and change | Validation | References and review | User impact and project-specific fields |
| --- | --- | --- | --- | --- |
| [Kubernetes][kubernetes] | A combined purpose-and-motivation field | Introductory instructions to add or run appropriate tests; no dedicated test-results heading | Related Issues, optional KEP references, reviewer notes, additional documentation | User-facing changes and a release-note block; `/kind` commands, a documentation block, AI-use disclosure |
| [VS Code][vscode] | Hidden instructions to describe proposed changes; no fixed visible body headings | Explain how to test the changes | Associate an Issue, consult contribution guidance, keep current with `main` | No dedicated compatibility or release-note field in the inspected template |
| [PyTorch][pytorch-fix] | Issue-fix template asks for a brief summary and points design discussion to the Issue | Checklist for lint, tests, documentation where applicable, and benchmarks for performance changes | Issue-fix template expects an Issue; [preapproved template][pytorch-preapproved] asks for the agreeing maintainer and discussion context | Backward compatibility and migration field; [documentation/typo template][pytorch-docs] asks only for a one-sentence change description |
| [Rust][rust] | Hidden instructions, with no prescribed problem or summary heading | No dedicated validation field in this template | Link a tracking Issue when relevant; `r?` reviewer-selection syntax | LLM-policy/disclosure links and `homu-ignore` markers for content excluded from the eventual merge-commit message |
| [Tauri][tauri] | Hidden instructions with concrete Conventional Commit-style title examples | Reminders that `cargo test` and `cargo clippy` pass | Reference a related Issue when one exists; use a draft for unfinished work | Add a `.changes` file when a new version is needed; commit-signature reminder |
| [Tokio][tokio] | Two visible sections separate motivation and solution | Hidden instructions ask for tests with bug fixes and features, and link to the contributor guide for project-specific checks | Link to contribution guidance | No dedicated compatibility, release-note, or bot field in this template |

The inspected VS Code, Rust, and Tauri files primarily provide instructions in
HTML comments instead of imposing a visible body outline. Kubernetes chooses
explicit fields for review and release processing. PyTorch chooses different
templates for different contribution paths. These are concrete differences in
the source files, not evidence that one approach produces better reviews.
[VS Code][vscode], [Rust][rust], [Tauri][tauri], [Kubernetes][kubernetes],
[PyTorch issue fix][pytorch-fix], [PyTorch documentation][pytorch-docs].

Tokio supplies a particularly small explicit outline that still separates the
reason for a change from its implementation. Template reminders about tests or
signatures in any of these repositories do not by themselves prove CI or branch
protection enforcement. [Tokio][tokio], [Tauri][tauri].

## Implications for Yydra

The following recommendations were prepared for the Yydra decision, inferred
from the comparison and Yydra's existing workflow. They did not themselves amend
repository policy; the subsequent decision is recorded separately below.

1. **Define required information before the number of headings.** A reviewer
   should understand the problem, the resulting behavior, why the approach was
   selected, the evidence, and material impact without needing the task chat.
   This combines the purpose emphasized by GitHub and Google with the review and
   impact prompts visible in Kubernetes and PyTorch. Splitting motivation from
   implementation, as Tokio does, is useful when a generic summary tends to
   repeat the diff. [Tokio][tokio].
2. **Use a small common outline and conditional detail.** The candidate has four
   required sections: `Motivation`, `Solution`, `Validation`, and
   `User impact and compatibility`. Add `Reviewer notes` and `References` where
   useful; include follow-up work in reviewer notes with a tracking link. Preserve
   the important information for small changes with short prose; do not require
   invented risks or repetitive text simply to fill a large form. The exact
   headings and optional-section behavior were open during this investigation.
3. **Make validation evidence concrete.** Record executed checks, their outcomes,
   relevant scope and limitations, and evidence links where useful. Generic
   checked boxes cannot by themselves describe what a run establishes. Follow
   Yydra's [local validation policy](../agents/development-workflow.md), including
   the distinction between selected checks and full Conformance Evidence.
4. **Use impact to explain consumer consequences.** Where relevant, describe
   affected Framework APIs, CLI/configuration, Product Workspace contracts,
   compatibility, migration, and meaningful remaining risk. PyTorch's
   compatibility prompt and Kubernetes's user-change prompt are useful models;
   Tauri's separate change-file mechanism does not justify introducing new
   release tooling as part of this writing decision.
5. **Keep references accurate and descriptions current.** Link an existing Issue
   or ADR when useful, distinguish reference from closure, and retain sufficient
   context in the description. Refresh the title, scope, and validation after
   review changes the candidate. Keep the current Yydra allowance for an
   untracked small change instead of adopting PyTorch's Issue-first workflow
   solely because its template uses one.
6. **Keep project machinery separate from the portable writing contract.**
   Kubernetes commands and release blocks, Rust reviewer/bot syntax, Tauri
   change files, and PyTorch preapproval routing support their particular
   workflows. Yydra should add equivalent fields only when it adopts the
   corresponding process. The compared templates also contain differing AI-use
   policies; this research does not select an AI disclosure policy for Yydra.

## Subsequent decision — 2026-09-09

The maintainer confirmed the recommendation: English PR titles under the existing
Conventional Commit policy, English bodies, four required sections (`Motivation`,
`Solution`, `Validation`, `User impact and compatibility`), and two optional
sections (`Reviewer notes`, `References`). This is a Yydra convention informed by
the sources above, not an industry-wide PR-body standard. The
[GitHub workflow](../agents/github-workflow.md) records the content requirements,
and the [PR template](../../.github/pull_request_template.md) supplies the outline.

The format decision does not change PR creation or merge authorization, signing,
DCO, validation gates, or release policy. Template availability on GitHub depends
on its integration into the repository's default branch.

[kubernetes]: https://github.com/kubernetes/kubernetes/blob/82b6b6d3b2fc1b9882ea500b54fca32c8d862e68/.github/PULL_REQUEST_TEMPLATE.md
[vscode]: https://github.com/microsoft/vscode/blob/0af2bfdddee61954b27fdb831f7a8b20a139126b/.github/pull_request_template.md
[pytorch-fix]: https://github.com/pytorch/pytorch/blob/b51534ee807ffed6b56c79afccdd4f67a65be1b0/.github/PULL_REQUEST_TEMPLATE/fix_issue.md
[pytorch-preapproved]: https://github.com/pytorch/pytorch/blob/b51534ee807ffed6b56c79afccdd4f67a65be1b0/.github/PULL_REQUEST_TEMPLATE/preapproved.md
[pytorch-docs]: https://github.com/pytorch/pytorch/blob/b51534ee807ffed6b56c79afccdd4f67a65be1b0/.github/PULL_REQUEST_TEMPLATE/docs_typo.md
[rust]: https://github.com/rust-lang/rust/blob/1edd55dcfcd573872c727fa3e086369a71661ee0/.github/pull_request_template.md
[tauri]: https://github.com/tauri-apps/tauri/blob/7f2b7cb7921caf7d9370b7ebe99e3cf47dbcfed6/.github/PULL_REQUEST_TEMPLATE.md
[tokio]: https://github.com/tokio-rs/tokio/blob/8388d34f89c61c601651d036eaaac4fb6a24a292/.github/PULL_REQUEST_TEMPLATE.md
