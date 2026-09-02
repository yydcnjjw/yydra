# Yydra Distribution 0.0.3 freeze contract

Distribution `0.0.3` is the corrected exact V0 Distribution selected after the
negative Reading Queue campaign. It integrates one conjunctive Product
Presentation accessibility contract:

1. exact-Distribution Baseline Skill routing;
2. ordinary Product-owned React Native source and Playwright semantics specs;
3. the required `h5.product-presentation-accessibility` node under
   `yydra check .`.

The correction does not add a Framework UI wrapper, product-semantics DSL,
hidden-grader helper, compatibility override, or Product Workspace upgrade
path. Product meaning remains in Product Presentation source and its visible
spec.

## Frozen identities

- CLI Distribution version: `0.0.3`.
- Mechanical Quality evidence schema: `2`.
- Check catalog SHA-256:
  `dd53cbea7b50518805e6e74b005b04c5dcdc92ac6a369bcb9769ad375288a277`.
- Expo-compatible exact patch set is recorded in both template/reference npm
  manifests and lockfiles. `expo install --check` must pass; no compatibility
  override is accepted.
- Linux CI pins a 4 GiB Gradle heap and at most two workers so D8 dex merging
  is not subject to the runner's smaller generated default heap.
- Template, Baseline Skill, Workspace Origin Record, Cargo/npm locks, clean and
  reference inputs, workflow, CLI, fixture, aggregate, and native-runtime
  identities are recorded by `freeze-distribution` in
  `distribution-manifest.json`.

The template SHA-256 is derived by the exact CLI and written into every
generated Workspace Origin Record. It is not hand-copied into this document;
the freeze manifest and aggregate manifests are authoritative.

## Verification

Run the local contract tests with:

```sh
cargo fmt --all --check
cargo test --locked --workspace --all-features
distribution/v0.0.3/test-freeze
```

The `V0 Mechanical Quality Contract` workflow then requires, for the same exact
commit and Distribution inputs:

- clean and Reading Queue Linux host shards;
- clean and Reading Queue macOS host shards;
- fail-closed aggregate passes for both fixtures;
- a fresh Reading Queue iOS simulator lifecycle;
- heading-role-removal discrimination in which the semantic node fails while
  the pre-existing behavior flow and Android release still pass;
- one verified `distribution-v0.0.3-freeze` artifact.

## Claim boundary

An aggregate pass establishes only the named Mechanical Quality nodes for the
frozen clean/reference inputs. The H5 semantic node establishes only the
role/name/state assertions registered in the visible Product-owned spec.
Physical-device behavior, native assistive-technology accessibility, a fresh
Agent Eval, Baseline Skill effect, and Golden Stack promotion remain `not-run`.
No Agent Eval slot belongs to this Distribution-build task.
