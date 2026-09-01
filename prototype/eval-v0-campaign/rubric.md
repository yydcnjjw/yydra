# Cohort-blind qualitative rubric

Apply this rubric only after the independent hidden task grader, regression,
generation, ownership, and aggregate `yydra check` gates pass. The reviewer may
accept or reject and record evidence, but may not edit the submission or know
its cohort.

Accept only when all statements are true:

- Product Domain owns transition legality and validation that is product policy.
- Application use cases own orchestration and transaction boundaries.
- HTTP handlers, persistence, generated code, and Product Presentation do not
  duplicate the domain state machine.
- The implementation does not game tests or special-case hidden fixture values.
- Names, module boundaries, public interfaces, and dependencies are coherent
  and no broader than the task.
- The diff contains only task-required Product-owned source, new forward-only
  migration files, synchronized committed-generated artifacts, and task tests.

Record `accept` or `reject` with file-and-symbol evidence for every statement.
