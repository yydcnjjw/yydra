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

- The final embedded template is 676 KiB, creates offline and atomically, and
  produced two byte-identical 37-file Workspaces with template digest
  `98e2cfd04a4cdddd22fef12c497b1a291282cb78041ca47c56b74e9654810cee`.
- The Reading Queue reference validates Product Domain transitions, SQLx/
  PostgreSQL persistence and migration state, Axum code-first OpenAPI,
  Orval/Zod generation behind a Framework façade, H5 E2E, and Android runtime.
- `yydra generate api` closes the CLI/API orchestration gap found during the
  prototype. `yydra check` now protects Baseline Skill snapshots, `.ts`/`.tsx`
  generated boundaries, and source/generated drift; the negative fixture
  proves four independent failures without fail-fast masking.
- Clean Android and iOS CNG runs are deterministic. Android release build and a
  create/complete/reopen emulator flow pass. iOS build/runtime are not run
  because this WSL2 host has no macOS/Xcode toolchain.
- The campaign preflight returns `campaign-invalid` before any Agent slot: the
  current check graph is local-applicable rather than aggregate, required
  database/H5 E2E/native nodes remain `not-run`, the grader host lacks Xcode,
  and the hidden acceptance grader is not implemented.

See `prototype/reading-queue-reference/evidence/` and
`prototype/eval-v0/eval-manifest.json` for frozen primary-source evidence.
