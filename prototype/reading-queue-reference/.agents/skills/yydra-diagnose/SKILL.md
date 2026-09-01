---
name: yydra-diagnose
description: Diagnose Yydra Product Workspace doctor/check failures and choose safe remediation without editing Distribution snapshots, generated authorities, migrations, or quality gates. Use when doctor, generation, or yydra check reports a failure or not-run node.
metadata:
  yydra-distribution: "0.0.2-prototype"
---

# Diagnose a Yydra Workspace

Preserve the failing evidence, identify the first owning boundary, and make only
the narrowest Product-owned repair.

## Diagnose

1. Run `yydra doctor .`. If it reports an exact-Distribution mismatch, stop.
   Do not edit the Workspace Origin Record or add a compatibility override.
2. Run the smallest failing command directly when the reported node names one,
   then rerun `yydra check .` after a repair.
3. Read [references/rule-routing.md](references/rule-routing.md) for the stable
   rule owner and safe remediation.
4. Distinguish `FAIL` from `not-run`. A missing database, browser, Android SDK,
   macOS/Xcode host, credential, or other prerequisite is not a pass and is not
   automatically a product defect.

## Stop boundaries

Never repair by changing `.agents/skills/`, `.yydra/origin.toml`, an existing
migration, committed generated output by hand, CNG native output, the check
implementation, CI gates, or an exception list. Do not run forced dependency
updates or install a different Distribution implicitly. Explain the required
authority or environment instead.

Finish with the failing rule IDs, root cause, changed Product-owned files,
rerun results, and remaining `not-run` evidence.
