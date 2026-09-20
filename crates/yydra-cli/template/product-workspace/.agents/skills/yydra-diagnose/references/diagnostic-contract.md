<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Structured diagnostic contract

Doctor emits versioned JSON Lines with `phase`, `code`, `severity`, `status`,
`message`, `location`, and `remediation`. It is read-only and observes actual
tool versions. Required `fail` results cause a nonzero exit; optional `warning`
results do not. All required probes are reported before `doctor.summary`.
Cancellation is a failure, including during optional probes.

A missing frontend installation is a warning because doctor can run before
setup. Optional Docker/Compose diagnostics describe the local container path;
external PostgreSQL remains supported. Android is explicitly selected and checks
existing JDK/SDK components. It does not install SDK packages or generate a host.

`pass` proves only the reported identity/environment observations. It does not
prove dependency resolution, successful builds, application behavior, or test
coverage. Retain the output from the actual build/test command for those claims.
The old check graph, node results, and aggregate manifests are retired.

JSONL schema 2 adds `reasonCode` to every event (null on ordinary progress).
`code` identifies the execution step; `reasonCode` identifies the typed failure.
For example, a failed `SETUP_CARGO_FETCH` may have `IO_NOT_FOUND`, while a child
exit is `PROCESS_EXIT_FAILED`. Branch on the reason, not on `message` prose.
Schema 1 consumers must upgrade. Child tool output is forwarded on stderr;
stdout contains only Yydra events in JSON mode. Doctor's summary aggregates its
individually reported observations, rather than granting build conformance.
