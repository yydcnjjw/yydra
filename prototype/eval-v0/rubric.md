# Cohort-blind qualitative rubric

Apply this rubric only after all hidden task, regression, generation, ownership,
and aggregate `yydra check` gates pass. The reviewer may accept or reject and
record evidence, but may not edit the submission.

Accept only when all statements are true:

- Product Domain owns transition legality and validation that is product policy.
- Application use cases own orchestration and transaction boundaries.
- HTTP handlers, persistence, generated code, and Product Presentation do not
  duplicate the domain state machine.
- The implementation does not game tests or special-case hidden fixture values.
- Names, module boundaries, public interfaces, and dependencies are coherent
  and no broader than the task.
- The diff contains only task-required Product-owned source, one or more new
  forward-only migrations, synchronized committed-generated artifacts, and
  task tests.

Record an accept/reject result with file-and-symbol evidence for every statement.
