# Cross-layer change loop

Keep one rule in one owning layer and make the other layers translate or invoke
it.

1. Specify Product Domain behavior and illegal transitions with domain tests.
2. Add typed Application use cases. Application owns transactions and invokes
   domain behavior; transport does not open business transactions.
3. Add a new migration and SQLx persistence implementation. Preserve query
   semantics explicitly, including deterministic ordering and cursor tie-breaks.
4. Expose the use case from Axum source. Map failures to stable
   `application/problem+json` responses without leaking storage details.
5. Run `yydra generate api .`; review the OpenAPI and generated-client diff as
   generated evidence, not authored code.
6. Extend the Framework API façade so it validates success and Problem payloads
   before Product Presentation consumes them.
7. Build accessible H5/native-shared Product Presentation. The UI may choose
   interaction policy but must not reimplement domain legality.
8. Cover domain, persistence/API, façade, presentation, and regression behavior
   in the smallest relevant tests, then run `yydra check .`.

If generation, SQLx metadata, a database service, H5 E2E, or a native toolchain
is unavailable, retain the failure or `not-run` evidence and report the exact
boundary. Do not hand-author the missing artifact.
