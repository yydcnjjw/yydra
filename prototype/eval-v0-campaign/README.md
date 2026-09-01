# V0 Agent Eval campaign prototype

This directory is the current freeze candidate for the next Reading Queue campaign.
It does not overwrite `prototype/eval-v0/`, which remains the immutable evidence
for the first `campaign-invalid` preflight.

No scored Agent slot may start until all of these conditions hold:

1. `task.md`, `rubric.md`, the independent grader, the exact Distribution,
   template, dependency locks, Baseline Skills, Primary Agent Configuration,
   cohort overlay, run order, timeouts, and network policy have recorded hashes.
2. A fresh clean Workspace passes the pre-existing Mechanical Quality Contract
   but fails the new task acceptance.
3. The known-good reference passes the same Mechanical Quality Contract and the
   independent hidden acceptance grader.
4. Clean and reference Linux/macOS shards merge to aggregate `pass` on the
   supported GitHub-hosted grader.
5. The human confirms this freeze candidate. Any later change creates a new
   campaign ID and repeats the complete preflight.

The slot launcher independently verifies the sealed hashes, a clean repository,
the authorized freeze commit, and the manifest hash before exposing the task to
an Agent. The authorization record stays outside the repository so confirmation
does not mutate the frozen commit.

The hidden grader consumes only the public wire and accessibility contract in
`task.md`. It is kept outside every Agent Workspace and does not import the
known-good implementation or assert its file layout, Rust symbols, SQL schema,
or frontend component structure.

The no-Skills cohort uses one declared environment overlay: the exact Baseline
Skill directory is absent while the Agent runs, then the harness restores the
unchanged Distribution snapshot before mechanical grading. The Agent sees a
clean Git baseline for that overlay, and the grader excludes the exact restore
from Product edit metrics. No visible instruction refers to a missing Skill.

Local preflight usage:

```sh
prototype/eval-v0-campaign/preflight.sh \
  target/release/yydra \
  prototype/reading-queue-reference \
  target/eval-v0-campaign/preflight
```

This local phase checks the seal inputs, clean/reference origin, and hidden
acceptance discrimination. Aggregate macOS/iOS validity remains a separate,
required CI preflight; local Linux success is never promoted to campaign
readiness.
