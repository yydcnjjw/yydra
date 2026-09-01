# Yydra V0 Golden Stack prototype

This branch is a throwaway primary-source artifact for the question:

> Can the selected Yydra V0 stack and contracts work together in a generated
> Product Workspace without leaking Product Domain changes into Framework,
> generated, or native-host ownership?

It is not production implementation and does not promote the candidate to the
Golden Stack.

## Evidence stages

1. Generate a clean Product Workspace offline and atomically.
2. Apply the known-good Reading Queue reference change.
3. Exercise source-generated OpenAPI, the validated Generated Client, H5, and
   the Mechanical Quality Contract.
4. Record Android/iOS build and runtime evidence separately.
5. Run the sealed Agent Eval Campaign.

## Observed result

The core vertical slice works, but the complete V0 claim does not yet pass.

- The final embedded template is 704 KiB, creates offline and atomically, and
  has 41 authored files with template digest
  `fad420ccb216e14f1b79a61d17bb2bcdc7db0e4dd9c4a4a1490d5f88f8490248`.
- The Reading Queue reference validates Product Domain transitions, SQLx/
  PostgreSQL persistence and migration state, Axum code-first OpenAPI,
  Orval/Zod generation behind a Framework façade, H5 E2E, and Android runtime.
- `yydra generate api` closes the CLI/API orchestration gap found during the
  prototype. `yydra check` now executes the Distribution-owned Mechanical
  Quality Contract as Linux, macOS, and aggregate profiles, while preserving
  per-node logs, artifact digests, and explicit non-execution reasons. The
  negative fixture proves four independent failures without fail-fast masking.
- Clean Android and iOS CNG runs are deterministic. Android release build and a
  create/complete/reopen emulator flow pass. iOS build/runtime are not run
  because this WSL2 host has no macOS/Xcode toolchain.
- On this Linux host, both a newly generated Workspace and the Reading Queue
  reference pass every Linux-owned node, including a running PostgreSQL/API
  smoke test, H5 E2E, and Android release assembly. A Linux-only aggregate
  remains fail-closed because iOS is `not-run`; the CI workflow supplies the
  required macOS/Xcode shard before an aggregate can pass.
- The frozen campaign preflight remains `campaign-invalid`: it predates this
  graph, the current host still lacks Xcode, and the hidden acceptance grader
  is not implemented. No Agent Eval outcome is inferred from the quality-graph
  result.

See `prototype/quality-check/README.md` for the current graph contract and
verified result hashes. The earlier `prototype/reading-queue-reference/evidence/`
and `prototype/eval-v0/eval-manifest.json` remain frozen primary-source
evidence from the initial prototype run.
