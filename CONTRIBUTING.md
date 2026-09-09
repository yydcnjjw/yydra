<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Contributing to Yydra

Yydra-authored contributions are accepted under the same exact dual-license
terms as the Distribution: `MIT OR Apache-2.0`. Copied third-party material
must retain its original license terms, notices, and provenance.

## Commit conventions

Follow the [commit conventions](docs/agents/commits.md) for human and agent
contributions to this repository. Use English Conventional Commit messages,
keep each commit focused on one logical change, and use the same subject
format for PR titles. The policy defines type and scope, Issue references,
breaking-change notes, and the merge-subject exception. Apply it through
contributor guidance and review alongside the existing DCO check below.

## Developer Certificate of Origin

Every submitted commit must certify the
[Developer Certificate of Origin 1.1](https://developercertificate.org/). Add
a sign-off matching the commit author identity:

```text
Signed-off-by: Your Name <your.email@example.com>
```

Git can add it with `git commit --signoff`. The pull-request check evaluates
every commit in the submitted base-to-head range and rejects a missing or
non-matching sign-off.

Yydra V0 does not use a Contributor License Agreement (CLA).
