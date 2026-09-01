# V0 Mechanical Quality Contract

Distribution `0.0.2-prototype` exposes one read-only quality interface:

```sh
yydra check <workspace> --profile local|linux-ci|macos-ci|aggregate \
  --evidence-dir <new-directory> [--shard <manifest> ...]
```

The graph validates the exact Workspace Origin Record and Distribution-owned
boundaries before exercising Rust, source-generated OpenAPI, the Orval client,
Expo dependencies, advisories, type checks, tests, production H5 export,
PostgreSQL-backed live API behavior, H5 E2E, Android release assembly, and iOS
simulator release assembly. Each node records a stable identifier, normalized
commands, platform applicability, status, raw log digest, output artifact
digests, duration, and a failure or non-execution reason. The graph also checks
that its CNG and generation steps leave authored inputs unchanged.

Host shards intentionally report `pass-applicable`, never aggregate `pass`.
Only the aggregate profile can report `pass`, and it does so only when every
required node has a passing result for the exact same Distribution, template,
and Workspace input digest. A missing, skipped, failed, or infrastructure-error
node prevents aggregate completion. Shard failures take precedence over passes
when evidence is merged.

## Reproducible fixtures

Both fixtures use template digest
`fad420ccb216e14f1b79a61d17bb2bcdc7db0e4dd9c4a4a1490d5f88f8490248`:

- `clean`: a fresh Workspace created locally by the exact release CLI.
- `reference`: `prototype/reading-queue-reference`, including ten live API
  assertions and the full Reading Queue H5 flow.

The final Linux runs produced:

| Fixture | Linux result SHA-256 | Live API SHA-256 | H5 E2E SHA-256 | Android APK SHA-256 |
| --- | --- | --- | --- | --- |
| clean | `75d915aa1066ed52c5b643c46025e8155f63215d86fe38001994830d4768ebf9` | `7833bde4a2e817589d448658dd9ba5ad1b52d6a2725ef130955acb762bfe0ad6` | `f278b20a9b07e190871a1920b61fe7f38857c1afa599006e52e4f96fe6d0c530` | `14e15fd66a33bae5aa098d7d44992725acb957c61b19ee837e97f04cf72735e0` |
| reference | `aed4d0aa4beda24e3472f2de0e18b8cac62b2c26eb9538921bd535e24ea2f99a` | `d6849d814e53c040015939949feb424146b450237cface02ae089f0b65bc1327` | `b70ee103999f617ea3d8f514d0afaef32ddb037c91fdae81be7b9dbbe78916af` | `af493a7777c356c9ebec341290640c3e66fe2c486e0981c91e987123cb48a25a` |

All Linux-owned nodes passed for both fixtures. `ios.simulator-release` was
explicitly `not-run` because the verification host is Linux. Merging each Linux
manifest without a macOS shard exited nonzero and produced `status=fail`,
`aggregateComplete=false`; iOS was the only non-passing node. The clean and
reference Linux-only aggregate result digests were respectively
`ef22c3b0ce37d8230ca1bb50f816fd0c21fc887037b3e2a2b91e92432317016a`
and
`03c14206d9267c981b9552a0bb95c4da39fecc29065607cab04db92bc34a230c`.

Raw manifests, JSONL diagnostics, logs, and artifacts remain in the ignored
local `target/quality-results/` tree. They are reproducible runtime evidence,
not committed binaries.

## CI boundary

`.github/workflows/v0-mechanical-quality.yml` runs clean/reference Linux and
macOS shards independently and uploads their evidence even on failure. The
aggregate jobs download both shards and invoke the same CLI merge. Toolchain
versions and actions are pinned; the macOS shard selects a named Xcode image and
records iOS build output before checking authored-input stability.

No local aggregate pass is claimed. Actual macOS build and runtime validation
remains the responsibility of the dedicated iOS validation ticket; this graph
provides the fail-closed node and CI entrypoint that consumes that evidence.
