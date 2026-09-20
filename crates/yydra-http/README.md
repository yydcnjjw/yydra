<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# yydra-http

Shared RFC 9457 responses and server-generated request correlation for Yydra HTTP
adapters. Product and authentication adapters own error classification and mapping.
Domain, application, CLI and build code do not depend on this crate.

Wrap the fully assembled router with `request_context`. Each request receives an
untrusted-client-independent `requestId`, also returned as `x-request-id` and
attached to the tracing span. `ProblemResponse` uses the same identity. Report
technical failures with static operation/reason codes and a source count; raw
third-party error messages, credentials, URLs, SQL values and response bodies are
never formatted by this library. Expected rejections do not emit error logs.

`technical` retains safe metadata for the final response reporter. Callers must
supply context made from known-safe fields rather than third-party Display or
Debug. Captured SNAFU backtraces are included only when present (enable with
`SNAFU_BACKTRACE=1` or `RUST_BACKTRACE=1`); expected rejections do not capture one.
`violations` contains public `field` and rule `code` pairs. Clients branch on
`type` and structured codes, never on `title` or `detail` prose.
