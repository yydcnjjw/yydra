# Throwaway Product Presentation accessibility-contract prototype

This prototype answers [Prototype an enforceable Product Presentation
accessibility contract](https://github.com/yydcnjjw/yydra/issues/22). It is
decision evidence, not production implementation and not a promoted Yydra
Distribution.

## Question and failure reproduced

Campaign C produced six visually complete entry titles that were absent from the
browser accessibility tree as headings. The known-good source already used
direct React Native `accessibilityRole="header"`, but the visible Product-owned
Playwright flow did not assert that semantic requirement. The Baseline Skill
said only to build accessible presentation, and the Mechanical Quality Contract
reported only the broad `h5.e2e` node.

The decisive mutation for this prototype removes only the entry title's
`accessibilityRole="header"`:

- ordinary rendering and the pre-existing behavior flow remain valid;
- the new visible Product-owned semantics spec fails by role and accessible
  name;
- the new required `h5.product-presentation-accessibility` node records the
  failure before any sealed Agent Eval.

## Compared intervention surfaces

| Surface | What it contributes | Why it is insufficient alone |
| --- | --- | --- |
| Baseline Skill guidance | Routes Coding Agents through requirement, semantic assertion, direct React Native implementation, and focused check | Prose cannot fail closed and is absent from the no-Skills cohort |
| Ordinary source/test pattern | Demonstrates direct Supported Golden Stack Surface usage and role/name/state assertions without a Framework UI wrapper | A passive example may be omitted, deleted, or weakened |
| Visible task-independent check seam | Makes one canonical Product-owned semantic spec required, retry-free, deletion-resistant, and evidence-producing under `yydra check` | The runner cannot infer an unstated product meaning without becoming a Product semantics DSL |

The smallest coherent correction is therefore conjunctive: Skill routing plus
an ordinary source/test pattern plus the Distribution-owned check node. The
external interface remains `yydra check .`; the focused npm command is only a
repair loop. Product meaning stays in Product Presentation and ordinary
Playwright source.

## Prototype interface

- Product Presentation uses direct React Native `accessibilityRole`,
  `accessibilityLabel`, `accessibilityState`, and visible child content.
- Product-owned semantic assertions live in exactly
  `frontend/e2e/product-presentation.accessibility.spec.ts`.
- `npm run test:product-semantics` gives focused local feedback.
- `yydra check .` reports the required
  `h5.product-presentation-accessibility` node and keeps hidden grading
  independent.

The node starts the same local PostgreSQL/backend/Expo H5 stack as ordinary E2E,
invokes pinned Playwright directly with zero retries and `--forbid-only`, stores
a JSON report, and rejects a missing spec, zero executed tests, skipped tests,
or any failed assertion. It does not import a hidden grader or known-good
implementation.

## Run

Open `contract-demo.html` directly for the free-play decision model and guided
walkthroughs. The repository prototype is exercised with:

```sh
cargo test --locked --workspace --all-features
cargo run --locked --bin yydra -- new <empty-directory> \
  --product-name "Accessibility Probe" --product-id accessibility-probe

cd <generated-workspace>/frontend
npm ci --ignore-scripts
npm exec -- playwright install chromium
npm run test:product-semantics
```

The complete node additionally needs the Product Workspace's PostgreSQL-backed
stack and runs through `yydra check`.

## Prototype results (2026-09-02)

- CLI report classification: 3/3 Rust tests passed, covering catalog identity
  plus passing, zero-executed, skipped, and failed Playwright statistics.
- Generated clean Product Workspace: the canonical semantic spec passed 1/1 on
  a real Expo H5 surface in about 2.3 minutes.
- Reading Queue positive fixture: exact origin, Baseline Skill inventory,
  ownership, Rust, API drift, frontend type/test/audit, H5 export, PostgreSQL,
  `h5.product-presentation-accessibility`, the pre-existing `h5.e2e`, Android
  release, and authored-input immutability all passed. The semantic node passed
  1/1 in 139.7 seconds.
- Heading-only mutation: removing only the dynamic entry title's
  `accessibilityRole="header"` left the pre-existing create/filter/complete/
  reopen behavior flow passing 1/1, while the canonical semantic spec failed
  1/1 at the exact heading role/name locator. The positive source was restored
  after the mutation.

The Linux host result is deliberately **not** called aggregate conformance. Its
overall status remained `fail` because the live Expo compatibility matrix had
advanced several pinned `57.0.x` patch expectations since the sealed 0.0.2
campaign; iOS was correctly `not-run` on Linux. That independent dependency
drift must be resolved and the complete Linux/macOS aggregate rerun before a new
Distribution can be frozen.

The ignored local positive manifest and semantic report were respectively
SHA-256 `9b0682c2e98ea14cf46eacd29e34776e778b71c47bbc6599e56df79893229be1`
and `a7394bca29af3d401662fd9f81bb3729bb48c03cc9b386545b24929ab1b486f0`.
The committed `evidence/results.json` is a reproducible summary, not a substitute
for retaining full freeze evidence in a later campaign.

## Evidence boundary

A pass proves only that the visible Product-owned role/name/state assertions ran
and passed against the H5 Application Surface. It does not prove:

- complete WCAG conformance;
- Android or iOS assistive-technology behavior;
- that every product meaning was registered in the visible spec;
- hidden task acceptance, Safe Completion, or a positive Baseline Skill effect.

React Native documents `heading` as the role for a content-section header, and
Playwright recommends role locators because they reflect how users and
assistive technology perceive a page. Playwright also says role locators do not
replace full accessibility audits. W3C likewise treats a heading as structural
semantics, not visual styling:

- <https://reactnative.dev/docs/accessibility.html>
- <https://playwright.dev/docs/locators>
- <https://www.w3.org/WAI/ARIA/apg/practices/names-and-descriptions/>

## Candidate next exact Distribution

If the human accepts this seam, the next exact Distribution should freeze the
corrected CLI/check-node implementation, template digest, package locks,
Baseline Skill inventory, clean/reference positive evidence, role-removal and
missing/empty/skipped-spec negative evidence, and clean/reference aggregate
Mechanical Quality Contract evidence.

Only after that evidence exists should a fresh campaign be specified. It must
freeze a new unexposed task and independent grader, clarified public semantic
requirements, known-good reference, manifest and hashes, unchanged Primary
Agent Configuration, randomized 3 with-Skills plus 3 no-Skills structure, and
the unchanged 2/3 with-Skills Safe Completion threshold. Campaign C remains
valid negative evidence and is never reinterpreted.
