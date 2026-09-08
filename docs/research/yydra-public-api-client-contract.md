<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Yydra V0 public API and client contract

_Research date: 2026-08-27. Scope: current official specifications, official project documentation, and official source repositories only. Target stack: Rust/Axum/SQLx backend and React Native/Expo/React Native Web frontend._

## Executive finding

**Verified facts:** OpenAPI is designed to describe an HTTP API's surface and semantics in a language-neutral form, and the description can drive documentation, server/client generation, and testing. The specification requires a unique `operationId` when one is supplied, which gives generators a stable operation identity. ([OpenAPI 3.1.1 introduction and Operation Object](https://spec.openapis.org/oas/v3.1.1.html#operation-object))

**Verified facts:** The direct spec-first Rust route exists, but its maturity is the main constraint: OpenAPI Generator labels `rust-axum` a **Beta** server generator. It can validate generated request handling for headers, paths, queries, and bodies, but its published feature matrix has material gaps; OpenAPI Generator also labels OpenAPI 3.1 support itself Beta. ([`rust-axum` generator](https://openapi-generator.tech/docs/generators/rust-axum/), [OpenAPI Generator compatibility](https://github.com/OpenAPITools/openapi-generator#readme))

**Verified facts:** The frontend route is stronger. React Native provides Fetch, and Expo currently supplies a WinterCG-compliant `expo/fetch` across web and mobile. Orval can generate a Fetch-based TypeScript client, inject a runtime `fetch`, generate Zod schemas, and validate eligible JSON responses with `Schema.parse()`. ([React Native networking](https://reactnative.dev/docs/network), [Expo Fetch](https://docs.expo.dev/versions/latest/sdk/expo/#expofetch-api), [Orval Fetch and runtime validation](https://orval.dev/docs/reference/configuration/output/#runtime-validation-support-matrix))

**Inference for Yydra:** OpenAPI-first is credible as the V0 public contract only if “authoritative” means one reviewed specification in the Product Workspace plus mechanical gates. It should not mean “the current Axum generator makes implementation drift impossible.” The least speculative V0 path is a hand-written Axum transport adapter constrained by the canonical OpenAPI description, a generated Fetch client with runtime response validation, and contract tests against the running service. Generating the Axum server trait/router is a stronger spec-first option, but should remain prototype-gated because both the generator and its OpenAPI 3.1 path are explicitly Beta.

## 1. What OpenAPI can and cannot authoritatively define

### Verified facts

- The OpenAPI Description formally describes the API surface and semantics and can feed code generators and testing tools. It covers paths, operations, parameters, request bodies, responses, headers, content types, schemas, and security requirements. ([OpenAPI 3.1.1](https://spec.openapis.org/oas/v3.1.1.html))
- `openapi` identifies the OpenAPI language version and is distinct from `info.version`; therefore neither field alone establishes a Yydra compatibility policy. ([OpenAPI Object](https://spec.openapis.org/oas/v3.1.1.html#openapi-object), [Info Object](https://spec.openapis.org/oas/v3.1.1.html#info-object))
- `operationId`, when present, must be unique across the description, is case-sensitive, and may be used by tools to identify an operation. ([Operation Object](https://spec.openapis.org/oas/v3.1.1.html#operation-object))
- OpenAPI 3.2 is now published, but OpenAPI Generator's own compatibility statement still calls 3.1 support Beta, and `rust-axum` is independently Beta. ([OpenAPI 3.2](https://spec.openapis.org/oas/v3.2.0.html), [OpenAPI Generator](https://github.com/OpenAPITools/openapi-generator#readme), [`rust-axum`](https://openapi-generator.tech/docs/generators/rust-axum/))

### Inference for Yydra

The repository—not the OpenAPI standard—must declare which artifact is authoritative. A coherent OpenAPI-first rule is:

1. one source OpenAPI description is edited and reviewed;
2. generated Rust/TypeScript/docs artifacts are outputs and never independent authorities;
3. every operation has a stable, intentional `operationId`;
4. every observable status, media type, response header, error shape, security requirement, and pagination shape is described; and
5. CI checks the description, regenerated outputs, compatibility, and running implementation separately.

OpenAPI does not encode every behavioral invariant, transaction rule, authorization rule, or Product Domain state transition in a machine-checkable way. Those remain prose plus tests or Product Domain code. Calling the OpenAPI document authoritative should therefore mean “the complete public wire contract,” not “the complete application behavior.”

For V0, selecting an OpenAPI 3.1 feature subset is less risky than assuming all 3.1 or 3.2 JSON Schema constructs survive every Rust and TypeScript generator. The exact version and schema constructs need a small cross-generator fixture test before promotion.

## 2. Axum-side generation and validation options

### 2.1 Spec-first generated server trait/router

**Verified facts:** OpenAPI Generator's `rust-axum` target generates an Axum server library. Its documented default enables validation of request data in headers, paths, queries, and bodies; the option is named `disableValidator` and defaults to `false`. The same page marks the generator Beta and publishes unsupported or partial OAS features, including gaps in polymorphism, OpenID Connect, OAuth flows, callbacks, and Link Objects. ([`rust-axum` generator metadata, options, and feature matrix](https://openapi-generator.tech/docs/generators/rust-axum/))

**Inference:** This is the only reviewed option that makes the OpenAPI description directly generate the server boundary, so it has the strongest construction-time claim to OpenAPI-first. Its Beta status and feature gaps make it unsuitable as an assumed V0 invariant without a representative prototype covering Yydra's actual errors, auth scheme, pagination, nullable values, unions, and generated trait ergonomics.

### 2.2 Canonical OpenAPI plus hand-written Axum adapter

**Verified facts:** Axum's official API supplies routing, extractors, responses, and Tower integration. `Json<T>` checks the JSON content type, syntax, buffering, and Serde deserialization into `T`; these checks do not themselves claim full validation against an external OpenAPI/JSON Schema document. ([Axum overview](https://docs.rs/axum/latest/axum/), [`Json<T>`](https://docs.rs/axum/latest/axum/struct.Json.html))

**Verified facts:** Schemathesis can generate requests from OpenAPI and test a running service for documented status codes, content types, required response headers, response-schema conformance, rejection of invalid input, and ignored authentication requirements. ([Schemathesis checks](https://schemathesis.readthedocs.io/en/stable/reference/checks/), [quick start](https://schemathesis.readthedocs.io/en/stable/quick-start/))

**Inference:** Hand-written Axum transport code introduces a second representation of routes and DTOs, but runtime contract testing can detect drift without putting a Beta server generator in the Framework's critical path. For V0 this is the lower-toolchain-risk OpenAPI-first route, provided the contract tests are mandatory rather than advisory.

### 2.3 Code-first alternative: Utoipa

**Verified facts:** Utoipa describes itself as code-first, compile-time OpenAPI generation for Rust and supports OpenAPI 3.1. `utoipa-axum` supplies an `OpenApiRouter`/routing layer that collects Axum handlers annotated with `#[utoipa::path]` while composing the router and OpenAPI document. ([Utoipa repository](https://github.com/juhaku/utoipa), [`utoipa-axum`](https://docs.rs/utoipa-axum/latest/utoipa_axum/))

**Material trade-off:** Utoipa removes the need for a separate hand-authored route inventory and fits Axum naturally. However, its examples still declare response metadata in `#[utoipa::path]` alongside the Rust handler and return type, so annotations and behavior can still disagree. The Rust code/annotations become the source; a hand-authored OpenAPI file is no longer the authority. This is credible when “compile-time, code-first Rust boundary” matters more than “review the public contract before server code.”

## 3. TypeScript client generation and runtime validation

### Verified facts

- React Native's official networking guide provides the Fetch API. Expo's current `expo/fetch` is described as WinterCG-compliant across web and mobile and is installed as global `fetch` on Android and iOS unless explicitly disabled. ([React Native networking](https://reactnative.dev/docs/network), [Expo Fetch](https://docs.expo.dev/versions/latest/sdk/expo/#expofetch-api))
- Orval generates TypeScript models and request functions from OpenAPI and supports a native Fetch client. ([Orval overview](https://orval.dev/docs/))
- Orval's Fetch client can accept an injected runtime `fetch` with `useRuntimeFetcher`, which lets the same generated boundary use the runtime's implementation. ([Orval `useRuntimeFetcher`](https://orval.dev/docs/reference/configuration/output/#useruntimefetcher))
- With Zod schema output and `override.fetch.runtimeValidation`, Orval validates eligible JSON responses using `Schema.parse()`. Its documentation also states that custom mutators can bypass the generated validation unless the schema is explicitly passed to the mutator. ([Orval runtime-validation matrix](https://orval.dev/docs/reference/configuration/output/#runtime-validation-support-matrix), [`includeZodSchemaInArguments`](https://orval.dev/docs/reference/configuration/output/#includezodschemainarguments))
- OpenAPI Generator's `typescript-fetch` is a stable Fetch client generator, but its published feature matrix records unsupported composition/union features. It has runtime-check options, but those do not remove its schema-feature limitations. ([`typescript-fetch` generator](https://openapi-generator.tech/docs/generators/typescript-fetch/))

### Inference for Yydra

Orval's plain Fetch client is the most directly evidenced V0 candidate for the selected Expo stack because runtime injection and response validation are documented together. Generate the transport SDK and schemas, not framework-specific UI cache policy, as the mandatory contract. React Query hooks and mocks can remain optional downstream outputs so that the public client boundary is usable from React Native, React Native Web, tests, and non-React code.

Compile-time TypeScript types are not runtime evidence. The V0 fixture should prove that a malformed backend success response fails at the client boundary on both web and at least one native runtime. Generator, Zod major version, and configuration must be pinned; Orval explicitly supports pinning the generated Zod major for deterministic output. ([Orval Zod version option](https://orval.dev/docs/reference/configuration/output/#version-1))

## 4. Errors: RFC 9457 Problem Details

### Verified facts

- RFC 9457 defines `application/problem+json` and obsoletes RFC 7807. Its standard members are `type`, `status`, `title`, `detail`, and `instance`, plus extension members. ([RFC 9457](https://www.rfc-editor.org/rfc/rfc9457.html))
- Consumers must treat the `type` URI as the primary machine identifier. `detail` is human-readable and consumers should not parse it for structured information; extensions should carry machine-readable fields. `status`, if present, is advisory and must agree with the actual HTTP response status generated at the origin. ([RFC 9457, Problem Details members](https://www.rfc-editor.org/rfc/rfc9457.html#name-members-of-a-problem-detail))
- New problem types must document a type URI, title, and HTTP status. The RFC warns against leaking stack dumps or sensitive implementation details. ([RFC 9457, defining problem types and security](https://www.rfc-editor.org/rfc/rfc9457.html#name-defining-new-problem-types))

### Inference for Yydra

Define one reusable base `Problem` schema and reusable `application/problem+json` responses in OpenAPI. Product Domain error cases that require client action should receive stable absolute `type` URIs and typed extension fields; generic HTTP failures can use `about:blank`. Generated clients should branch on `type`/status and typed extensions, never localized `title` or `detail` text.

The Axum rejection layer must translate extraction failures into this same public shape. Otherwise framework-generated parse errors and Product Domain errors would expose two incompatible contracts even if successful responses conform.

## 5. Authentication mechanism and authentication state

### Verified facts

- OpenAPI Security Scheme and Security Requirement objects describe mechanisms such as HTTP bearer, API keys, OAuth 2, and OpenID Connect, and declare which mechanisms an operation requires. An empty requirement can make anonymous access explicit. ([OpenAPI Security Scheme Object](https://spec.openapis.org/oas/v3.1.1.html#security-scheme-object), [Security Requirement Object](https://spec.openapis.org/oas/v3.1.1.html#security-requirement-object))
- HTTP 401 means the request lacks valid credentials and the server must include at least one `WWW-Authenticate` challenge. HTTP 403 means the server understood the request but refuses to fulfill it; repeating unchanged credentials should not be assumed to help. ([RFC 9110, 401 and 403](https://www.rfc-editor.org/rfc/rfc9110.html#name-401-unauthorized))
- For OAuth bearer tokens, RFC 6750 uses 401 plus `WWW-Authenticate: Bearer` for missing/invalid tokens and 403 for insufficient scope. ([RFC 6750, error responses](https://www.rfc-editor.org/rfc/rfc6750.html#section-3.1))
- React Native's current networking guide warns that cookie-based authentication is unstable and lists redirect/cookie limitations. ([React Native networking limitations](https://reactnative.dev/docs/network#known-issues-with-fetch-and-cookie-based-authentication))

### Inference for Yydra

OpenAPI can declare how credentials travel and which operations require them, but it does not define a frontend application state machine such as `unknown`, `anonymous`, `authenticated`, `refreshing`, or `expired`. If V0 exposes authentication state, it needs an explicit operation/response schema and transition/error semantics in addition to the Security Scheme.

Because full Identity is outside V0, the public-contract decision can remain narrow: standardize 401/403, `WWW-Authenticate`, and reusable Problem responses now; do not imply a complete identity/session capability. If cookie sessions are considered later, native runtime behavior must be prototype-tested rather than inferred from browser behavior.

## 6. Cursor pagination

### Verified facts

- OpenAPI can describe pagination parameters and response fields. Its Link Object can describe a design-time relationship to another operation, but it does not prescribe cursor semantics or require a runtime next link. ([OpenAPI Link Object](https://spec.openapis.org/oas/v3.1.1.html#link-object))
- Google's approved pagination guidance requires page tokens to be opaque and URL-safe, not user-parseable; a token must indicate continuation only and must not confer authorization. It also notes that adding pagination later is a breaking client change. ([Google AIP-158](https://google.aip.dev/158))

### Inference for Yydra

Treat cursor pagination as a Yydra wire convention expressed in OpenAPI, not as an OpenAPI feature. A minimal forward-only contract needs a bounded optional limit, an optional opaque cursor, an `items` array, and an optional/empty next cursor with one unambiguous end-of-list rule. The contract must state that filter/sort inputs remain consistent across requests, define invalid/expired cursor behavior as a Problem type, and perform authorization independently of cursor contents.

Cursor internals, SQLx keyset columns, signatures, and snapshot behavior are backend implementation details unless deliberately exposed. A stable total order and its tie-breaker belong in backend tests even if the cursor remains opaque.

## 7. Schema evolution and contract drift

### Verified facts

- OpenAPI's `deprecated` flag tells consumers to refrain from using an operation, but does not itself define removal timing. ([OpenAPI Operation Object](https://spec.openapis.org/oas/v3.1.1.html#operation-object))
- RFC 9745 standardizes the `Deprecation` response header and `deprecation` link relation. RFC 8594 defines `Sunset` for the time a resource is expected to become unavailable. ([RFC 9745](https://www.rfc-editor.org/rfc/rfc9745.html), [RFC 8594](https://www.rfc-editor.org/rfc/rfc8594.html))
- Redocly CLI supports OpenAPI 3.0, 3.1, and 3.2 and can lint a description against structural and configured API rules. ([Redocly CLI](https://github.com/Redocly/redocly-cli), [`lint`](https://redocly.com/docs/cli/commands/lint))
- `oasdiff` compares two OpenAPI descriptions and can fail on breaking changes. Its documented rule is consumer-oriented: a change is breaking when a consumer conforming to the old contract can stop working, even if the server happens to accept more than the contract says. ([oasdiff repository](https://github.com/oasdiff/oasdiff), [breaking-change model](https://github.com/oasdiff/oasdiff/blob/main/docs/BREAKING-CHANGES.md))
- Schemathesis validates the running service's status codes, content types, response headers, response bodies, input rejection, and authentication enforcement against the description. ([Schemathesis checks](https://schemathesis.readthedocs.io/en/stable/reference/checks/))

### Inference for Yydra

“No contract drift” needs independent gates because each catches a different failure:

1. **Description validity:** lint and bundle the canonical OpenAPI source.
2. **Generated artifact determinism:** run the pinned client generator, type-check the output, and fail if regeneration changes committed outputs unexpectedly.
3. **Evolution compatibility:** compare the base and proposed descriptions with `oasdiff`; require an explicit decision for allowed breaking changes.
4. **Implementation conformance:** boot the Axum service and run contract tests, including malformed inputs and deliberately malformed fixture responses.

Do not equate `info.version`, a URL prefix, or SemVer with an evolution policy. V0 must separately define which changes are compatible. Conservative defaults are additive optional fields, stable operation IDs, no silent type/narrowing changes, and explicit treatment of new response enum values because exhaustive generated clients can break even when the wire format remains a string.

## 8. Second credible alternative: TypeSpec schema-first

### Verified facts

TypeSpec is a language for defining API shapes as a single source of truth, with reusable patterns, a linter framework, and emitters. Its official OpenAPI emitter can produce OpenAPI 3.0.0, 3.1.0, or 3.2.0; the versioning library provides decorators such as `@added`, `@removed`, `@renamedFrom`, and type/optionality change markers. ([TypeSpec repository](https://github.com/microsoft/typespec), [OpenAPI emitter options](https://typespec.io/docs/emitters/openapi3/reference/emitter/), [versioning library](https://typespec.io/docs/libraries/versioning/reference/))

**Material trade-off:** TypeSpec provides a stronger reusable schema/design language and can emit OpenAPI as an intermediate artifact, but it introduces another Node-based DSL, compiler, and dependency set. It does not improve the maturity of Axum server generation: Yydra would still consume emitted OpenAPI through the same Beta `rust-axum` generator or enforce it against hand-written Axum code. For a one-stack V0 with no workspace-upgrade workflow, its versioning and multi-emitter power may exceed the immediate need.

## 9. Decision-ready V0 options

### Option A — OpenAPI-first, hand-written Axum boundary, generated validated Fetch client

- Authority: reviewed OpenAPI source.
- Server: normal Axum handlers/DTOs; Redocly/oasdiff/Schemathesis gates.
- Client: pinned Orval Fetch output with generated Zod schemas and runtime response validation; injectable fetch for Expo runtimes.
- Material cost: duplicate server representation and mandatory runtime contract tests.
- Evidence status: each component is documented as stable enough to prototype; no claim that the Axum implementation is generated from the contract.

### Option B — OpenAPI-first, generated `rust-axum` server boundary

- Authority: reviewed OpenAPI source; generated Rust trait/router and generated TypeScript client.
- Material benefit: the server's route/type boundary is constructed from the same contract.
- Material risk: `rust-axum` and OpenAPI Generator's 3.1 support are explicitly Beta with published feature gaps.
- Evidence status: credible only after a representative prototype passes compile, contract, ergonomics, and regeneration-diff gates.

### Alternative 1 — Utoipa code-first

- Authority: Rust types, handlers, and annotations; OpenAPI is generated.
- Material benefit: native Axum composition and compile-time schema generation.
- Material trade-off: the public description cannot be reviewed independently as the upstream authority, and annotations can still misdescribe runtime behavior.

### Alternative 2 — TypeSpec schema-first

- Authority: TypeSpec source; OpenAPI is generated and consumed downstream.
- Material benefit: reusable API design patterns and explicit version projections.
- Material trade-off: extra DSL/toolchain without removing the Axum generator/conformance problem.

**Research conclusion:** Option A has the fewest unverified dependencies for V0. Option B is the only reviewed construction path that makes spec-first reach directly into Axum routing, but its maturity claim must come from a Yydra prototype, not from the generator's current status. Utoipa and TypeSpec are credible alternatives only if Yydra intentionally changes what the authoritative source is.
