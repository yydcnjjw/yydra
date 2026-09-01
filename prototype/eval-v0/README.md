# V0 Agent Eval prototype

This directory freezes the Reading Queue task, qualitative rubric, and campaign
preflight owned by the prototype. It is evidence, not a successful campaign.

The preflight runs before any Agent slot. It rejects a campaign unless both the
clean starting Workspace and known-good reference pass an aggregate Mechanical
Quality Contract and the grader host can execute every required platform node.
An invalid preflight launches no with-Skills or control runs, because those
outcomes could not be graded as Agent capability.

The 2026-09-01 preflight ended `campaign-invalid`: the current CLI reports only
`pass-local-applicable` and explicitly leaves database, H5 E2E, Android release,
and iOS build nodes unexecuted; the Linux/WSL2 grader also has no Xcode, and no
hidden acceptance grader can yet prove implementation-independent task
behavior. The planned 3+3 run order is retained in `eval-manifest.json`, with
every slot marked `not-started`.

This paragraph and `eval-manifest.json` are frozen evidence from the initial
prototype run. The subsequent aggregate graph implementation is documented in
`prototype/quality-check/README.md`; it does not retroactively change the
campaign result or start any Agent slot.

Run the frozen preflight with:

```sh
prototype/eval-v0/preflight.sh \
  target/debug/yydra \
  prototype-runs/final-blank-i \
  prototype/reading-queue-reference \
  prototype/eval-v0/evidence
```
