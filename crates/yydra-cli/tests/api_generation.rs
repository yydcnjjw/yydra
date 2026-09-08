// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;
use tempfile::{TempDir, tempdir};

#[test]
fn generates_rebuildable_outputs_in_the_configured_cargo_target_directory() {
    let fixture = Fixture::new();
    let output = fixture.generate().output().expect("generate API");
    assert_success(&output);
    let contract = fixture.api_output().join("openapi.json");
    assert_eq!(
        fs::read(contract).unwrap(),
        fs::read(&fixture.openapi).unwrap()
    );
    assert!(
        fixture
            .api_output()
            .join("public-api/fetch/client.ts")
            .is_file()
    );
}

#[test]
fn creates_only_authored_api_inputs_and_generates_without_full_workspace_identity() {
    let fixture = Fixture::new();
    for relative in [
        "contracts/openapi.json",
        "frontend/src/generated/public-api",
        ".yydra/api-generation.json",
        ".yydra/api-generation-history.json",
        ".yydra/api-generation.lock",
    ] {
        assert!(
            !fixture.root.join(relative).exists(),
            "unexpected committed output {relative}"
        );
    }
    fs::write(
        fixture.root.join(".yydra/origin.toml"),
        "old or edited origin\n",
    )
    .unwrap();
    fs::remove_file(fixture.root.join("LICENSE-MIT")).unwrap();
    assert_success(&fixture.generate().output().unwrap());
    let doctor = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .arg("doctor")
        .arg(&fixture.root)
        .output()
        .unwrap();
    assert!(
        !doctor.status.success(),
        "doctor still verifies complete identity"
    );
}

#[test]
fn frontend_resolves_the_generated_package_after_build_output_cleanup() {
    let fixture = Fixture::new();
    for _ in 0..2 {
        assert_success(&fixture.generate().output().unwrap());
        let package = fixture
            .root
            .join("frontend/node_modules/@yydra/generated-api");
        assert!(
            fs::canonicalize(&package)
                .unwrap()
                .starts_with(&fixture.target)
        );
        assert!(package.join("fetch/client.ts").is_file());
        fs::remove_dir_all(fixture.target.join("yydra/api")).unwrap();
    }
}

#[test]
fn failed_generation_can_be_rebuilt_without_recovery_and_preserves_other_build_outputs() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.target).unwrap();
    let cache = fixture.target.join("unrelated-cache");
    fs::write(&cache, "keep").unwrap();
    let failed = fixture
        .generate()
        .env("YYDRA_FAKE_FAIL", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(
        !fixture
            .root
            .join("frontend/node_modules/@yydra/generated-api")
            .exists()
    );
    let obsolete = fixture.api_output().join("public-api/obsolete.ts");
    fs::write(&obsolete, "partial output").unwrap();
    assert_success(&fixture.generate().output().unwrap());
    assert!(!obsolete.exists());
    assert_eq!(fs::read_to_string(cache).unwrap(), "keep");
}

#[test]
fn sequential_workspaces_sharing_a_cargo_cache_keep_their_own_generated_clients() {
    let first = Fixture::new();
    assert_success(&first.generate().output().unwrap());
    let first_client = first
        .root
        .join("frontend/node_modules/@yydra/generated-api/fetch/client.ts");
    let expected = fs::read(&first_client).unwrap();
    let second = Fixture::new();
    fs::write(
        &second.metadata,
        json!({"target_directory": first.target, "workspace_root": second.root}).to_string(),
    )
    .unwrap();
    let mut different = expected.clone();
    different.extend_from_slice(b"// Another product's client\n");
    fs::write(second.client.join("fetch/client.ts"), different).unwrap();
    assert_success(&second.generate().output().unwrap());
    assert_eq!(fs::read(first_client).unwrap(), expected);
}

#[test]
fn rejects_invalid_current_schema_client_and_generator_version() {
    for (relative, contents, code) in [
        ("fixture.json", "{}", "API_OPENAPI_PROFILE_INVALID"),
        (
            "fixture-client/fetch/client.ts",
            "export {};",
            "API_CLIENT_OUTPUT_INVALID",
        ),
        (
            "product/frontend/node_modules/orval/package.json",
            r#"{"version":"0.0.0"}"#,
            "API_CLIENT_TOOL_VERSION_INVALID",
        ),
    ] {
        let fixture = Fixture::new();
        fs::write(fixture._sandbox.path().join(relative), contents).unwrap();
        let output = fixture.generate().output().unwrap();
        assert!(!output.status.success(), "accepted {relative}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn frontend_entrypoint_stops_on_generation_failure_and_runs_after_repair() {
    let fixture = Fixture::new();
    assert_success(&fixture.generate().output().unwrap());
    let frontend = fixture.root.join("frontend");
    let package_path = frontend.join("package.json");
    let mut package: serde_json::Value =
        serde_json::from_slice(&fs::read(&package_path).unwrap()).unwrap();
    package["scripts"]["typecheck"] =
        json!("node -e \"require('node:fs').writeFileSync('consumer-ran', 'yes')\"");
    fs::write(&package_path, serde_json::to_vec(&package).unwrap()).unwrap();
    let npm = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("npm"))
        .find(|path| path.is_file())
        .expect("real npm installed for frontend entrypoint test");
    let run = |fail: bool| {
        let generator = fixture.generate();
        let mut command = Command::new(&npm);
        command
            .args(["run", "typecheck"])
            .current_dir(&frontend)
            .envs(
                generator
                    .get_envs()
                    .map(|(key, value)| (key, value.unwrap())),
            )
            .env("YYDRA_EXECUTABLE", env!("CARGO_BIN_EXE_yydra"));
        if fail {
            command.env("YYDRA_FAKE_FAIL", "1");
        }
        command.output().unwrap()
    };
    assert!(!run(true).status.success());
    assert!(!frontend.join("consumer-ran").exists());
    assert_success(&run(false));
    assert_eq!(
        fs::read_to_string(frontend.join("consumer-ran")).unwrap(),
        "yes"
    );
}

struct Fixture {
    _sandbox: TempDir,
    root: PathBuf,
    target: PathBuf,
    bin: PathBuf,
    openapi: PathBuf,
    client: PathBuf,
    metadata: PathBuf,
}

impl Fixture {
    fn api_output(&self) -> PathBuf {
        fs::read_dir(self.target.join("yydra/api"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
    }

    fn new() -> Self {
        let sandbox = tempdir().unwrap();
        let root = sandbox.path().join("product");
        assert_success(
            &Command::new(env!("CARGO_BIN_EXE_yydra"))
                .args([
                    "new",
                    root.to_str().unwrap(),
                    "--product-name",
                    "API Product",
                    "--product-id",
                    "api-product",
                    "--product-source-license",
                    "Apache-2.0",
                ])
                .output()
                .unwrap(),
        );
        let target = sandbox.path().join("configured target");
        let metadata = sandbox.path().join("metadata.json");
        fs::write(
            &metadata,
            json!({"target_directory": target, "workspace_root": root}).to_string(),
        )
        .unwrap();
        let openapi = sandbox.path().join("fixture.json");
        let decimal = json!({"type": "string", "pattern": "^-?(0|[1-9][0-9]*)(\\.[0-9]+)?$"});
        fs::write(&openapi, json!({
            "openapi": "3.1.0",
            "info": {"title": "API fixture", "version": "1.0.0"},
            "paths": {"/profile": {"get": {
                "operationId": "getFrameworkContractProfile",
                "responses": {"200": {"description": "Profile", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/FrameworkContractProfile"}}}}}
            }}},
            "components": {"schemas": {
                "FrameworkContractCreate": {"type": "object", "additionalProperties": false, "properties": {"exactAmount": decimal}},
                "FrameworkContractPatch": {"type": "object", "additionalProperties": false, "properties": {"optionalNote": {"type": "string"}}},
                "FrameworkContractProfile": {"type": "object", "additionalProperties": true,
                    "required": ["opaqueId", "occurredAt", "safeCount", "exactAmount", "items", "nullableNote"],
                    "properties": {
                        "opaqueId": {"type": "string"},
                        "occurredAt": {"type": "string", "format": "date-time", "pattern": "Z$"},
                        "safeCount": {"type": "integer", "minimum": -9007199254740991_i64, "maximum": 9007199254740991_i64},
                        "exactAmount": decimal, "items": {"type": "array", "items": {"type": "string"}},
                        "nullableNote": {"type": ["null", "string"]}, "optionalNote": {"type": "string"}
                    }
                },
                "ProblemDetails": {"type": "object", "additionalProperties": true, "properties": {"title": {"type": "string"}}}
            }}
        }).to_string()).unwrap();
        let client = sandbox.path().join("fixture-client");
        for (relative, source) in [
            (
                "fetch/client.ts",
                "// SPDX-License-Identifier: MIT OR Apache-2.0\n// Do not edit manually.\n// getFrameworkContractProfile fetchFn FrameworkContractProfile.parse\n",
            ),
            (
                "fetch/schemas/frameworkContractProfile.zod.ts",
                "export {};\n",
            ),
            ("fetch/schemas/problemDetails.zod.ts", "export {};\n"),
            (
                "request/schemas/frameworkContractCreate.zod.ts",
                "export {};\n",
            ),
            (
                "request/schemas/frameworkContractPatch.zod.ts",
                "export {};\n",
            ),
        ] {
            let path = client.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        let orval = root.join("frontend/node_modules/orval");
        fs::create_dir_all(&orval).unwrap();
        fs::write(orval.join("package.json"), r#"{"version":"8.27.0"}"#).unwrap();
        let bin = sandbox.path().join("bin");
        fs::create_dir(&bin).unwrap();
        executable(
            &bin.join("cargo"),
            r#"#!/bin/sh
if [ "$1" = metadata ]; then
  /bin/cat "$YYDRA_FAKE_METADATA"
  exit 0
fi
for argument in "$@"; do last="$argument"; done
/bin/mkdir -p "$(/usr/bin/dirname "$last")"
/bin/cp "$YYDRA_FAKE_OPENAPI" "$last"
"#,
        );
        executable(
            &bin.join("npm"),
            r#"#!/bin/sh
if [ -n "$YYDRA_GENERATED_API_OUTPUT" ]; then
  /bin/mkdir -p "$YYDRA_GENERATED_API_OUTPUT"
  /bin/cp -R "$YYDRA_FAKE_CLIENT/." "$YYDRA_GENERATED_API_OUTPUT/"
  if [ -n "$YYDRA_FAKE_FAIL" ]; then exit 7; fi
fi
"#,
        );
        Self {
            _sandbox: sandbox,
            root,
            target,
            bin,
            openapi,
            client,
            metadata,
        }
    }

    fn generate(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yydra"));
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command
            .args(["generate", "api"])
            .arg(&self.root)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("YYDRA_FAKE_METADATA", &self.metadata)
            .env("YYDRA_FAKE_OPENAPI", &self.openapi)
            .env("YYDRA_FAKE_CLIENT", &self.client);
        command
    }
}

fn executable(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
