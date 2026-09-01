# Mechanical Quality negative fixtures

`ownership-and-drift.patch` is applied to a disposable copy of the known-good
Reading Queue reference. A valid checker run must remain non-zero while still
continuing independent nodes, and must report all of:

- `ownership.generated-boundaries` because handwritten source entered the
  Orval-owned directory;
- `ownership.baseline-skills` because a Distribution-owned Skill snapshot was
  edited locally;
- `api.openapi-drift` because committed OpenAPI no longer matches Axum/utoipa
  source;
- `api.generated-client-drift` because committed generated inventory no longer
  matches pinned Orval output.

The prototype execution must report all four failures while Rust, Expo,
advisory, TypeScript, frontend-test, and H5-export nodes continued to pass.

`generation-failure-output.txt` records a separate Orval failure injection for
`yydra generate api`: the command stayed non-zero, both committed artifact
inventories remained byte-identical, and all staging/backup paths were cleaned.
