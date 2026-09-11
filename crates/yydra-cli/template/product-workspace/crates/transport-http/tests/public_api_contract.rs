// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use product_application::{
    ApplicationFuture, ChangeReadingEntryStateCommand, ChangeReadingEntryStateError,
    CreateReadingEntryCommand, CreateReadingEntryError, ListReadingEntriesError,
    ListReadingEntriesQuery, ReadingQueueApplication, ReadingQueueEntry, ReadingQueueEntryState,
    ReadingQueuePage,
};
use serde_json::Value;
use tower::ServiceExt;

const HTTP_METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];
const EXACT_DECIMAL_PATTERN: &str = r"^-?(0|[1-9][0-9]*)(\.[0-9]+)?$";

struct OperationFixture {
    operation_id: &'static str,
    method: &'static str,
    path: &'static str,
    body: &'static str,
    content_type: Option<&'static str>,
    authorization: Option<&'static str>,
}

const OPERATION_FIXTURES: &[OperationFixture] = &[
    OperationFixture {
        operation_id: "getProductSession",
        method: "get",
        path: "/api/v1/auth/session",
        body: "",
        content_type: None,
        authorization: None,
    },
    OperationFixture {
        operation_id: "logoutProductSession",
        method: "post",
        path: "/api/v1/auth/logout",
        body: "{}",
        content_type: Some("application/json"),
        authorization: None,
    },
    OperationFixture {
        operation_id: "exchangeNativeHandoff",
        method: "post",
        path: "/api/v1/auth/native/exchange",
        body: r#"{"handoff":"invalid","verifier":"invalid"}"#,
        content_type: Some("application/json"),
        authorization: None,
    },
    OperationFixture {
        operation_id: "createReadingQueueEntry",
        method: "post",
        path: "/api/v1/reading-queue/entries",
        body: r#"{"title":"Contract fixture","sourceUrl":"https://example.test/contract"}"#,
        content_type: Some("application/json"),
        authorization: None,
    },
    OperationFixture {
        operation_id: "changeReadingQueueEntryState",
        method: "patch",
        path: "/api/v1/reading-queue/entries/opaque-contract-entry",
        body: r#"{"state":"completed"}"#,
        content_type: Some("application/json"),
        authorization: None,
    },
    OperationFixture {
        operation_id: "getFrameworkContractProfile",
        method: "get",
        path: "/api/v1/framework-contract",
        body: "",
        content_type: None,
        authorization: None,
    },
    OperationFixture {
        operation_id: "getFrameworkProtectedContract",
        method: "get",
        path: "/api/v1/framework-auth-contract",
        body: "",
        content_type: None,
        authorization: Some("Bearer contract-test"),
    },
    OperationFixture {
        operation_id: "listReadingQueueEntries",
        method: "get",
        path: "/api/v1/reading-queue/entries",
        body: "",
        content_type: None,
        authorization: None,
    },
];

struct DocumentedOperation<'a> {
    method: &'a str,
    path: &'a str,
    contract: &'a Value,
}

#[tokio::test]
async fn public_router_executes_every_current_operation_against_its_contract() {
    let (router, collected) = product_transport_http::public_routes()
        .with_state(product_transport_http::ReadingQueueHttpState::new(
            FixtureReadingQueue,
        ))
        .split_for_parts();
    let router = fixture_auth(yydra_auth::AuthService::contract_fixture().layer(router));
    let collected: Value = serde_json::to_value(collected).expect("serialize collected OpenAPI");
    let current: Value = serde_json::from_str(
        &product_transport_http::normalized_openapi_json()
            .expect("derive current Public API Contract"),
    )
    .expect("parse current OpenAPI");
    assert_eq!(collected, current, "runtime route collection drifted");

    let operations = documented_operations(&current).expect("enumerate documented operations");
    let documented_ids = operations.keys().copied().collect::<BTreeSet<_>>();
    let fixture_ids = OPERATION_FIXTURES
        .iter()
        .map(|fixture| fixture.operation_id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        fixture_ids, documented_ids,
        "every Generated Client operation needs one live runtime fixture"
    );

    for fixture in OPERATION_FIXTURES {
        let operation = &operations[fixture.operation_id];
        assert_eq!(operation.method, fixture.method);
        assert_eq!(
            operation
                .path
                .replace("{entry_id}", "opaque-contract-entry"),
            fixture.path
        );
        let method = Method::from_bytes(fixture.method.to_ascii_uppercase().as_bytes())
            .expect("fixture HTTP method");
        let mut request = Request::builder().method(method).uri(fixture.path);
        if let Some(content_type) = fixture.content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        if let Some(authorization) = fixture.authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        let response = router
            .clone()
            .oneshot(
                request
                    .body(Body::from(fixture.body))
                    .expect("build request"),
            )
            .await
            .expect("call public router");
        assert_response_contract(&current, operation.contract, response)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", fixture.operation_id));
    }
}

#[derive(Clone, Copy)]
struct FixtureReadingQueue;

impl ReadingQueueApplication for FixtureReadingQueue {
    fn create<'a>(
        &'a self,
        command: CreateReadingEntryCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, CreateReadingEntryError>> {
        Box::pin(async move {
            Ok(ReadingQueueEntry {
                id: "opaque-contract-entry".to_owned(),
                title: command.title,
                source_url: command.source_url,
                state: ReadingQueueEntryState::Queued,
            })
        })
    }

    fn list<'a>(
        &'a self,
        query: ListReadingEntriesQuery,
    ) -> ApplicationFuture<'a, Result<ReadingQueuePage, ListReadingEntriesError>> {
        Box::pin(async move {
            if query.cursor.as_deref() == Some("invalid") {
                return Err(ListReadingEntriesError::InvalidCursor);
            }
            if query.status.as_deref() == Some("invented") {
                return Err(ListReadingEntriesError::InvalidInput {
                    field: "status",
                    message: "must be all, queued, or completed",
                });
            }
            Ok(ReadingQueuePage {
                entries: vec![ReadingQueueEntry {
                    id: "opaque-contract-entry".to_owned(),
                    title: "Contract fixture".to_owned(),
                    source_url: "https://example.test/contract".to_owned(),
                    state: ReadingQueueEntryState::Queued,
                }],
                next_cursor: (query.limit == Some(1)).then(|| "v1.opaque.signed".to_owned()),
            })
        })
    }

    fn change<'a>(
        &'a self,
        command: ChangeReadingEntryStateCommand,
    ) -> ApplicationFuture<'a, Result<ReadingQueueEntry, ChangeReadingEntryStateError>> {
        Box::pin(async move {
            match command.id.as_str() {
                "missing" => Err(ChangeReadingEntryStateError::NotFound { id: command.id }),
                "conflict" => Err(ChangeReadingEntryStateError::Conflict {
                    current: ReadingQueueEntryState::Completed,
                    requested: command.target,
                }),
                _ => Ok(ReadingQueueEntry {
                    id: command.id,
                    title: "Contract fixture".to_owned(),
                    source_url: "https://example.test/contract".to_owned(),
                    state: command.target,
                }),
            }
        })
    }
}

fn fixture_router() -> axum::Router {
    fixture_auth(
        product_transport_http::public_routes()
            .with_state(product_transport_http::ReadingQueueHttpState::new(
                FixtureReadingQueue,
            ))
            .into(),
    )
}

fn fixture_auth(router: axum::Router) -> axum::Router {
    router.layer(axum::middleware::from_fn(|mut request: axum::extract::Request, next: axum::middleware::Next| async move {
        let is_contract = request.uri().path() == "/api/v1/framework-auth-contract";
        let auth_header = request.headers_mut().remove(header::AUTHORIZATION);
        let auth = auth_header.as_ref().and_then(|v| v.to_str().ok());
        if is_contract && auth == Some("Bearer contract-forbidden") {
            return Response::builder().status(403).header(header::CONTENT_TYPE, "application/problem+json")
                .body(Body::from(r#"{"type":"https://yydra.dev/problems/access-forbidden","title":"Access forbidden","status":403}"#)).unwrap();
        }
        if !is_contract || auth == Some("Bearer contract-test") {
            request.extensions_mut().insert(yydra_auth::Principal { account_id: "contract-account".into() });
        }
        next.run(request).await
    }))
}

#[tokio::test]
async fn invalid_requests_transitions_and_auth_have_stable_problem_semantics() {
    let unauthorized = fixture_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/framework-auth-contract")
                .body(Body::empty())
                .expect("unauthorized request"),
        )
        .await
        .expect("call protected route");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        unauthorized.headers()[header::WWW_AUTHENTICATE],
        "Bearer realm=\"yydra-framework-contract\""
    );
    assert_problem_type(
        unauthorized,
        "https://yydra.dev/problems/authentication-required",
    )
    .await;

    let invalid_credential = fixture_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/framework-auth-contract")
                .header(header::AUTHORIZATION, "Bearer unknown")
                .body(Body::empty())
                .expect("invalid credential request"),
        )
        .await
        .expect("call protected route");
    assert_eq!(invalid_credential.status(), StatusCode::UNAUTHORIZED);
    assert!(
        invalid_credential
            .headers()
            .contains_key(header::WWW_AUTHENTICATE)
    );
    assert_problem_type(
        invalid_credential,
        "https://yydra.dev/problems/authentication-required",
    )
    .await;

    let forbidden = fixture_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/framework-auth-contract")
                .header(header::AUTHORIZATION, "Bearer contract-forbidden")
                .body(Body::empty())
                .expect("forbidden request"),
        )
        .await
        .expect("call protected route");
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert!(!forbidden.headers().contains_key(header::WWW_AUTHENTICATE));
    assert_problem_type(forbidden, "https://yydra.dev/problems/access-forbidden").await;

    for body in [
        "not-json",
        r#"{"title":"Example","sourceUrl":"https://example.test","unknown":true}"#,
    ] {
        let invalid = fixture_router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/reading-queue/entries")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("invalid JSON request"),
            )
            .await
            .expect("call create route");
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_problem_type(invalid, "https://yydra.dev/problems/invalid-request-body").await;
    }

    for (id, status, problem_type) in [
        (
            "missing",
            StatusCode::NOT_FOUND,
            "https://yydra.dev/problems/reading-entry-not-found",
        ),
        (
            "conflict",
            StatusCode::CONFLICT,
            "https://yydra.dev/problems/reading-entry-transition-conflict",
        ),
    ] {
        let response = fixture_router()
            .oneshot(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/api/v1/reading-queue/entries/{id}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"state":"completed"}"#))
                    .expect("transition request"),
            )
            .await
            .expect("call transition route");
        assert_eq!(response.status(), status);
        assert_problem_type(response, problem_type).await;
    }
}

#[tokio::test]
async fn pagination_query_and_cursor_failures_have_stable_public_semantics() {
    let page = fixture_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/reading-queue/entries?status=queued&sort=newest&limit=1")
                .body(Body::empty())
                .expect("page request"),
        )
        .await
        .expect("call paginated list route");
    assert_eq!(page.status(), StatusCode::OK);
    let page_body = to_bytes(page.into_body(), usize::MAX)
        .await
        .expect("read page body");
    let page_body: Value = serde_json::from_slice(&page_body).expect("parse page body");
    assert_eq!(page_body["nextCursor"], "v1.opaque.signed");

    for (query, problem_type) in [
        (
            "cursor=invalid",
            "https://yydra.dev/problems/invalid-reading-queue-cursor",
        ),
        (
            "status=invented",
            "https://yydra.dev/problems/invalid-reading-queue-query",
        ),
        (
            "unknown=true",
            "https://yydra.dev/problems/invalid-reading-queue-query",
        ),
    ] {
        let response = fixture_router()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/reading-queue/entries?{query}"))
                    .body(Body::empty())
                    .expect("invalid pagination request"),
            )
            .await
            .expect("call paginated list route");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_problem_type(response, problem_type).await;
    }
}

async fn assert_problem_type(response: Response<Body>, expected: &str) {
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read Problem body");
    let body: Value = serde_json::from_slice(&body).expect("parse Problem body");
    assert_eq!(body["type"], expected);
}

#[tokio::test]
async fn conformance_fixture_rejects_undocumented_status_content_type_and_body() {
    let document: Value = serde_json::from_str(
        &product_transport_http::normalized_openapi_json()
            .expect("derive current Public API Contract"),
    )
    .expect("parse OpenAPI");
    let operations = documented_operations(&document).expect("enumerate operations");
    let operation = operations["getFrameworkContractProfile"].contract;

    let declared_problem = Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(header::CONTENT_TYPE, "application/problem+json")
        .body(Body::from(
            r#"{"type":"https://yydra.dev/problems/example","title":"Example","status":500}"#,
        ))
        .expect("build declared Problem fixture");
    assert_response_contract(&document, operation, declared_problem)
        .await
        .expect("declared Problem response");

    let wrong_status = Response::builder()
        .status(StatusCode::CREATED)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .expect("build status fixture");
    assert_eq!(
        assert_response_contract(&document, operation, wrong_status).await,
        Err("undocumented status 201".to_owned())
    );

    let wrong_content_type = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/jsonp")
        .body(Body::from("ok"))
        .expect("build content-type fixture");
    assert_eq!(
        assert_response_contract(&document, operation, wrong_content_type).await,
        Err("undocumented content type application/jsonp".to_owned())
    );

    for (name, body) in [
        ("missing fields", r#"{"safeCount":1}"#),
        (
            "opaque identifier type",
            r#"{"opaqueId":7,"occurredAt":"2000-01-01T00:00:00Z","safeCount":1,"exactAmount":"0.01","items":[],"nullableNote":null}"#,
        ),
        (
            "UTC timestamp",
            r#"{"opaqueId":"id","occurredAt":"2000-01-01T01:00:00+01:00","safeCount":1,"exactAmount":"0.01","items":[],"nullableNote":null}"#,
        ),
        (
            "safe integer",
            r#"{"opaqueId":"id","occurredAt":"2000-01-01T00:00:00Z","safeCount":4294967296,"exactAmount":"0.01","items":[],"nullableNote":null}"#,
        ),
        (
            "exact decimal",
            r#"{"opaqueId":"id","occurredAt":"2000-01-01T00:00:00Z","safeCount":1,"exactAmount":"01.0","items":[],"nullableNote":null}"#,
        ),
        (
            "array collection",
            r#"{"opaqueId":"id","occurredAt":"2000-01-01T00:00:00Z","safeCount":1,"exactAmount":"0.01","items":null,"nullableNote":null}"#,
        ),
        (
            "nullable field",
            r#"{"opaqueId":"id","occurredAt":"2000-01-01T00:00:00Z","safeCount":1,"exactAmount":"0.01","items":[],"nullableNote":false}"#,
        ),
        (
            "optional non-null field",
            r#"{"opaqueId":"id","occurredAt":"2000-01-01T00:00:00Z","safeCount":1,"exactAmount":"0.01","items":[],"nullableNote":null,"optionalNote":null}"#,
        ),
    ] {
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .expect("build malformed wire fixture");
        assert!(
            assert_response_contract(&document, operation, response)
                .await
                .expect_err("malformed response must fail")
                .starts_with("response schema violation:"),
            "{name} fixture did not report a schema violation"
        );
    }

    let list_operation = operations["listReadingQueueEntries"].contract;
    let invalid_state = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"entries":[{"id":"opaque","title":"Example","sourceUrl":"https://example.test","state":"invented"}],"nextCursor":null}"#,
        ))
        .expect("build invalid reading-entry state fixture");
    assert!(
        assert_response_contract(&document, list_operation, invalid_state)
            .await
            .expect_err("an undocumented Product Domain state must fail")
            .contains("is outside its declared enum"),
    );
}

fn documented_operations<'a>(
    document: &'a Value,
) -> Result<BTreeMap<&'a str, DocumentedOperation<'a>>, String> {
    let paths = document["paths"]
        .as_object()
        .ok_or_else(|| "OpenAPI paths is not an object".to_owned())?;
    let mut operations = BTreeMap::new();
    for (path, item) in paths {
        let item = item
            .as_object()
            .ok_or_else(|| format!("path item {path} is not an object"))?;
        for (method, operation) in item {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let operation_id = operation["operationId"]
                .as_str()
                .ok_or_else(|| format!("{method} {path} has no operationId"))?;
            if operations
                .insert(
                    operation_id,
                    DocumentedOperation {
                        method,
                        path,
                        contract: operation,
                    },
                )
                .is_some()
            {
                return Err(format!("duplicate operationId {operation_id}"));
            }
        }
    }
    Ok(operations)
}

async fn assert_response_contract(
    document: &Value,
    operation: &Value,
    response: Response<Body>,
) -> Result<(), String> {
    let status = response.status().as_u16().to_string();
    let documented = operation["responses"]
        .get(&status)
        .ok_or_else(|| format!("undocumented status {status}"))?;
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or("missing");
    let media = documented["content"]
        .get(content_type)
        .ok_or_else(|| format!("undocumented content type {content_type}"))?;
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_slice(&body)
        .map_err(|error| format!("response schema violation: invalid JSON: {error}"))?;
    validate_schema(document, &media["schema"], &value, "response")
        .map_err(|error| format!("response schema violation: {error}"))
}

fn validate_schema(
    document: &Value,
    schema: &Value,
    value: &Value,
    location: &str,
) -> Result<(), String> {
    if let Some(reference) = schema["$ref"].as_str() {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("{location} uses a non-local schema reference"))?;
        let resolved = document
            .pointer(pointer)
            .ok_or_else(|| format!("{location} has an unresolved schema reference {reference}"))?;
        return validate_schema(document, resolved, value, location);
    }
    let types = match &schema["type"] {
        Value::String(value) => vec![value.as_str()],
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| format!("{location} has a malformed type union"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(format!("{location} has no supported schema type")),
    };
    if value.is_null() && types.contains(&"null") {
        return Ok(());
    }
    let non_null = types
        .iter()
        .copied()
        .filter(|kind| *kind != "null")
        .collect::<Vec<_>>();
    if non_null.len() != 1 {
        return Err(format!("{location} has an unsupported type union"));
    }
    match non_null[0] {
        "object" => validate_object(document, schema, value, location),
        "array" => {
            let items = value
                .as_array()
                .ok_or_else(|| format!("{location} is not an array"))?;
            for (index, item) in items.iter().enumerate() {
                validate_schema(
                    document,
                    &schema["items"],
                    item,
                    &format!("{location}[{index}]"),
                )?;
            }
            Ok(())
        }
        "boolean" => value
            .as_bool()
            .map(|_| ())
            .ok_or_else(|| format!("{location} is not a boolean")),
        "string" => validate_string(schema, value, location),
        "integer" => validate_integer(schema, value, location),
        other => Err(format!("{location} uses unsupported schema type {other}")),
    }
}

fn validate_object(
    document: &Value,
    schema: &Value,
    value: &Value,
    location: &str,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{location} is not an object"))?;
    let properties = schema["properties"]
        .as_object()
        .ok_or_else(|| format!("{location} object schema has no properties"))?;
    let required = schema["required"].as_array().map_or(&[][..], Vec::as_slice);
    for field in required {
        let field = field
            .as_str()
            .ok_or_else(|| format!("{location} has a malformed required field"))?;
        if !object.contains_key(field) {
            return Err(format!("{location} is missing {field}"));
        }
    }
    for (field, field_value) in object {
        if let Some(field_schema) = properties.get(field) {
            validate_schema(
                document,
                field_schema,
                field_value,
                &format!("{location}.{field}"),
            )?;
        } else if schema["additionalProperties"] != true {
            return Err(format!("{location} contains undocumented field {field}"));
        }
    }
    Ok(())
}

fn validate_string(schema: &Value, value: &Value, location: &str) -> Result<(), String> {
    let value = value
        .as_str()
        .ok_or_else(|| format!("{location} is not a string"))?;
    if schema["format"] == "date-time" && !is_utc_datetime(value) {
        return Err(format!("{location} is not an RFC 3339 UTC timestamp"));
    }
    if schema["enum"].as_array().is_some_and(|values| {
        !values
            .iter()
            .any(|candidate| candidate.as_str() == Some(value))
    }) {
        return Err(format!("{location} is outside its declared enum"));
    }
    match schema["pattern"].as_str() {
        None => Ok(()),
        Some("Z$") if value.ends_with('Z') => Ok(()),
        Some(EXACT_DECIMAL_PATTERN) if is_exact_decimal(value) => Ok(()),
        Some("Z$") => Err(format!("{location} does not end in Z")),
        Some(EXACT_DECIMAL_PATTERN) => Err(format!("{location} is not an exact decimal")),
        Some(pattern) => Err(format!("{location} uses unsupported pattern {pattern}")),
    }
}

fn validate_integer(schema: &Value, value: &Value, location: &str) -> Result<(), String> {
    let value = value
        .as_u64()
        .ok_or_else(|| format!("{location} is not a non-negative integer"))?;
    if schema["minimum"]
        .as_u64()
        .is_some_and(|minimum| value < minimum)
        || schema["maximum"]
            .as_u64()
            .is_some_and(|maximum| value > maximum)
    {
        return Err(format!("{location} is outside its declared range"));
    }
    Ok(())
}

fn is_utc_datetime(value: &str) -> bool {
    let Some(value) = value.strip_suffix('Z') else {
        return false;
    };
    let Some((date, time)) = value.split_once('T') else {
        return false;
    };
    let date = date.split('-').collect::<Vec<_>>();
    let time = time.split(':').collect::<Vec<_>>();
    if date.len() != 3 || time.len() != 3 {
        return false;
    }
    let valid_digits = |part: &str, width: usize| {
        part.len() == width && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    if !valid_digits(date[0], 4)
        || !valid_digits(date[1], 2)
        || !valid_digits(date[2], 2)
        || !valid_digits(time[0], 2)
        || !valid_digits(time[1], 2)
    {
        return false;
    }
    let second = time[2]
        .split_once('.')
        .map_or(time[2], |(whole, fraction)| {
            if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
                ""
            } else {
                whole
            }
        });
    valid_digits(second, 2)
        && date[1]
            .parse::<u8>()
            .is_ok_and(|value| (1..=12).contains(&value))
        && date[2]
            .parse::<u8>()
            .is_ok_and(|value| (1..=31).contains(&value))
        && time[0].parse::<u8>().is_ok_and(|value| value <= 23)
        && time[1].parse::<u8>().is_ok_and(|value| value <= 59)
        && second.parse::<u8>().is_ok_and(|value| value <= 60)
}

fn is_exact_decimal(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
        && fraction.is_none_or(|fraction| {
            !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
}
