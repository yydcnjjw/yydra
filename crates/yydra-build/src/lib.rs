// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

mod error;
use error::BuildContext;
pub use error::{Error, ErrorKind};
type Result<T> = std::result::Result<T, Error>;
use serde_json::Value;
use sha2::{Digest, Sha256};

macro_rules! failure {
    ($kind:expr, $($detail:tt)*) => { Error::input($kind, format!($($detail)*)) };
}
macro_rules! fail {
    ($kind:expr, $($detail:tt)*) => { return Err(failure!($kind, $($detail)*)) };
}

const ORVAL_VERSION: &str = "8.27.0";
const EXACT_DECIMAL_PATTERN: &str = r"^-?(0|[1-9][0-9]*)(\.[0-9]+)?$";
const HTTP_METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Filesystem inputs for a product build script. `out_dir` must be its Cargo OUT_DIR.
pub struct ApiBuild<'a> {
    pub frontend: &'a Path,
    pub out_dir: &'a Path,
}

/// Generate and validate the current Public API Contract and Generated Client.
/// The caller supplies the contract; this function never invokes Cargo.
pub fn generate_api(openapi: &[u8], config: &ApiBuild<'_>) -> Result<()> {
    validate_openapi_profile(openapi)?;
    let frontend = config.frontend.canonicalize().build_context(
        ErrorKind::WorkspaceInvalid,
        "locate frontend generation inputs",
    )?;
    for relative in [
        "orval.config.mjs",
        "package.json",
        "package-lock.json",
        "scripts",
        "node_modules/orval/package.json",
        "node_modules/typescript/package.json",
        "node_modules/zod/package.json",
    ] {
        println!(
            "cargo::rerun-if-changed={}",
            frontend.join(relative).display()
        );
    }
    println!("cargo::rerun-if-env-changed=PATH");
    validate_generator_version(&frontend)?;
    if !config.out_dir.is_absolute() {
        fail!(ErrorKind::OutputPrepareFailed, "OUT_DIR must be absolute");
    }
    // Cargo may reuse one OUT_DIR for same-named packages in different checkouts.
    let workspace_key = hex::encode(Sha256::digest(frontend.as_os_str().as_encoded_bytes()));
    let relative = Path::new("yydra-api").join(workspace_key);
    ensure_no_symlink_target(config.out_dir, &relative)?;
    let output = config.out_dir.join(&relative);
    if output.exists() {
        fs::remove_dir_all(&output)
            .build_context(ErrorKind::OutputPrepareFailed, "clean owned API outputs")?;
    }
    fs::create_dir_all(&output)
        .build_context(ErrorKind::OutputPrepareFailed, "create API build directory")?;
    let contract = output.join("openapi.json");
    fs::write(&contract, openapi)
        .build_context(ErrorKind::OpenapiExportFailed, "write derived OpenAPI")?;
    let client = output.join("public-api");
    generate_client(&frontend, &contract, &client)?;
    validate_generated_client(&client)?;
    let files = directory_inventory(&client)?
        .keys()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    fs::write(
        client.join("package.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "name": "@yydra/generated-api", "private": true, "type": "module",
            "license": "MIT OR Apache-2.0", "exports": {"./*": "./*.ts"}, "files": files
        }))?,
    )
    .build_context(
        ErrorKind::ClientOutputInvalid,
        "write generated package metadata",
    )?;
    println!("cargo::rustc-env=YYDRA_API_OUTPUT={}", output.display());
    Ok(())
}

fn npm_program() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

fn generate_client(root: &Path, openapi: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output).build_context(
        ErrorKind::OutputPrepareFailed,
        format!("create '{}'", output.display()),
    )?;
    let config = root.join("orval.config.mjs");
    let npm = npm_program();
    run_generation_command(
        npm,
        duct::cmd!(
            npm,
            "exec",
            "--offline",
            "--",
            "orval",
            "--config",
            config,
            "--clean",
            "--fail-on-warnings"
        )
        .dir(root)
        .env("YYDRA_OPENAPI_INPUT", openapi)
        .env("YYDRA_GENERATED_API_OUTPUT", output),
        ErrorKind::ClientGenerationFailed,
    )?;
    let tsconfig = output
        .parent()
        .build_context(
            ErrorKind::ClientTypecheckFailed,
            "generated output has no parent",
        )?
        .join("generated-client-tsconfig.json");
    let frontend = root;
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
    fs::write(&tsconfig, config).build_context(
        ErrorKind::ClientTypecheckFailed,
        "write generated-client tsconfig",
    )?;
    run_generation_command(
        npm,
        duct::cmd!(npm, "exec", "--offline", "--", "tsc", "--project", tsconfig).dir(root),
        ErrorKind::ClientTypecheckFailed,
    )
}

fn validate_generator_version(root: &Path) -> Result<()> {
    let package = root.join("node_modules/orval/package.json");
    let package: Value = serde_json::from_slice(&fs::read(&package).build_context(
        ErrorKind::ClientToolVersionInvalid,
        "project-local Orval is missing; run `moon run product:setup`",
    )?)
    .build_context(
        ErrorKind::ClientToolVersionInvalid,
        "project-local Orval package metadata is malformed",
    )?;
    if package["version"] != ORVAL_VERSION {
        fail!(
            ErrorKind::ClientToolVersionInvalid,
            "expected project-local orval@{ORVAL_VERSION}, found {}",
            package["version"]
        );
    }
    Ok(())
}

fn run_generation_command(program: &str, command: duct::Expression, kind: ErrorKind) -> Result<()> {
    let output = command
        .stdin_null()
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .build_context(kind, format!("start {program}"))?;
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
    Err(Error::command(kind, program, output.status.code(), detail))
}

fn validate_openapi_profile(bytes: &[u8]) -> Result<()> {
    let document: Value = serde_json::from_slice(bytes).build_context(
        ErrorKind::OpenapiProfileInvalid,
        "generated document is not JSON",
    )?;
    if document["openapi"] != "3.1.0" {
        fail!(ErrorKind::OpenapiProfileInvalid, "expected OpenAPI 3.1.0");
    }
    if document.get("security").is_some() {
        fail!(
            ErrorKind::OpenapiProfileInvalid,
            "global security is outside the supported Public API profile"
        );
    }
    let components = object_at(&document, "/components")?;
    if components.keys().any(|key| key != "schemas") {
        fail!(
            ErrorKind::OpenapiProfileInvalid,
            "reusable parameters, request bodies, responses, headers, examples, links, callbacks, and security schemes are outside the supported Public API profile"
        );
    }
    let paths = object_at(&document, "/paths")?;
    let mut operation_ids = BTreeSet::new();
    for (path, path_item) in paths {
        let operations = path_item.as_object().ok_or_else(|| {
            failure!(
                ErrorKind::OpenapiProfileInvalid,
                "path {path} is not an object"
            )
        })?;
        if operations
            .keys()
            .any(|method| !HTTP_METHODS.contains(&method.as_str()))
        {
            fail!(
                ErrorKind::OpenapiProfileInvalid,
                "path-item declarations outside HTTP operations are not supported at {path}"
            );
        }
        for (method, operation) in operations {
            let id = operation["operationId"].as_str().ok_or_else(|| {
                failure!(
                    ErrorKind::OpenapiOperationIdInvalid,
                    "{method} {path} has no operationId"
                )
            })?;
            if !is_lower_camel_identifier(id) || !operation_ids.insert(id.to_owned()) {
                fail!(
                    ErrorKind::OpenapiOperationIdInvalid,
                    "operationId '{id}' must be globally unique lowerCamelCase"
                );
            }
            let responses = operation["responses"].as_object().ok_or_else(|| {
                failure!(ErrorKind::OpenapiProfileInvalid, "{id} has no responses")
            })?;
            for (status, response) in responses {
                let status_number = status.parse::<u16>().build_context(
                    ErrorKind::OpenapiProfileInvalid,
                    format!("{id} uses non-numeric status {status}"),
                )?;
                let content = response["content"].as_object().ok_or_else(|| {
                    failure!(
                        ErrorKind::OpenapiProfileInvalid,
                        "{id} status {status} has no declared content"
                    )
                })?;
                let required = if status_number >= 400 {
                    "application/problem+json"
                } else {
                    "application/json"
                };
                if content.len() != 1 || !content.contains_key(required) {
                    fail!(
                        ErrorKind::OpenapiContentTypeInvalid,
                        "{id} status {status} must declare only {required}"
                    );
                }
            }
        }
    }
    if operation_ids.is_empty() {
        fail!(
            ErrorKind::OpenapiProfileInvalid,
            "Public API has zero operations"
        );
    }

    let schemas = object_at(&document, "/components/schemas")?;
    for (name, schema) in schemas {
        if schema["type"] == "object" {
            let properties = schema["properties"].as_object().ok_or_else(|| {
                failure!(
                    ErrorKind::OpenapiProfileInvalid,
                    "schema {name} has no properties"
                )
            })?;
            for field in properties.keys() {
                if !is_lower_camel_identifier(field) {
                    fail!(
                        ErrorKind::OpenapiFieldNameInvalid,
                        "schema {name} field '{field}' is not lowerCamelCase"
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
            fail!(
                ErrorKind::OpenapiRequirednessInvalid,
                "FrameworkContractProfile.{field} must be required"
            );
        }
    }
    if required.contains("optionalNote") {
        fail!(
            ErrorKind::OpenapiRequirednessInvalid,
            "optionalNote must remain optional"
        );
    }
    let properties = object_at(profile, "/properties")?;
    require_schema_type(&properties["opaqueId"], "string", "opaqueId")?;
    require_schema_type(&properties["exactAmount"], "string", "exactAmount")?;
    if properties["exactAmount"]["pattern"] != EXACT_DECIMAL_PATTERN {
        fail!(
            ErrorKind::OpenapiDecimalInvalid,
            "exactAmount must carry the canonical decimal pattern"
        );
    }
    if schemas["FrameworkContractCreate"]["properties"]["exactAmount"]["pattern"]
        != properties["exactAmount"]["pattern"]
    {
        fail!(
            ErrorKind::OpenapiDecimalInvalid,
            "create and response exactAmount fields must share the exact decimal rule"
        );
    }
    if properties["occurredAt"]["format"] != "date-time" {
        fail!(
            ErrorKind::OpenapiTimestampInvalid,
            "occurredAt must use date-time"
        );
    }
    if properties["occurredAt"]["pattern"] != "Z$" {
        fail!(
            ErrorKind::OpenapiTimestampInvalid,
            "occurredAt must be UTC with a Z suffix"
        );
    }
    require_schema_type(&properties["safeCount"], "integer", "safeCount")?;
    let safe_minimum = properties["safeCount"]["minimum"].as_i64();
    let safe_maximum = properties["safeCount"]["maximum"].as_i64();
    if safe_minimum.is_none_or(|value| value < -9_007_199_254_740_991)
        || safe_maximum.is_none_or(|value| value > 9_007_199_254_740_991)
    {
        fail!(
            ErrorKind::OpenapiSafeIntegerInvalid,
            "safeCount must be bounded to the JavaScript safe range"
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
        fail!(
            ErrorKind::OpenapiShapeReuseInvalid,
            "create, response, and patch schemas must remain separate"
        );
    }

    Ok(())
}

fn validate_generated_client(root: &Path) -> Result<()> {
    let inventory = directory_inventory(root)?;
    let client = inventory.get(Path::new("fetch/client.ts")).ok_or_else(|| {
        failure!(
            ErrorKind::ClientOutputInvalid,
            "Orval did not emit client.ts"
        )
    })?;
    let client = String::from_utf8_lossy(client);
    for marker in [
        "SPDX-License-Identifier: MIT OR Apache-2.0",
        "Do not edit manually.",
        "getFrameworkContractProfile",
        "fetchFn",
        "FrameworkContractProfile.parse",
    ] {
        if !client.contains(marker) {
            fail!(
                ErrorKind::ClientOutputInvalid,
                "client.ts is missing {marker:?}"
            );
        }
    }
    for expected in [
        "fetch/schemas/frameworkContractProfile.zod.ts",
        "fetch/schemas/problemDetails.zod.ts",
        "request/schemas/frameworkContractCreate.zod.ts",
        "request/schemas/frameworkContractPatch.zod.ts",
    ] {
        if !inventory.contains_key(Path::new(expected)) {
            fail!(
                ErrorKind::ClientOutputInvalid,
                "Orval did not emit {expected}"
            );
        }
    }
    for (response, bytes) in inventory.iter().filter(|(path, _)| {
        path.starts_with("fetch/schemas") && path.extension().is_some_and(|value| value == "ts")
    }) {
        let source = String::from_utf8_lossy(bytes);
        if source.contains("strictObject") || source.contains(".strict()") {
            fail!(
                ErrorKind::ClientOutputInvalid,
                "Fetch schema {} must tolerate additive unknown fields",
                response.display()
            );
        }
    }
    for (request, bytes) in inventory.iter().filter(|(path, _)| {
        path.starts_with("request/schemas") && path.extension().is_some_and(|value| value == "ts")
    }) {
        let source = String::from_utf8_lossy(bytes);
        if source.contains("zod.object(") && !source.contains(".strict()") {
            fail!(
                ErrorKind::ClientOutputInvalid,
                "request schema {} must reject unknown fields",
                request.display()
            );
        }
    }
    Ok(())
}

fn ensure_no_symlink_target(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => fail!(
                ErrorKind::OutputPrepareFailed,
                "output path '{}' traverses a symlink",
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
    let metadata = fs::symlink_metadata(current).build_context(
        ErrorKind::ClientOutputInvalid,
        format!("inspect '{}'", current.display()),
    )?;
    if metadata.file_type().is_symlink() {
        fail!(
            ErrorKind::ClientOutputInvalid,
            "generated output contains a symlink"
        );
    }
    if metadata.is_file() {
        inventory.insert(
            current.strip_prefix(root)?.to_path_buf(),
            fs::read(current)?,
        );
        return Ok(());
    }
    if !metadata.is_dir() {
        fail!(
            ErrorKind::ClientOutputInvalid,
            "generated output contains a special file"
        );
    }
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        collect_directory(root, &entry.path(), inventory)?;
    }
    Ok(())
}

fn object_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .pointer(pointer)
        .and_then(Value::as_object)
        .ok_or_else(|| {
            failure!(
                ErrorKind::OpenapiProfileInvalid,
                "missing object at {pointer}"
            )
        })
}

fn string_set(value: &Value) -> Result<BTreeSet<String>> {
    if value.is_null() {
        return Ok(BTreeSet::new());
    }
    value
        .as_array()
        .ok_or_else(|| failure!(ErrorKind::OpenapiProfileInvalid, "expected string array"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| failure!(ErrorKind::OpenapiProfileInvalid, "expected string"))
        })
        .collect()
}

fn require_additional_properties(
    schemas: &serde_json::Map<String, Value>,
    name: &str,
    expected: bool,
) -> Result<()> {
    if schema_at(schemas, name)?["additionalProperties"] != expected {
        fail!(
            ErrorKind::OpenapiUnknownFieldPolicyInvalid,
            "{name}.additionalProperties must be {expected}"
        );
    }
    Ok(())
}

fn schema_at<'a>(schemas: &'a serde_json::Map<String, Value>, name: &str) -> Result<&'a Value> {
    schemas.get(name).ok_or_else(|| {
        failure!(
            ErrorKind::OpenapiProfileInvalid,
            "required schema {name} is missing"
        )
    })
}

fn require_schema_type(schema: &Value, expected: &str, field: &str) -> Result<()> {
    if schema["type"] != expected {
        fail!(
            ErrorKind::OpenapiWireTypeInvalid,
            "{field} must be {expected}"
        );
    }
    Ok(())
}

fn require_type_union(schema: &Value, expected: &[&str], field: &str) -> Result<()> {
    let actual = schema["type"]
        .as_array()
        .ok_or_else(|| failure!(ErrorKind::OpenapiNullabilityInvalid, "{field}"))?
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        fail!(
            ErrorKind::OpenapiNullabilityInvalid,
            "{field} has the wrong union"
        );
    }
    Ok(())
}

fn is_lower_camel_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}
