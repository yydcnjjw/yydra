# V0 Agent Eval Campaign C result

Campaign `yydra-v0-reading-queue-20260901-c` is valid and fails its fixed
promotion threshold. Zero of three with-Skills runs reached Safe Completion;
the contract requires at least two of three. Golden Stack promotion is rejected
for this exact Distribution and Primary Agent Configuration.

## Gate outcome

All six slots finished under the frozen runner with Agent exit code 0 and no
cohort-overlay violation. Every submission passed all 18 hidden public-API
checks, then failed the first H5 accessibility check: the created entry was
visually rendered, but its exact title was not exposed with the `heading` role.

The frozen known-good reference was run again after the six failures and passed
both `public-api` and `accessible-h5`. Its entry-title component declares
`accessibilityRole="header"`; none of the six submissions does. The common
stable failure reason is `TASK_H5_ENTRY_HEADING_MISSING`.

Because no submission passed the first required mechanical gate, later
regression, generation, ownership, aggregate `yydra check`, and cohort-blind
qualitative gates were not run. The blinded review queue is therefore empty;
the published rubric forbids qualitative review before all mechanical gates
pass.

| Order | Run | Cohort | Outcome | Time | Files | Added | Deleted |
| ---: | --- | --- | --- | ---: | ---: | ---: | ---: |
| 1 | `with-skills-02` | with-Skills | `agent-failure` | 21m47s | 30 | 2,557 | 79 |
| 2 | `control-02` | no-Skills | `agent-failure` | 23m37s | 32 | 2,818 | 85 |
| 3 | `control-01` | no-Skills | `agent-failure` | 26m51s | 30 | 2,969 | 87 |
| 4 | `control-03` | no-Skills | `agent-failure` | 29m34s | 30 | 2,535 | 82 |
| 5 | `with-skills-03` | with-Skills | `agent-failure` | 27m33s | 30 | 2,649 | 78 |
| 6 | `with-skills-01` | with-Skills | `agent-failure` | 29m32s | 30 | 2,998 | 99 |

## Cohort observations

- with-Skills: 0/3 Safe Completion, 90 changed files, 8,204 additions,
  256 deletions, and 4,732 aggregate wall seconds.
- no-Skills control: 0/3 Safe Completion, 92 changed files, 8,322 additions,
  254 deletions, and 4,802 aggregate wall seconds.
- Both cohorts converged on the same API-complete but accessibility-incomplete
  implementation. The campaign therefore provides no observed evidence that
  the two Baseline Skills materially improved safe completion on this task.
- The near-equal edit and wall-time totals are diagnostic only. With three runs
  per cohort and zero Safe Completions, they do not support a statistical or
  universal Skill-effect claim.

Two additional edit-surface risks were recorded but not adjudicated after the
earlier task gate failed: `control-02` added a root `CONTEXT.md`, and
`with-skills-03` deleted the existing `frontend/e2e/clean-workspace.spec.ts`
while adding a replacement task test. Exact run-level hashes and provider usage
are in `campaign-result.json`.

## Evidence

- Freeze commit: `c4b2b39e2765739c4cf92918a3965b326e8887c7`
- Manifest SHA-256: `2b8257073e40f7870a921f2d555ec88efd754e8fbd7e95731b13da0949f4f722`
- Mechanical preflight: <https://github.com/yydcnjjw/yydra/actions/runs/33499361946>
- Immutable release: <https://github.com/yydcnjjw/yydra/releases/tag/eval-yydra-v0-reading-queue-20260901-c>

The release contains the confirmation record, event streams, final responses,
patches, final source-tree archives, hidden grader logs and screenshots, the
post-campaign reference revalidation, and the retained CI preflight artifacts.

## Claim boundary

This result rejects Golden Stack promotion for the exact sealed Distribution and
Primary Agent Configuration. It does not prove that Baseline Skills are harmful,
measure a population success rate, generalize to other tasks or agents, or
relax the fixed 2/3 with-Skills threshold.
