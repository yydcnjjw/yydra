// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::{Builder, TempDir};

use crate::{Reporter, npm_program, verify_workspace};

const CONTRACT_PATH: &str = "contracts/openapi.json";
const GENERATED_PATH: &str = "frontend/src/generated/public-api";
const RECORD_PATH: &str = ".yydra/api-generation.json";
const HISTORY_PATH: &str = ".yydra/api-generation-history.json";
const LOCK_PATH: &str = ".yydra/api-generation.lock";
const ORVAL_VERSION: &str = "8.27.0";
const SOURCE_AUTHORITY: &str = "rust-utoipa-openapi-router";
const NORMALIZATION_SCHEMA_VERSION: u64 = 1;
const EXACT_DECIMAL_PATTERN: &str = r"^-?(0|[1-9][0-9]*)(\.[0-9]+)?$";
const HTTP_METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

pub(crate) struct ApiGenerationRequest<'a> {
    pub(crate) workspace: &'a Path,
    pub(crate) check: bool,
    pub(crate) acknowledgements: &'a [String],
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct CommittedApiGenerationRecord {
    schema_version: u64,
    source_authority: String,
    normalization_schema_version: u64,
    generator: String,
    openapi_sha256: String,
    generated_client_sha256: String,
    previous_openapi_sha256: Option<String>,
    previous_record_sha256: Option<String>,
    breaking_changes: Vec<String>,
    acknowledgements: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApiGenerationHistoryEntry {
    openapi: String,
    record: CommittedApiGenerationRecord,
}

struct CommittedCompatibilityState {
    history: Vec<ApiGenerationHistoryEntry>,
    record: CommittedApiGenerationRecord,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApiGenerationTransactionManifest {
    schema_version: u64,
    outputs: Vec<ApiGenerationTransactionOutput>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApiGenerationTransactionOutput {
    path: String,
    had_original: bool,
}

pub(crate) fn generate_api(request: ApiGenerationRequest<'_>, reporter: &Reporter) -> Result<()> {
    if request.check && !request.acknowledgements.is_empty() {
        bail!("API_ACKNOWLEDGEMENT_INVALID: --check cannot acknowledge a breaking change");
    }
    validate_acknowledgements(request.acknowledgements)?;
    let (root, _) = verify_workspace(request.workspace)
        .context("API_WORKSPACE_INVALID: verify exact Product Workspace")?;
    let _generation_lock = acquire_generation_lock(&root, request.check)?;
    recover_incomplete_transactions(&root, request.check)?;
    let stage = Builder::new()
        .prefix("yydra-api-generation-")
        .tempdir()
        .context("API_STAGE_CREATE_FAILED: create isolated API generation root")?;
    let openapi = stage.path().join("openapi/openapi.json");
    let generated = stage.path().join("generated-client");

    reporter.phase(
        "generate.api.openapi",
        "API_OPENAPI_EXPORT",
        Some(&root.join(CONTRACT_PATH)),
        Some("fix the authoritative Rust route declarations and rerun `yydra generate api`"),
        || export_openapi(&root, &openapi),
    )?;
    let proposed_openapi = fs::read(&openapi)
        .with_context(|| format!("API_OPENAPI_EXPORT_FAILED: read '{}'", openapi.display()))?;
    validate_openapi_profile(&proposed_openapi)?;

    reporter.phase(
        "generate.api.client",
        "API_CLIENT_GENERATE",
        Some(&root.join(GENERATED_PATH)),
        Some("run `yydra setup`, then fix the Public API or pinned Orval configuration"),
        || generate_client(&root, &openapi, &generated),
    )?;
    validate_generated_client(&generated)?;

    let committed_openapi = read_optional_file(&root.join(CONTRACT_PATH))?;
    if committed_openapi.is_none()
        && (root.join(GENERATED_PATH).exists()
            || root.join(RECORD_PATH).exists()
            || root.join(HISTORY_PATH).exists())
    {
        bail!(
            "API_GENERATION_BASELINE_INVALID: committed API outputs are incomplete; restore one complete reviewed output set"
        );
    }
    let breaking_changes = committed_openapi
        .as_deref()
        .map(|old| breaking_changes(old, &proposed_openapi))
        .transpose()?
        .unwrap_or_default();
    let openapi_sha256 = sha256(&proposed_openapi);
    let generated_client_sha256 = directory_digest(&generated)?;

    if request.check {
        check_committed_outputs(
            &root,
            &proposed_openapi,
            &generated,
            &openapi_sha256,
            &generated_client_sha256,
            &breaking_changes,
        )?;
        return Ok(());
    }

    // A write invocation with already-current committed outputs is a true no-op.
    // In particular, keep the original compatibility baseline instead of
    // rewriting `previousOpenapiSha256` to the current contract on every run.
    if check_committed_outputs(
        &root,
        &proposed_openapi,
        &generated,
        &openapi_sha256,
        &generated_client_sha256,
        &breaking_changes,
    )
    .is_ok()
    {
        return Ok(());
    }

    if !breaking_changes.is_empty() && request.acknowledgements.is_empty() {
        bail!(
            "API_BREAKING_CHANGE_UNACKNOWLEDGED: {}; rerun with a reviewed --acknowledge-breaking-change <reference>",
            breaking_changes.join("; ")
        );
    }
    if breaking_changes.is_empty() && !request.acknowledgements.is_empty() {
        bail!("API_ACKNOWLEDGEMENT_INVALID: there is no breaking change to acknowledge");
    }

    let committed_state = committed_openapi
        .as_deref()
        .map(|openapi| load_committed_compatibility_state(&root, openapi))
        .transpose()?;
    let (
        history,
        previous_openapi_sha256,
        previous_record_sha256,
        recorded_breaking,
        recorded_acknowledgements,
    ) = match (committed_openapi.as_deref(), committed_state) {
        (Some(committed), Some(state)) if committed == proposed_openapi => (
            state.history,
            state.record.previous_openapi_sha256,
            state.record.previous_record_sha256,
            state.record.breaking_changes,
            state.record.acknowledgements,
        ),
        (Some(committed), Some(mut state)) => {
            let previous_openapi_sha256 = sha256(committed);
            let previous_record_sha256 = sha256(&generation_record_bytes(&state.record)?);
            state.history.push(ApiGenerationHistoryEntry {
                openapi: String::from_utf8(committed.to_vec())
                    .context("API_GENERATION_BASELINE_INVALID: committed OpenAPI is not UTF-8")?,
                record: state.record,
            });
            (
                state.history,
                Some(previous_openapi_sha256),
                Some(previous_record_sha256),
                breaking_changes.clone(),
                request.acknowledgements.to_vec(),
            )
        }
        (None, None) => (Vec::new(), None, None, Vec::new(), Vec::new()),
        _ => unreachable!("committed OpenAPI and compatibility state must agree"),
    };
    let record = CommittedApiGenerationRecord {
        schema_version: 1,
        source_authority: SOURCE_AUTHORITY.to_owned(),
        normalization_schema_version: NORMALIZATION_SCHEMA_VERSION,
        generator: format!("orval@{ORVAL_VERSION}"),
        openapi_sha256,
        generated_client_sha256,
        previous_openapi_sha256,
        previous_record_sha256,
        breaking_changes: recorded_breaking,
        acknowledgements: recorded_acknowledgements,
    };
    let record_path = stage.path().join("api-generation.json");
    fs::write(&record_path, generation_record_bytes(&record)?)
        .context("API_STAGE_WRITE_FAILED: write staged API generation record")?;
    let history_path = stage.path().join("api-generation-history.json");
    fs::write(&history_path, generation_history_bytes(&history)?)
        .context("API_STAGE_WRITE_FAILED: write staged API generation history")?;

    if std::env::var_os("YYDRA_TEST_API_GENERATION_FAIL_AFTER_STAGE").is_some() {
        bail!("API_STAGED_FAILURE_INJECTED: staged outputs were intentionally rejected");
    }

    reporter.phase(
        "generate.api.commit",
        "API_OUTPUT_ATOMIC_REPLACE",
        Some(&root),
        Some("inspect filesystem permissions; the previous complete outputs were restored"),
        || {
            replace_outputs(
                &root,
                [
                    (openapi.as_path(), Path::new(CONTRACT_PATH)),
                    (generated.as_path(), Path::new(GENERATED_PATH)),
                    (history_path.as_path(), Path::new(HISTORY_PATH)),
                    (record_path.as_path(), Path::new(RECORD_PATH)),
                ],
            )
        },
    )
}

fn export_openapi(root: &Path, output: &Path) -> Result<()> {
    let parent = output
        .parent()
        .context("API_STAGE_CREATE_FAILED: output has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("API_STAGE_CREATE_FAILED: create '{}'", parent.display()))?;
    run_stage_command(
        root,
        "cargo",
        &[
            OsStr::new("run"),
            OsStr::new("--locked"),
            OsStr::new("--quiet"),
            OsStr::new("--bin"),
            OsStr::new("export-openapi"),
            OsStr::new("--"),
            output.as_os_str(),
        ],
        &[],
        "API_OPENAPI_EXPORT_FAILED",
    )
}

fn generate_client(root: &Path, openapi: &Path, output: &Path) -> Result<()> {
    validate_local_generator_authority(root)?;
    fs::create_dir_all(output)
        .with_context(|| format!("API_STAGE_CREATE_FAILED: create '{}'", output.display()))?;
    let config = root.join("frontend/orval.config.mjs");
    run_stage_command(
        &root.join("frontend"),
        npm_program(),
        &[
            OsStr::new("exec"),
            OsStr::new("--offline"),
            OsStr::new("--"),
            OsStr::new("orval"),
            OsStr::new("--config"),
            config.as_os_str(),
            OsStr::new("--clean"),
            OsStr::new("--fail-on-warnings"),
        ],
        &[
            ("YYDRA_OPENAPI_INPUT", openapi.as_os_str()),
            ("YYDRA_GENERATED_API_OUTPUT", output.as_os_str()),
        ],
        "API_CLIENT_GENERATION_FAILED",
    )?;
    let tsconfig = output
        .parent()
        .context("API_CLIENT_TYPECHECK_FAILED: generated output has no parent")?
        .join("generated-client-tsconfig.json");
    let frontend = root.join("frontend");
    let include = format!("{}/**/*.ts", output.display()).replace('\\', "/");
    let config = serde_json::json!({
        "compilerOptions": {
            "strict": true,
            "noEmit": true,
            "target": "ES2022",
            "module": "ESNext",
            "moduleResolution": "Bundler",
            "lib": ["ES2022", "DOM"],
            "paths": {
                "zod": [frontend.join("node_modules/zod/index.d.cts")]
            },
            "skipLibCheck": false
        },
        "include": [include]
    });
    let mut config = serde_json::to_vec_pretty(&config)?;
    config.push(b'\n');
    fs::write(&tsconfig, config)
        .context("API_CLIENT_TYPECHECK_FAILED: write isolated generated-client tsconfig")?;
    run_stage_command(
        &root.join("frontend"),
        npm_program(),
        &[
            OsStr::new("exec"),
            OsStr::new("--offline"),
            OsStr::new("--"),
            OsStr::new("tsc"),
            OsStr::new("--project"),
            tsconfig.as_os_str(),
        ],
        &[],
        "API_CLIENT_TYPECHECK_FAILED",
    )
}

fn validate_local_generator_authority(root: &Path) -> Result<()> {
    let package = root.join("frontend/node_modules/orval/package.json");
    let package: Value = serde_json::from_slice(&fs::read(&package).with_context(
        || "API_CLIENT_TOOL_VERSION_INVALID: project-local Orval is missing; run `yydra setup`",
    )?)
    .context(
        "API_CLIENT_TOOL_VERSION_INVALID: project-local Orval package metadata is malformed",
    )?;
    if package["version"] != ORVAL_VERSION {
        bail!(
            "API_CLIENT_TOOL_VERSION_INVALID: expected project-local orval@{ORVAL_VERSION}, found {}",
            package["version"]
        );
    }
    Ok(())
}

fn run_stage_command(
    directory: &Path,
    program: &str,
    arguments: &[&OsStr],
    environment: &[(&str, &OsStr)],
    code: &str,
) -> Result<()> {
    let output = Command::new(program)
        .args(arguments)
        .envs(environment.iter().copied())
        .current_dir(directory)
        .output()
        .with_context(|| format!("{code}: start {program}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stdout
        .lines()
        .chain(stderr.lines())
        .take(20)
        .collect::<Vec<_>>()
        .join(" | ");
    bail!(
        "{code}: {program} exited with {}; {detail}",
        output
            .status
            .code()
            .map_or_else(|| "signal".to_owned(), |value| value.to_string())
    )
}

fn validate_openapi_profile(bytes: &[u8]) -> Result<()> {
    let document: Value = serde_json::from_slice(bytes)
        .context("API_OPENAPI_PROFILE_INVALID: generated document is not JSON")?;
    if document["openapi"] != "3.1.0" {
        bail!("API_OPENAPI_PROFILE_INVALID: expected OpenAPI 3.1.0");
    }
    if document.get("security").is_some() {
        bail!(
            "API_OPENAPI_PROFILE_INVALID: global security is outside the supported Public API profile"
        );
    }
    let components = object_at(&document, "/components")?;
    if components.keys().any(|key| key != "schemas") {
        bail!(
            "API_OPENAPI_PROFILE_INVALID: reusable parameters, request bodies, responses, headers, examples, links, callbacks, and security schemes are outside the supported Public API profile"
        );
    }
    let paths = object_at(&document, "/paths")?;
    let mut operation_ids = BTreeSet::new();
    for (path, path_item) in paths {
        let operations = path_item.as_object().ok_or_else(|| {
            anyhow::anyhow!("API_OPENAPI_PROFILE_INVALID: path {path} is not an object")
        })?;
        if operations
            .keys()
            .any(|method| !HTTP_METHODS.contains(&method.as_str()))
        {
            bail!(
                "API_OPENAPI_PROFILE_INVALID: path-item declarations outside HTTP operations are not supported at {path}"
            );
        }
        for (method, operation) in operations {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let id = operation["operationId"].as_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "API_OPENAPI_OPERATION_ID_INVALID: {method} {path} has no operationId"
                )
            })?;
            if !is_lower_camel_identifier(id) || !operation_ids.insert(id.to_owned()) {
                bail!(
                    "API_OPENAPI_OPERATION_ID_INVALID: operationId '{id}' must be globally unique lowerCamelCase"
                );
            }
            let responses = operation["responses"].as_object().ok_or_else(|| {
                anyhow::anyhow!("API_OPENAPI_PROFILE_INVALID: {id} has no responses")
            })?;
            for (status, response) in responses {
                let status_number = status.parse::<u16>().with_context(|| {
                    format!("API_OPENAPI_PROFILE_INVALID: {id} uses non-numeric status {status}")
                })?;
                let content = response["content"].as_object().ok_or_else(|| {
                    anyhow::anyhow!(
                        "API_OPENAPI_PROFILE_INVALID: {id} status {status} has no declared content"
                    )
                })?;
                let required = if status_number >= 400 {
                    "application/problem+json"
                } else {
                    "application/json"
                };
                if content.len() != 1 || !content.contains_key(required) {
                    bail!(
                        "API_OPENAPI_CONTENT_TYPE_INVALID: {id} status {status} must declare only {required}"
                    );
                }
            }
        }
    }
    if operation_ids.is_empty() {
        bail!("API_OPENAPI_PROFILE_INVALID: Public API has zero operations");
    }

    let schemas = object_at(&document, "/components/schemas")?;
    for (name, schema) in schemas {
        if schema["type"] == "object" {
            let properties = schema["properties"].as_object().ok_or_else(|| {
                anyhow::anyhow!("API_OPENAPI_PROFILE_INVALID: schema {name} has no properties")
            })?;
            for field in properties.keys() {
                if !is_lower_camel_identifier(field) {
                    bail!(
                        "API_OPENAPI_FIELD_NAME_INVALID: schema {name} field '{field}' is not lowerCamelCase"
                    );
                }
            }
        }
    }
    require_additional_properties(schemas, "FrameworkContractCreate", false)?;
    require_additional_properties(schemas, "FrameworkContractPatch", false)?;
    require_additional_properties(schemas, "FrameworkContractProfile", true)?;
    require_additional_properties(schemas, "ProblemDetails", true)?;

    let profile = &schemas["FrameworkContractProfile"];
    let required = string_set(&profile["required"])?;
    for field in [
        "opaqueId",
        "occurredAt",
        "safeCount",
        "exactAmount",
        "items",
        "nullableNote",
    ] {
        if !required.contains(field) {
            bail!(
                "API_OPENAPI_REQUIREDNESS_INVALID: FrameworkContractProfile.{field} must be required"
            );
        }
    }
    if required.contains("optionalNote") {
        bail!("API_OPENAPI_REQUIREDNESS_INVALID: optionalNote must remain optional");
    }
    let properties = object_at(profile, "/properties")?;
    require_schema_type(&properties["opaqueId"], "string", "opaqueId")?;
    require_schema_type(&properties["exactAmount"], "string", "exactAmount")?;
    if properties["exactAmount"]["pattern"] != EXACT_DECIMAL_PATTERN {
        bail!("API_OPENAPI_DECIMAL_INVALID: exactAmount must carry the canonical decimal pattern");
    }
    if schemas["FrameworkContractCreate"]["properties"]["exactAmount"]["pattern"]
        != properties["exactAmount"]["pattern"]
    {
        bail!(
            "API_OPENAPI_DECIMAL_INVALID: create and response exactAmount fields must share the exact decimal rule"
        );
    }
    if properties["occurredAt"]["format"] != "date-time" {
        bail!("API_OPENAPI_TIMESTAMP_INVALID: occurredAt must use date-time");
    }
    if properties["occurredAt"]["pattern"] != "Z$" {
        bail!("API_OPENAPI_TIMESTAMP_INVALID: occurredAt must be UTC with a Z suffix");
    }
    require_schema_type(&properties["safeCount"], "integer", "safeCount")?;
    let safe_minimum = properties["safeCount"]["minimum"].as_i64();
    let safe_maximum = properties["safeCount"]["maximum"].as_i64();
    if safe_minimum.is_none_or(|value| value < -9_007_199_254_740_991)
        || safe_maximum.is_none_or(|value| value > 9_007_199_254_740_991)
    {
        bail!(
            "API_OPENAPI_SAFE_INTEGER_INVALID: safeCount must be bounded to the JavaScript safe range"
        );
    }
    require_schema_type(&properties["items"], "array", "items")?;
    require_type_union(
        &properties["nullableNote"],
        &["null", "string"],
        "nullableNote",
    )?;
    require_schema_type(&properties["optionalNote"], "string", "optionalNote")?;
    if schemas["FrameworkContractCreate"] == schemas["FrameworkContractProfile"]
        || schemas["FrameworkContractPatch"] == schemas["FrameworkContractProfile"]
        || schemas["FrameworkContractCreate"] == schemas["FrameworkContractPatch"]
    {
        bail!(
            "API_OPENAPI_SHAPE_REUSE_INVALID: create, response, and patch schemas must remain separate"
        );
    }

    Ok(())
}

fn validate_generated_client(root: &Path) -> Result<()> {
    let inventory = directory_inventory(root)?;
    let client = inventory
        .get(Path::new("fetch/client.ts"))
        .ok_or_else(|| anyhow::anyhow!("API_CLIENT_STAGE_INVALID: Orval did not emit client.ts"))?;
    let client = String::from_utf8_lossy(client);
    for marker in [
        "SPDX-License-Identifier: MIT OR Apache-2.0",
        "Do not edit manually.",
        "getFrameworkContractProfile",
        "fetchFn",
        "FrameworkContractProfile.parse",
    ] {
        if !client.contains(marker) {
            bail!("API_CLIENT_STAGE_INVALID: client.ts is missing {marker:?}");
        }
    }
    for expected in [
        "fetch/schemas/frameworkContractProfile.zod.ts",
        "fetch/schemas/problemDetails.zod.ts",
        "request/schemas/frameworkContractCreate.zod.ts",
        "request/schemas/frameworkContractPatch.zod.ts",
    ] {
        if !inventory.contains_key(Path::new(expected)) {
            bail!("API_CLIENT_STAGE_INVALID: Orval did not emit {expected}");
        }
    }
    for (response, bytes) in inventory.iter().filter(|(path, _)| {
        path.starts_with("fetch/schemas") && path.extension().is_some_and(|value| value == "ts")
    }) {
        let source = String::from_utf8_lossy(bytes);
        if source.contains("strictObject") || source.contains(".strict()") {
            bail!(
                "API_CLIENT_STAGE_INVALID: Fetch schema {} must tolerate additive unknown fields",
                response.display()
            );
        }
    }
    for (request, bytes) in inventory.iter().filter(|(path, _)| {
        path.starts_with("request/schemas") && path.extension().is_some_and(|value| value == "ts")
    }) {
        let source = String::from_utf8_lossy(bytes);
        if source.contains("zod.object(") && !source.contains(".strict()") {
            bail!(
                "API_CLIENT_STAGE_INVALID: request schema {} must reject unknown fields",
                request.display()
            );
        }
    }
    Ok(())
}

fn check_committed_outputs(
    root: &Path,
    proposed_openapi: &[u8],
    proposed_generated: &Path,
    openapi_sha256: &str,
    generated_client_sha256: &str,
    breaking: &[String],
) -> Result<()> {
    let committed_openapi = fs::read(root.join(CONTRACT_PATH))
        .context("API_GENERATED_DRIFT: committed contracts/openapi.json is missing")?;
    if committed_openapi != proposed_openapi {
        if !breaking.is_empty() {
            bail!(
                "API_BREAKING_CHANGE_UNACKNOWLEDGED: {}",
                breaking.join("; ")
            );
        }
        bail!(
            "API_GENERATED_DRIFT: Rust-authored OpenAPI differs from contracts/openapi.json; run `yydra generate api`"
        );
    }
    if directory_inventory(&root.join(GENERATED_PATH))? != directory_inventory(proposed_generated)?
    {
        bail!("API_CLIENT_DRIFT: committed Orval/Zod outputs differ; run `yydra generate api`");
    }
    let state = load_committed_compatibility_state(root, &committed_openapi)
        .context("API_GENERATION_RECORD_DRIFT: compatibility history is invalid")?;
    if state.record.openapi_sha256 != openapi_sha256
        || state.record.generated_client_sha256 != generated_client_sha256
    {
        bail!("API_GENERATION_RECORD_DRIFT: generation record output digests differ");
    }
    Ok(())
}

fn load_committed_compatibility_state(
    root: &Path,
    committed_openapi: &[u8],
) -> Result<CommittedCompatibilityState> {
    let record_bytes = fs::read(root.join(RECORD_PATH))
        .context("API_GENERATION_BASELINE_INVALID: generation record is missing")?;
    let record: CommittedApiGenerationRecord = serde_json::from_slice(&record_bytes)
        .context("API_GENERATION_BASELINE_INVALID: generation record is malformed")?;
    if generation_record_bytes(&record)? != record_bytes {
        bail!("API_GENERATION_BASELINE_INVALID: generation record is not canonical JSON");
    }
    let history_bytes = fs::read(root.join(HISTORY_PATH))
        .context("API_GENERATION_BASELINE_INVALID: generation history is missing")?;
    let history: Vec<ApiGenerationHistoryEntry> = serde_json::from_slice(&history_bytes)
        .context("API_GENERATION_BASELINE_INVALID: generation history is malformed")?;
    if generation_history_bytes(&history)? != history_bytes {
        bail!("API_GENERATION_BASELINE_INVALID: generation history is not canonical JSON");
    }
    validate_compatibility_chain(&history, committed_openapi, &record)?;
    Ok(CommittedCompatibilityState { history, record })
}

fn validate_compatibility_chain(
    history: &[ApiGenerationHistoryEntry],
    current_openapi: &[u8],
    current_record: &CommittedApiGenerationRecord,
) -> Result<()> {
    let mut previous: Option<(&[u8], &CommittedApiGenerationRecord)> = None;
    for entry in history {
        let openapi = entry.openapi.as_bytes();
        validate_openapi_profile(openapi).context(
            "API_GENERATION_BASELINE_INVALID: archived OpenAPI violates the supported profile",
        )?;
        validate_generation_record(openapi, &entry.record, previous)?;
        previous = Some((openapi, &entry.record));
    }
    validate_openapi_profile(current_openapi).context(
        "API_GENERATION_BASELINE_INVALID: committed OpenAPI violates the supported profile",
    )?;
    validate_generation_record(current_openapi, current_record, previous)
}

fn validate_generation_record(
    openapi: &[u8],
    record: &CommittedApiGenerationRecord,
    previous: Option<(&[u8], &CommittedApiGenerationRecord)>,
) -> Result<()> {
    if record.schema_version != 1
        || record.source_authority != SOURCE_AUTHORITY
        || record.normalization_schema_version != NORMALIZATION_SCHEMA_VERSION
        || record.generator != format!("orval@{ORVAL_VERSION}")
        || record.openapi_sha256 != sha256(openapi)
        || !is_sha256(&record.generated_client_sha256)
    {
        bail!("API_GENERATION_BASELINE_INVALID: record authority or digests differ");
    }
    if record
        .breaking_changes
        .iter()
        .any(|change| change.is_empty() || change.bytes().any(|byte| byte.is_ascii_control()))
        || !record
            .breaking_changes
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || validate_acknowledgements(&record.acknowledgements).is_err()
    {
        bail!("API_GENERATION_BASELINE_INVALID: compatibility decision is malformed");
    }
    match previous {
        None => {
            if record.previous_openapi_sha256.is_some()
                || record.previous_record_sha256.is_some()
                || !record.breaking_changes.is_empty()
                || !record.acknowledgements.is_empty()
            {
                bail!(
                    "API_GENERATION_BASELINE_INVALID: first generation has an impossible predecessor or decision"
                );
            }
        }
        Some((previous_openapi, previous_record)) => {
            let expected_previous_openapi = sha256(previous_openapi);
            let expected_previous_record = sha256(&generation_record_bytes(previous_record)?);
            if record.previous_openapi_sha256.as_deref() != Some(expected_previous_openapi.as_str())
                || record.previous_record_sha256.as_deref()
                    != Some(expected_previous_record.as_str())
            {
                bail!("API_GENERATION_BASELINE_INVALID: compatibility chain digest differs");
            }
            let expected_breaking = breaking_changes(previous_openapi, openapi)?;
            if record.breaking_changes != expected_breaking
                || (record.breaking_changes.is_empty() && !record.acknowledgements.is_empty())
                || (!record.breaking_changes.is_empty() && record.acknowledgements.is_empty())
            {
                bail!(
                    "API_GENERATION_BASELINE_INVALID: recorded breaking decision cannot be reproduced"
                );
            }
        }
    }
    Ok(())
}

fn generation_record_bytes(record: &CommittedApiGenerationRecord) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(record)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn generation_history_bytes(history: &[ApiGenerationHistoryEntry]) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(history)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn breaking_changes(old: &[u8], new: &[u8]) -> Result<Vec<String>> {
    let old: Value = serde_json::from_slice(old)
        .context("API_COMPATIBILITY_BASE_INVALID: committed OpenAPI is malformed")?;
    let new: Value = serde_json::from_slice(new)
        .context("API_COMPATIBILITY_PROPOSED_INVALID: proposed OpenAPI is malformed")?;
    let mut changes = Vec::new();
    let old_paths = object_at(&old, "/paths")?;
    let new_paths = object_at(&new, "/paths")?;
    for (path, old_item) in old_paths {
        let Some(new_item) = new_paths.get(path) else {
            changes.push(format!("removed path {path}"));
            continue;
        };
        for method in HTTP_METHODS {
            let Some(old_operation) = old_item.get(method) else {
                continue;
            };
            let Some(new_operation) = new_item.get(method) else {
                changes.push(format!("removed operation {method} {path}"));
                continue;
            };
            if old_operation["operationId"] != new_operation["operationId"] {
                changes.push(format!("changed operationId for {method} {path}"));
            }
            compare_operation_parameters(
                old_operation,
                new_operation,
                &format!("{method} {path}"),
                &mut changes,
            )?;
            compare_request_body(
                old_operation,
                new_operation,
                &format!("{method} {path}"),
                &mut changes,
            );
            if contract_shape(&old_operation["security"])
                != contract_shape(&new_operation["security"])
            {
                changes.push(format!("changed security requirements for {method} {path}"));
            }
            if contract_shape_without(
                old_operation,
                &[
                    "operationId",
                    "responses",
                    "parameters",
                    "requestBody",
                    "security",
                    "description",
                    "summary",
                    "tags",
                ],
            ) != contract_shape_without(
                new_operation,
                &[
                    "operationId",
                    "responses",
                    "parameters",
                    "requestBody",
                    "security",
                    "description",
                    "summary",
                    "tags",
                ],
            ) {
                changes.push(format!("changed operation contract for {method} {path}"));
            }
            if let Some(old_responses) = old_operation["responses"].as_object() {
                let new_responses = new_operation["responses"].as_object();
                for (status, old_response) in old_responses {
                    let Some(new_response) = new_responses.and_then(|value| value.get(status))
                    else {
                        changes.push(format!("removed response {status} from {method} {path}"));
                        continue;
                    };
                    if contract_shape_without(old_response, &["description", "content"])
                        != contract_shape_without(new_response, &["description", "content"])
                    {
                        changes.push(format!(
                            "changed response metadata for {method} {path} {status}"
                        ));
                    }
                    if let Some(old_content) = old_response["content"].as_object() {
                        for content_type in old_content.keys() {
                            let Some(new_media) = new_response["content"].get(content_type) else {
                                changes.push(format!(
                                    "removed content type {content_type} from {method} {path} {status}"
                                ));
                                continue;
                            };
                            compare_schema_contract(
                                &old_content[content_type]["schema"],
                                &new_media["schema"],
                                &format!(
                                    "response schema for {method} {path} {status} {content_type}"
                                ),
                                &mut changes,
                            )?;
                        }
                    }
                }
            }
        }
    }
    let old_schemas = object_at(&old, "/components/schemas")?;
    let new_schemas = object_at(&new, "/components/schemas")?;
    for (name, old_schema) in old_schemas {
        let Some(new_schema) = new_schemas.get(name) else {
            changes.push(format!("removed schema {name}"));
            continue;
        };
        compare_schema_contract(
            old_schema,
            new_schema,
            &format!("schema {name}"),
            &mut changes,
        )?;
    }
    changes.sort();
    changes.dedup();
    Ok(changes)
}

fn compare_operation_parameters(
    old_operation: &Value,
    new_operation: &Value,
    operation: &str,
    changes: &mut Vec<String>,
) -> Result<()> {
    let old = parameter_map(&old_operation["parameters"])?;
    let new = parameter_map(&new_operation["parameters"])?;
    for (key, old_parameter) in &old {
        let Some(new_parameter) = new.get(key) else {
            changes.push(format!("removed parameter {key} from {operation}"));
            continue;
        };
        if contract_shape(old_parameter) != contract_shape(new_parameter) {
            changes.push(format!("changed parameter {key} on {operation}"));
        }
    }
    for (key, new_parameter) in new {
        if !old.contains_key(&key)
            && (new_parameter["required"] == true || new_parameter.get("$ref").is_some())
        {
            changes.push(format!("added required parameter {key} to {operation}"));
        }
    }
    Ok(())
}

fn parameter_map(value: &Value) -> Result<BTreeMap<String, &Value>> {
    let mut parameters = BTreeMap::new();
    let Some(values) = value.as_array() else {
        if value.is_null() {
            return Ok(parameters);
        }
        bail!("API_COMPATIBILITY_BASE_INVALID: parameters must be an array");
    };
    for parameter in values {
        let key = if let Some(reference) = parameter["$ref"].as_str() {
            format!("ref:{reference}")
        } else {
            let location = parameter["in"].as_str().ok_or_else(|| {
                anyhow::anyhow!("API_COMPATIBILITY_BASE_INVALID: parameter has no location")
            })?;
            let name = parameter["name"].as_str().ok_or_else(|| {
                anyhow::anyhow!("API_COMPATIBILITY_BASE_INVALID: parameter has no name")
            })?;
            format!("{location}:{name}")
        };
        if parameters.insert(key.clone(), parameter).is_some() {
            bail!("API_COMPATIBILITY_BASE_INVALID: duplicate parameter {key}");
        }
    }
    Ok(parameters)
}

fn compare_request_body(
    old_operation: &Value,
    new_operation: &Value,
    operation: &str,
    changes: &mut Vec<String>,
) {
    let old = &old_operation["requestBody"];
    let new = &new_operation["requestBody"];
    if !old.is_null() {
        if new.is_null() {
            changes.push(format!("removed request body from {operation}"));
        } else if contract_shape(old) != contract_shape(new) {
            changes.push(format!("changed request body on {operation}"));
        }
    } else if !new.is_null() && (new["required"] == true || new.get("$ref").is_some()) {
        changes.push(format!("added required request body to {operation}"));
    }
}

fn compare_schema_contract(
    old: &Value,
    new: &Value,
    label: &str,
    changes: &mut Vec<String>,
) -> Result<()> {
    let (Some(old_object), Some(new_object)) = (old.as_object(), new.as_object()) else {
        if contract_shape(old) != contract_shape(new) {
            changes.push(format!("changed {label}"));
        }
        return Ok(());
    };
    if contract_shape_without(old, &["description", "properties", "required"])
        != contract_shape_without(new, &["description", "properties", "required"])
    {
        changes.push(format!("changed type or constraint of {label}"));
    }
    let old_required = string_set(&old["required"])?;
    let new_required = string_set(&new["required"])?;
    for field in old_required.difference(&new_required) {
        changes.push(format!("made {label}.{field} optional"));
    }
    for field in new_required.difference(&old_required) {
        changes.push(format!("made {label}.{field} required"));
    }
    if let Some(old_properties) = old_object.get("properties").and_then(Value::as_object) {
        let new_properties = new_object.get("properties").and_then(Value::as_object);
        for (field, old_property) in old_properties {
            let Some(new_property) = new_properties.and_then(|properties| properties.get(field))
            else {
                changes.push(format!("removed field {label}.{field}"));
                continue;
            };
            compare_schema_contract(
                old_property,
                new_property,
                &format!("{label}.{field}"),
                changes,
            )?;
        }
    }
    Ok(())
}

fn contract_shape_without(value: &Value, excluded: &[&str]) -> Value {
    let Some(object) = value.as_object() else {
        return contract_shape(value);
    };
    Value::Object(
        object
            .iter()
            .filter(|(key, _)| !excluded.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), contract_shape(value)))
            .collect(),
    )
}

fn contract_shape(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "description" | "summary" | "example" | "examples" | "externalDocs"
                    )
                })
                .map(|(key, value)| (key.clone(), contract_shape(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(contract_shape).collect()),
        _ => value.clone(),
    }
}

fn acquire_generation_lock(root: &Path, check: bool) -> Result<File> {
    let path = root.join(LOCK_PATH);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| {
            format!(
                "API_GENERATION_LOCK_INVALID: open committed lock file '{}'",
                path.display()
            )
        })?;
    let result = if check {
        file.try_lock_shared()
    } else {
        file.try_lock()
    };
    result.with_context(|| {
        "API_GENERATION_BUSY: another API generation or check invocation owns the Workspace lock"
    })?;
    Ok(file)
}

fn recover_incomplete_transactions(root: &Path, check: bool) -> Result<()> {
    let mut transactions = fs::read_dir(root)
        .context("API_GENERATION_RECOVERY_FAILED: inspect Workspace root")?
        .collect::<std::io::Result<Vec<_>>>()?;
    transactions.sort_by_key(fs::DirEntry::file_name);
    transactions.retain(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(".yydra-api-transaction-"))
    });
    if check && !transactions.is_empty() {
        bail!(
            "API_GENERATION_RECOVERY_REQUIRED: an interrupted API output transaction must be recovered by `yydra generate api`"
        );
    }
    for transaction in transactions {
        let path = transaction.path();
        let metadata = fs::symlink_metadata(&path).with_context(|| {
            format!(
                "API_GENERATION_RECOVERY_FAILED: inspect '{}'",
                path.display()
            )
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            bail!(
                "API_GENERATION_RECOVERY_FAILED: transaction path '{}' is not a real directory",
                path.display()
            );
        }
        recover_transaction(root, &path)?;
    }
    Ok(())
}

fn recover_transaction(root: &Path, transaction: &Path) -> Result<()> {
    let committed_marker = transaction.join("committed");
    if committed_marker.exists() {
        if fs::read(&committed_marker)
            .context("API_GENERATION_RECOVERY_FAILED: read committed transaction marker")?
            != b"committed\n"
        {
            bail!("API_GENERATION_RECOVERY_FAILED: committed transaction marker is malformed");
        }
        for relative in [CONTRACT_PATH, GENERATED_PATH, HISTORY_PATH, RECORD_PATH] {
            ensure_no_symlink_target(root, Path::new(relative))?;
            if !root.join(relative).exists() {
                bail!(
                    "API_GENERATION_RECOVERY_FAILED: committed output '{}' is missing",
                    root.join(relative).display()
                );
            }
        }
        return remove_path(transaction).with_context(|| {
            format!(
                "API_GENERATION_RECOVERY_FAILED: clean committed transaction '{}'",
                transaction.display()
            )
        });
    }
    let manifest_path = transaction.join("transaction-manifest.json");
    let manifest_bytes = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            remove_path(transaction).with_context(|| {
                format!(
                    "API_GENERATION_RECOVERY_FAILED: remove uncommenced transaction '{}'",
                    transaction.display()
                )
            })?;
            return Ok(());
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "API_GENERATION_RECOVERY_FAILED: read '{}'",
                    manifest_path.display()
                )
            });
        }
    };
    let manifest: ApiGenerationTransactionManifest = serde_json::from_slice(&manifest_bytes)
        .context("API_GENERATION_RECOVERY_FAILED: transaction manifest is malformed")?;
    let expected = [CONTRACT_PATH, GENERATED_PATH, HISTORY_PATH, RECORD_PATH];
    if manifest.schema_version != 1
        || manifest.outputs.len() != expected.len()
        || manifest
            .outputs
            .iter()
            .zip(expected)
            .any(|(output, expected)| output.path != expected)
    {
        bail!("API_GENERATION_RECOVERY_FAILED: transaction manifest output set is invalid");
    }
    for output in manifest.outputs.iter().rev() {
        let relative = Path::new(&output.path);
        ensure_safe_relative(relative)?;
        ensure_no_symlink_target(root, relative)?;
        let target = root.join(relative);
        let backup = transaction.join("old").join(relative);
        let replacement = transaction.join("new").join(relative);
        if backup.exists() {
            remove_path(&target)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&backup, &target).with_context(|| {
                format!(
                    "API_GENERATION_RECOVERY_FAILED: restore '{}'",
                    target.display()
                )
            })?;
        } else if output.had_original {
            if !target.exists() {
                bail!(
                    "API_GENERATION_RECOVERY_FAILED: original output '{}' and its backup are both missing",
                    target.display()
                );
            }
        } else if replacement.exists() {
            if target.exists() {
                bail!(
                    "API_GENERATION_RECOVERY_FAILED: uncommenced output '{}' unexpectedly exists",
                    target.display()
                );
            }
        } else {
            remove_path(&target)?;
        }
    }
    remove_path(transaction).with_context(|| {
        format!(
            "API_GENERATION_RECOVERY_FAILED: remove recovered transaction '{}'",
            transaction.display()
        )
    })
}

fn replace_outputs<'a>(
    root: &Path,
    outputs: impl IntoIterator<Item = (&'a Path, &'a Path)>,
) -> Result<()> {
    let transaction = Builder::new()
        .prefix(".yydra-api-transaction-")
        .tempdir_in(root)
        .context("API_GENERATION_TRANSACTION_FAILED: create same-filesystem transaction")?;
    let outputs = outputs.into_iter().collect::<Vec<_>>();
    for (source, relative) in &outputs {
        ensure_safe_relative(relative)?;
        ensure_no_symlink_target(root, relative)?;
        copy_path(source, &transaction.path().join("new").join(relative))?;
    }

    let manifest = ApiGenerationTransactionManifest {
        schema_version: 1,
        outputs: outputs
            .iter()
            .map(|(_, relative)| ApiGenerationTransactionOutput {
                path: relative.to_string_lossy().replace('\\', "/"),
                had_original: root.join(relative).exists(),
            })
            .collect(),
    };
    let manifest_temporary = transaction.path().join("transaction-manifest.json.tmp");
    let manifest_path = transaction.path().join("transaction-manifest.json");
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    manifest_bytes.push(b'\n');
    let mut manifest_file = File::create(&manifest_temporary)
        .context("API_GENERATION_TRANSACTION_FAILED: create transaction manifest")?;
    manifest_file
        .write_all(&manifest_bytes)
        .context("API_GENERATION_TRANSACTION_FAILED: write transaction manifest")?;
    manifest_file
        .sync_all()
        .context("API_GENERATION_TRANSACTION_FAILED: persist transaction manifest")?;
    drop(manifest_file);
    fs::rename(&manifest_temporary, &manifest_path)
        .context("API_GENERATION_TRANSACTION_FAILED: publish transaction manifest")?;
    sync_directory(transaction.path())
        .context("API_GENERATION_TRANSACTION_FAILED: persist transaction manifest entry")?;

    let mut swapped = Vec::new();
    for (index, (_, relative)) in outputs.iter().enumerate() {
        let target = root.join(relative);
        let backup = transaction.path().join("old").join(relative);
        let replacement = transaction.path().join("new").join(relative);
        if let Some(parent) = target.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            return Err(transaction_failure(
                transaction,
                root,
                &swapped,
                None,
                format!("prepare '{}': {error}", target.display()),
            ));
        }
        let had_original = target.exists();
        if had_original {
            if let Some(parent) = backup.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                return Err(transaction_failure(
                    transaction,
                    root,
                    &swapped,
                    None,
                    format!("prepare backup for '{}': {error}", target.display()),
                ));
            }
            if let Err(error) = fs::rename(&target, &backup) {
                return Err(transaction_failure(
                    transaction,
                    root,
                    &swapped,
                    None,
                    format!("preserve '{}': {error}", target.display()),
                ));
            }
        }
        let replacement_result = if std::env::var("YYDRA_TEST_API_GENERATION_FAIL_REPLACE_INDEX")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            == Some(index)
        {
            Err(std::io::Error::other("injected replacement failure"))
        } else {
            fs::rename(&replacement, &target)
        };
        if let Err(error) = replacement_result {
            return Err(transaction_failure(
                transaction,
                root,
                &swapped,
                had_original.then_some((backup.as_path(), target.as_path())),
                format!("replace '{}': {error}", target.display()),
            ));
        }
        swapped.push(((*relative).to_path_buf(), had_original));
        if std::env::var("YYDRA_TEST_API_GENERATION_CRASH_REPLACE_INDEX")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            == Some(index)
        {
            std::process::exit(86);
        }
    }
    if let Err(error) = mark_transaction_committed(transaction.path()) {
        return Err(transaction_failure(
            transaction,
            root,
            &swapped,
            None,
            format!("publish commit marker: {error}"),
        ));
    }
    if std::env::var_os("YYDRA_TEST_API_GENERATION_FAIL_CLEANUP").is_some() {
        let recovery_root = transaction.keep();
        bail!(
            "API_GENERATION_TRANSACTION_FAILED: committed outputs but cleanup was intentionally rejected; committed recovery data retained at '{}'",
            recovery_root.display()
        );
    }
    transaction
        .close()
        .context("API_GENERATION_TRANSACTION_FAILED: committed outputs but cleanup failed")
}

fn mark_transaction_committed(transaction: &Path) -> Result<()> {
    let temporary = transaction.join("committed.tmp");
    let marker = transaction.join("committed");
    let mut file = File::create(&temporary)
        .context("API_GENERATION_TRANSACTION_FAILED: create committed transaction marker")?;
    file.write_all(b"committed\n")
        .context("API_GENERATION_TRANSACTION_FAILED: write committed transaction marker")?;
    file.sync_all()
        .context("API_GENERATION_TRANSACTION_FAILED: persist committed transaction marker")?;
    drop(file);
    if std::env::var_os("YYDRA_TEST_API_GENERATION_CRASH_COMMIT_MARKER").is_some() {
        std::process::exit(87);
    }
    fs::rename(&temporary, &marker)
        .context("API_GENERATION_TRANSACTION_FAILED: publish committed transaction marker")?;
    sync_directory(transaction)
        .context("API_GENERATION_TRANSACTION_FAILED: persist committed transaction entry")
}

fn transaction_failure(
    transaction: TempDir,
    root: &Path,
    swapped: &[(PathBuf, bool)],
    current_restore: Option<(&Path, &Path)>,
    primary: String,
) -> anyhow::Error {
    let mut rollback_errors = Vec::new();
    if let Some((backup, target)) = current_restore
        && let Err(error) = fs::rename(backup, target)
    {
        rollback_errors.push(format!("restore '{}': {error}", target.display()));
    }
    rollback_errors.extend(rollback_outputs(root, transaction.path(), swapped));
    if rollback_errors.is_empty() {
        return anyhow::anyhow!("API_GENERATION_TRANSACTION_FAILED: {primary}; rollback completed");
    }
    let recovery_root = transaction.keep();
    anyhow::anyhow!(
        "API_GENERATION_TRANSACTION_FAILED: {primary}; rollback incomplete: {}; recovery data retained at '{}'",
        rollback_errors.join("; "),
        recovery_root.display()
    )
}

fn rollback_outputs(root: &Path, transaction: &Path, swapped: &[(PathBuf, bool)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (relative, had_original) in swapped.iter().rev() {
        let target = root.join(relative);
        if let Err(error) = remove_path(&target) {
            errors.push(format!(
                "remove replacement '{}': {error}",
                target.display()
            ));
            continue;
        }
        if *had_original
            && let Err(error) = fs::rename(transaction.join("old").join(relative), &target)
        {
            errors.push(format!("restore '{}': {error}", target.display()));
        }
    }
    errors
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn copy_path(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("API_STAGE_COPY_FAILED: inspect '{}'", source.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("API_STAGE_COPY_FAILED: generated output cannot be a symlink");
    }
    if metadata.is_dir() {
        fs::create_dir_all(destination)?;
        set_mode(destination, 0o755)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        set_mode(destination, 0o644)?;
    } else {
        bail!("API_STAGE_COPY_FAILED: unsupported generated output type");
    }
    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)?
        }
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn ensure_safe_relative(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        bail!("API_GENERATION_TRANSACTION_FAILED: unsafe output path");
    }
    Ok(())
}

fn ensure_no_symlink_target(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => bail!(
                "API_GENERATION_TRANSACTION_FAILED: output path '{}' traverses a symlink",
                current.display()
            ),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn directory_inventory(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut inventory = BTreeMap::new();
    collect_directory(root, root, &mut inventory)?;
    Ok(inventory)
}

fn collect_directory(
    root: &Path,
    current: &Path,
    inventory: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<()> {
    let metadata = fs::symlink_metadata(current)
        .with_context(|| format!("API_CLIENT_STAGE_INVALID: inspect '{}'", current.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("API_CLIENT_STAGE_INVALID: generated output contains a symlink");
    }
    if metadata.is_file() {
        inventory.insert(
            current.strip_prefix(root)?.to_path_buf(),
            fs::read(current)?,
        );
        return Ok(());
    }
    if !metadata.is_dir() {
        bail!("API_CLIENT_STAGE_INVALID: generated output contains a special file");
    }
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        collect_directory(root, &entry.path(), inventory)?;
    }
    Ok(())
}

fn directory_digest(root: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    for (path, bytes) in directory_inventory(root)? {
        digest.update(path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(bytes);
        digest.update([0]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn object_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .pointer(pointer)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("API_OPENAPI_PROFILE_INVALID: missing object at {pointer}"))
}

fn string_set(value: &Value) -> Result<BTreeSet<String>> {
    if value.is_null() {
        return Ok(BTreeSet::new());
    }
    value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("API_OPENAPI_PROFILE_INVALID: expected string array"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| anyhow::anyhow!("API_OPENAPI_PROFILE_INVALID: expected string"))
        })
        .collect()
}

fn require_additional_properties(
    schemas: &serde_json::Map<String, Value>,
    name: &str,
    expected: bool,
) -> Result<()> {
    if schema_at(schemas, name)?["additionalProperties"] != expected {
        bail!(
            "API_OPENAPI_UNKNOWN_FIELD_POLICY_INVALID: {name}.additionalProperties must be {expected}"
        );
    }
    Ok(())
}

fn schema_at<'a>(schemas: &'a serde_json::Map<String, Value>, name: &str) -> Result<&'a Value> {
    schemas.get(name).ok_or_else(|| {
        anyhow::anyhow!("API_OPENAPI_PROFILE_INVALID: required schema {name} is missing")
    })
}

fn require_schema_type(schema: &Value, expected: &str, field: &str) -> Result<()> {
    if schema["type"] != expected {
        bail!("API_OPENAPI_WIRE_TYPE_INVALID: {field} must be {expected}");
    }
    Ok(())
}

fn require_type_union(schema: &Value, expected: &[&str], field: &str) -> Result<()> {
    let actual = schema["type"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("API_OPENAPI_NULLABILITY_INVALID: {field}"))?
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        bail!("API_OPENAPI_NULLABILITY_INVALID: {field} has the wrong union");
    }
    Ok(())
}

fn is_lower_camel_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

fn validate_acknowledgements(values: &[String]) -> Result<()> {
    for value in values {
        if value.is_empty()
            || value.len() > 160
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_graphic() || byte == b' ')
        {
            bail!(
                "API_ACKNOWLEDGEMENT_INVALID: references must be non-empty printable text up to 160 bytes"
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}
