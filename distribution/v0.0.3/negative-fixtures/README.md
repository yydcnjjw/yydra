# Product Presentation accessibility negative fixtures

These exact fixtures exercise the Distribution-owned
`h5.product-presentation-accessibility` seam without exposing or importing any
Agent Eval grader asset.

| Fixture | Stable expected diagnostic or discrimination |
| --- | --- |
| `missing-spec.patch` | `ACCESSIBILITY_SPEC_MISSING` |
| `report-zero-executed.json` | `ACCESSIBILITY_NO_EXECUTED_TESTS` |
| `report-skipped.json` | `ACCESSIBILITY_FOCUSED_OR_SKIPPED` |
| `report-focused.json` | `ACCESSIBILITY_FOCUSED_OR_SKIPPED` |
| `report-failed.json` | `ACCESSIBILITY_ASSERTION_FAILED` |
| `heading-role-removal.patch` | semantic node fails while the pre-existing behavior flow passes |

`report-passing.json` is the positive classifier control. The clean generated
Workspace and Reading Queue reference remain the end-to-end positive fixtures.
