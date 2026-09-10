// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;
use tempfile::{TempDir, tempdir};

#[test]
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
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
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
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
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
fn frontend_resolves_the_generated_package_after_build_output_cleanup() {
    let fixture = Fixture::new();
    let lock = fixture.root.join("Cargo.lock");
    // A valid lock may have a different textual ordering after product-name rendering.
    let source = fs::read_to_string(&lock).unwrap();
    let (header, packages) = source.split_once("[[package]]").unwrap();
    let mut packages = packages.split("[[package]]").collect::<Vec<_>>();
    packages.reverse();
    fs::write(
        &lock,
        format!("{header}[[package]]{}", packages.join("[[package]]")),
    )
    .unwrap();
    let locked_bytes = fs::read(&lock).unwrap();
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
        fs::remove_dir_all(fixture.api_output()).unwrap();
        assert!(
            fs::read(&lock).unwrap() == locked_bytes,
            "recovery changed Cargo.lock"
        );
    }
}

#[test]
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
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
    let out = walk_api_output(&fixture.target);
    let obsolete = out.join("public-api/obsolete.ts");
    fs::write(&obsolete, "partial output").unwrap();
    assert_success(&fixture.generate().output().unwrap());
    assert!(!obsolete.exists());
    assert_eq!(fs::read_to_string(cache).unwrap(), "keep");
}

#[test]
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
fn sequential_workspaces_sharing_a_cargo_cache_keep_their_own_generated_clients() {
    let first = Fixture::new();
    assert_success(&first.generate().output().unwrap());
    let first_client = first
        .root
        .join("frontend/node_modules/@yydra/generated-api/fetch/client.ts");
    let expected = fs::read(&first_client).unwrap();
    let mut second = Fixture::new();
    second.target = first.target.clone();
    let mut different = expected.clone();
    different.extend_from_slice(b"// Another product's client\n");
    fs::write(second.client.join("fetch/client.ts"), different).unwrap();
    assert_success(&second.generate().output().unwrap());
    assert_eq!(fs::read(first_client).unwrap(), expected);
}

#[test]
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
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
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
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
        let config = frontend.join("orval.config.mjs");
        let mut source = fs::read_to_string(&config).unwrap();
        source.push_str("\n// force a new generator attempt\n");
        fs::write(config, source).unwrap();
        let generator = fixture.generate();
        let mut command = Command::new(&npm);
        command
            .args(["run", "typecheck"])
            .current_dir(&frontend)
            .envs(
                generator
                    .get_envs()
                    .map(|(key, value)| (key, value.unwrap())),
            );
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
}

impl Fixture {
    fn api_output(&self) -> PathBuf {
        fs::canonicalize(self.root.join("frontend/node_modules/@yydra/generated-api"))
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
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
        let api_build = root.join("crates/api-build");
        fs::create_dir_all(api_build.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/api-build\"]\nresolver = \"3\"\n",
        )
        .unwrap();
        let helper = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../yydra-build")
            .canonicalize()
            .unwrap();
        fs::write(api_build.join("Cargo.toml"), format!(
            "[package]\nname = \"api-product-api-build\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[build-dependencies]\nyydra-build = {{ path = {:?} }}\n", helper)).unwrap();
        fs::write(api_build.join("src/lib.rs"), "// Build target\n").unwrap();
        fs::write(api_build.join("build.rs"), r#"fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    yydra_build::generate_api(include_bytes!("../../../fixture.json"), &yydra_build::ApiBuild {
        frontend: &manifest.join("../../frontend"), out_dir: &std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()),
    }).unwrap();
}
"#).unwrap();
        assert_success(
            &Command::new("cargo")
                .args(["generate-lockfile", "--offline"])
                .current_dir(&root)
                .output()
                .unwrap(),
        );
        for tool in ["typescript", "zod"] {
            let dir = root.join("frontend/node_modules").join(tool);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("package.json"), "{}").unwrap();
        }
        executable(
            &bin.join("npm"),
            r#"#!/bin/sh
if [ -n "$YYDRA_GENERATED_API_OUTPUT" ]; then
  /bin/mkdir -p "$YYDRA_GENERATED_API_OUTPUT"
  /bin/cp -R "$YYDRA_FAKE_CLIENT/." "$YYDRA_GENERATED_API_OUTPUT/"
  echo generated >> "$YYDRA_FAKE_GENERATIONS"
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
        }
    }

    fn generate(&self) -> Command {
        let mut command = Command::new("node");
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command
            .arg("scripts/prepare-api.mjs")
            .current_dir(self.root.join("frontend"))
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("CARGO_TARGET_DIR", &self.target)
            .env("CARGO_NET_OFFLINE", "true")
            .env("CARGO_BUILD_JOBS", "2")
            .env("YYDRA_FAKE_GENERATIONS", self.root.join("generations"))
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

fn walk_api_output(root: &Path) -> PathBuf {
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.join("openapi.json").is_file() {
                return path;
            }
            let found = walk_api_output(&path);
            if !found.as_os_str().is_empty() {
                return found;
            }
        }
    }
    PathBuf::new()
}

#[test]
#[ignore = "consumer integration: requires Node/npm and compilation of the consumer api-build crate"]
fn unchanged_inputs_reuse_generation_and_missing_frontend_links_are_restored() {
    let fixture = Fixture::new();
    assert_success(&fixture.generate().output().unwrap());
    let count = fs::read_to_string(fixture.root.join("generations")).unwrap();
    fs::remove_file(
        fixture
            .root
            .join("frontend/node_modules/@yydra/generated-api"),
    )
    .unwrap();
    assert_success(&fixture.generate().output().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.root.join("generations")).unwrap(),
        count
    );
    assert!(
        fixture
            .api_output()
            .join("public-api/fetch/client.ts")
            .is_file()
    );
}
