// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::tempdir;

static HEAVY_CHECK_LOCK: Mutex<()> = Mutex::new(());

fn create_workspace(destination: &Path, product_id: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            destination.to_str().expect("UTF-8 destination"),
            "--product-name",
            "Check Fixture",
            "--product-id",
            product_id,
            "--product-source-license",
            "MIT OR Apache-2.0",
        ])
        .output()
        .expect("create Product Workspace");
    assert!(
        output.status.success(),
        "creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn refresh_fixture_lock(workspace: &Path) {
    let output = Command::new("cargo")
        .args(["generate-lockfile", "--offline"])
        .current_dir(workspace)
        .output()
        .expect("refresh fixture lock");
    assert!(
        output.status.success(),
        "lock refresh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("make fixture executable");
}

fn check(workspace: &Path, evidence: &Path, nodes: &[&str]) -> Output {
    let _heavy_guard = nodes
        .iter()
        .any(|node| {
            node.starts_with("api.")
                || node.starts_with("frontend.")
                || node.starts_with("h5.")
                || matches!(
                    *node,
                    "rust.compile" | "rust.clippy" | "rust.test" | "rust.doctest"
                )
        })
        .then(|| {
            HEAVY_CHECK_LOCK
                .lock()
                .expect("lock heavyweight check fixture")
        });
    let mut command = Command::new(env!("CARGO_BIN_EXE_yydra"));
    command.args([
        "--message-format=json",
        "check",
        workspace.to_str().expect("UTF-8 workspace"),
        "--evidence-dir",
        evidence.to_str().expect("UTF-8 evidence path"),
    ]);
    for node in nodes {
        command.args(["--node", node]);
    }
    command.output().expect("run Mechanical Quality Contract")
}

fn events(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8(output.stdout.clone())
        .expect("UTF-8 JSON Lines")
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("invalid JSON Line {line:?}: {error}"))
        })
        .collect()
}

fn node<'a>(events: &'a [serde_json::Value], id: &str) -> &'a serde_json::Value {
    events
        .iter()
        .find(|event| event["event"] == "check-node" && event["nodeId"] == id)
        .unwrap_or_else(|| panic!("missing check-node event for {id}: {events:#?}"))
}

#[test]
fn json_lines_and_human_output_are_views_of_the_same_node_result() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("model-reader");
    create_workspace(&workspace, "model-reader");

    let json = check(
        &workspace,
        &sandbox.path().join("json-evidence"),
        &["origin.exact-distribution"],
    );
    assert!(
        json.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&json.stderr)
    );
    let json_events = events(&json);
    let result = node(&json_events, "origin.exact-distribution");
    assert_eq!(result["schemaVersion"], 1);
    assert_eq!(result["outcome"], "pass");
    assert_eq!(result["prerequisites"], serde_json::json!([]));
    assert!(result["durationMs"].is_u64());
    assert!(result["proves"].is_string());
    assert!(result["doesNotProve"].is_string());
    assert!(result["remediation"].is_null());

    let human_evidence = sandbox.path().join("human-evidence");
    let human = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            human_evidence.to_str().expect("UTF-8 evidence path"),
            "--node",
            "origin.exact-distribution",
        ])
        .output()
        .expect("run human Mechanical Quality Contract");
    assert!(human.status.success());
    let human_stdout = String::from_utf8(human.stdout).expect("UTF-8 human output");
    assert!(human_stdout.contains("PASS origin.exact-distribution"));
    assert!(human_stdout.contains("proves:"));
    assert!(human_stdout.contains("does-not-prove:"));

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(human_evidence.join("manifest.json")).expect("read evidence manifest"),
    )
    .expect("parse evidence manifest");
    assert_eq!(manifest["schemaVersion"], 1);
    assert_eq!(manifest["scope"], "clean-core-local");
    assert_eq!(manifest["complete"], false);
    assert_eq!(manifest["aggregateConformance"], false);
    assert_eq!(manifest["status"], "pass-selected");
    assert!(
        manifest["catalogNodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty())
    );
    assert!(
        manifest["inputDigest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:"))
    );
    assert!(manifest["requiredToolVersions"].is_object());
    assert!(manifest["observedToolVersions"].is_object());
    assert!(
        manifest["artifacts"]
            .as_array()
            .is_some_and(|items| items.len() == 3)
    );
    assert_eq!(node(&json_events, "rust.compile")["outcome"], "not-run");
    assert_eq!(
        node(&json_events, "rust.compile")["cause"]["code"],
        "CHECK_NOT_SELECTED"
    );
}

#[test]
fn evidence_path_policy_accepts_an_external_relative_path_and_rejects_unsafe_paths() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("evidence-reader");
    create_workspace(&workspace, "evidence-reader");

    let relative = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            "../relative-evidence",
            "--node",
            "origin.exact-distribution",
        ])
        .output()
        .expect("run relative evidence fixture");
    assert!(relative.status.success());
    assert!(
        sandbox
            .path()
            .join("relative-evidence/manifest.json")
            .is_file()
    );

    let inside = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            ".yydra/evidence",
            "--node",
            "origin.exact-distribution",
        ])
        .output()
        .expect("run inside evidence fixture");
    assert!(!inside.status.success());
    assert!(String::from_utf8_lossy(&inside.stderr).contains("outside the Product Workspace"));

    let existing = sandbox.path().join("existing-evidence");
    fs::create_dir(&existing).expect("create existing evidence fixture");
    let existing_output = check(&workspace, &existing, &["origin.exact-distribution"]);
    assert!(!existing_output.status.success());
    assert!(String::from_utf8_lossy(&existing_output.stderr).contains("already exists"));
}

#[cfg(unix)]
#[test]
fn evidence_and_workspace_symlinks_cannot_escape_their_authority_roots() {
    use std::os::unix::fs::symlink;

    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("symlink-reader");
    create_workspace(&workspace, "symlink-reader");

    let external_evidence = sandbox.path().join("external-evidence-parent");
    fs::create_dir(&external_evidence).expect("create evidence target");
    let evidence_link = sandbox.path().join("evidence-link");
    symlink(&external_evidence, &evidence_link).expect("create evidence symlink");
    let linked_output = check(
        &workspace,
        &evidence_link.join("evidence"),
        &["origin.exact-distribution"],
    );
    assert!(!linked_output.status.success());
    assert!(String::from_utf8_lossy(&linked_output.stderr).contains("symlink ancestor"));

    let outside = sandbox.path().join("outside.txt");
    fs::write(&outside, "outside\n").expect("write outside fixture");
    symlink(&outside, workspace.join("outside-link")).expect("create escaping symlink");
    let escaped_output = check(
        &workspace,
        &sandbox.path().join("escaped-evidence"),
        &["origin.exact-distribution"],
    );
    assert!(!escaped_output.status.success());
    let escaped_events = events(&escaped_output);
    let failure = node(&escaped_events, "ownership.authored-inputs-unchanged");
    assert_eq!(failure["cause"]["code"], "CHECK_SYMLINK_PATH_ESCAPE");
    assert!(failure["remediation"].is_string());
    assert!(
        sandbox
            .path()
            .join("escaped-evidence/manifest.json")
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(outside).expect("read outside fixture"),
        "outside\n"
    );
}

#[test]
fn exact_rust_and_frontend_tool_authorities_fail_with_stable_diagnostics() {
    let sandbox = tempdir().expect("create sandbox");
    let rust_workspace = sandbox.path().join("rust-tool-reader");
    create_workspace(&rust_workspace, "rust-tool-reader");
    fs::write(
        rust_workspace.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"nightly\"\n",
    )
    .expect("change Rust authority");
    let rust = check(
        &rust_workspace,
        &sandbox.path().join("rust-evidence"),
        &["rust.architecture"],
    );
    assert!(!rust.status.success());
    let rust_events = events(&rust);
    let rust_failure = node(&rust_events, "rust.architecture");
    assert_eq!(
        rust_failure["cause"]["code"],
        "RUST_TOOLCHAIN_AUTHORITY_DRIFT"
    );
    assert!(rust_failure["remediation"].is_string());

    let frontend_workspace = sandbox.path().join("frontend-tool-reader");
    create_workspace(&frontend_workspace, "frontend-tool-reader");
    let package_path = frontend_workspace.join("frontend/package.json");
    let package = fs::read_to_string(&package_path)
        .expect("read package")
        .replace("\"prettier\": \"3.9.6\"", "\"prettier\": \"3.9.5\"");
    fs::write(package_path, package).expect("change frontend tool authority");
    let frontend = check(
        &frontend_workspace,
        &sandbox.path().join("frontend-evidence"),
        &["frontend.lock"],
    );
    assert!(!frontend.status.success());
    let frontend_events = events(&frontend);
    let frontend_failure = node(&frontend_events, "frontend.lock");
    assert_eq!(
        frontend_failure["cause"]["code"],
        "FRONTEND_TOOLCHAIN_DRIFT"
    );
    assert!(frontend_failure["remediation"].is_string());
}

#[test]
fn distribution_owned_typecheck_cannot_be_bypassed_by_a_product_script() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("typecheck-reader");
    create_workspace(&workspace, "typecheck-reader");
    let package_path = workspace.join("frontend/package.json");
    let package = fs::read_to_string(&package_path)
        .expect("read package")
        .replace("\"typecheck\": \"tsc --noEmit\"", "\"typecheck\": \"true\"");
    fs::write(package_path, package).expect("weaken product script");
    let config_path = workspace.join("frontend/playwright.config.mts");
    let mut config = fs::read_to_string(&config_path).expect("read authored mts config");
    config.push_str("\nconst yydraTypeFailure: number = \"not a number\";\n");
    fs::write(config_path, config).expect("add strict mts failure");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["frontend.typecheck"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(node(&parsed, "frontend.lock")["outcome"], "pass");
    let failure = node(&parsed, "frontend.typecheck");
    assert_eq!(failure["cause"]["code"], "FRONTEND_TYPECHECK_FAILED");
    assert!(failure["remediation"].is_string());
}

#[test]
fn distribution_owned_frontend_test_executes_tsx_tests() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("tsx-test-reader");
    create_workspace(&workspace, "tsx-test-reader");
    fs::write(
        workspace.join("frontend/src/presentation-failure.test.tsx"),
        r#"import { expect, test } from "vitest";

test("tsx presentation fixture is executed", () => {
  expect("executed").toBe("silently skipped");
});
"#,
    )
    .expect("write failing TSX test");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["frontend.test"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(node(&parsed, "frontend.lock")["outcome"], "pass");
    let failure = node(&parsed, "frontend.test");
    assert_eq!(failure["cause"]["code"], "FRONTEND_TEST_FAILED");
    assert!(failure["remediation"].is_string());
}

#[test]
fn stale_cargo_lock_fails_before_metadata_can_rewrite_the_scratch_copy() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("lock-reader");
    create_workspace(&workspace, "lock-reader");
    let manifest = workspace.join("crates/domain/Cargo.toml");
    let mut contents = fs::read_to_string(&manifest).expect("read manifest");
    contents.push_str("\n[dev-dependencies]\nanyhow = \"1.0.104\"\n");
    fs::write(&manifest, contents).expect("make lock stale");
    let before = fs::read(workspace.join("Cargo.lock")).expect("read lock before check");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["rust.architecture"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "rust.architecture");
    assert_eq!(failure["cause"]["code"], "CARGO_LOCK_DRIFT");
    assert!(failure["remediation"].is_string());
    assert_eq!(
        fs::read(workspace.join("Cargo.lock")).expect("read lock after check"),
        before
    );
}

#[test]
fn baseline_skill_inventory_rejects_an_unexpected_snapshot() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("skill-reader");
    create_workspace(&workspace, "skill-reader");
    let rogue = workspace.join(".agents/skills/rogue/SKILL.md");
    fs::create_dir_all(rogue.parent().expect("rogue parent")).expect("create rogue directory");
    fs::write(&rogue, "---\nname: rogue\n---\n").expect("write rogue Skill");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["ownership.baseline-skills"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "ownership.baseline-skills");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(failure["cause"]["code"], "BASELINE_SKILL_INVENTORY_DRIFT");
    assert!(
        failure["remediation"]
            .as_str()
            .is_some_and(|value| value.contains("restore"))
    );
}

#[test]
fn generated_snapshot_drift_is_reported_by_its_own_node() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("snapshot-reader");
    create_workspace(&workspace, "snapshot-reader");
    fs::write(workspace.join("LICENSE-MIT"), "changed snapshot\n")
        .expect("change exact snapshot fixture");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["ownership.generated-snapshots"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "origin.exact-distribution")["outcome"],
        "pass"
    );
    let failure = node(&parsed, "ownership.generated-snapshots");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(failure["cause"]["code"], "GENERATED_SNAPSHOT_DRIFT");
    assert!(
        failure["cause"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("LICENSE-MIT"))
    );
}

#[test]
fn api_generated_contract_node_detects_client_drift_and_remains_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("api-drift-reader");
    create_workspace(&workspace, "api-drift-reader");
    let client = workspace.join("frontend/src/generated/public-api/fetch/client.ts");
    let mut source = fs::read(&client).expect("read Generated Client fixture");
    source.extend_from_slice(b"\n// forbidden hand edit\n");
    fs::write(&client, source).expect("drift Generated Client fixture");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["api.generated-contract"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    for prerequisite in ["rust.architecture", "rust.compile", "frontend.lock"] {
        let prerequisite_result = node(&parsed, prerequisite);
        assert_eq!(
            prerequisite_result["outcome"],
            "pass",
            "prerequisite={prerequisite}, result={prerequisite_result:#?}, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let failure = node(&parsed, "api.generated-contract");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(failure["cause"]["code"], "API_CLIENT_DRIFT");
    assert!(failure["remediation"].is_string());
    assert_eq!(workspace_files(&workspace), before);
    assert_eq!(
        node(&parsed, "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );
}

#[test]
fn api_runtime_conformance_reports_a_stable_failure_for_a_malformed_live_response() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("api-runtime-reader");
    create_workspace(&workspace, "api-runtime-reader");
    let transport = workspace.join("crates/transport-http/src/lib.rs");
    let source = fs::read_to_string(&transport).expect("read transport fixture");
    let malformed = source.replace("2000-01-01T00:00:00Z", "2000-01-01T01:00:00+01:00");
    assert_ne!(source, malformed, "runtime fixture timestamp was not found");
    fs::write(&transport, malformed).expect("write malformed runtime fixture");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["api.runtime-conformance"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(node(&parsed, "api.generated-contract")["outcome"], "pass");
    let failure = node(&parsed, "api.runtime-conformance");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(failure["cause"]["code"], "API_RUNTIME_CONFORMANCE_FAILED");
    assert!(failure["remediation"].is_string());
    assert_eq!(workspace_files(&workspace), before);
    assert_eq!(
        node(&parsed, "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );
}

#[test]
fn api_client_contract_rejects_a_multiline_direct_generated_import_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("api-import-reader");
    create_workspace(&workspace, "api-import-reader");
    let runtime = workspace.join("frontend/src/framework/runtime.tsx");
    let mut source = fs::read_to_string(&runtime).expect("read Runtime fixture");
    source.push_str(
        "\nimport type {\n  FrameworkContractProfile as ForbiddenGeneratedProfile,\n} from \"../generated/public-api/fetch/schemas\";\nexport type BoundaryFixture = ForbiddenGeneratedProfile;\n",
    );
    fs::write(&runtime, source).expect("write direct Generated Client import fixture");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["api.client-contract"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    for prerequisite in [
        "rust.architecture",
        "rust.compile",
        "frontend.lock",
        "api.generated-contract",
        "frontend.typecheck",
    ] {
        assert_eq!(
            node(&parsed, prerequisite)["outcome"],
            "pass",
            "prerequisite={prerequisite}, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let failure = node(&parsed, "api.client-contract");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(
        failure["cause"]["code"],
        "API_CLIENT_IMPORT_BOUNDARY_VIOLATION"
    );
    assert!(failure["remediation"].is_string());
    assert_eq!(workspace_files(&workspace), before);
    assert_eq!(
        node(&parsed, "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );
}

#[test]
fn architecture_rejects_forbidden_domain_dependencies_in_every_edge_class() {
    let cases = [
        ("normal", "\n[dependencies]\naxum.workspace = true\n"),
        (
            "development",
            "\n[dev-dependencies]\naxum.workspace = true\n",
        ),
        ("build", "\n[build-dependencies]\naxum.workspace = true\n"),
        (
            "target-specific",
            "\n[target.'cfg(unix)'.dependencies]\naxum.workspace = true\n",
        ),
    ];

    for (edge_class, addition) in cases {
        let sandbox = tempdir().expect("create sandbox");
        let product_id = format!("arch-{}", edge_class.replace('-', ""));
        let workspace = sandbox.path().join(&product_id);
        create_workspace(&workspace, &product_id);
        let manifest = workspace.join("crates/domain/Cargo.toml");
        let mut contents = fs::read_to_string(&manifest).expect("read domain manifest");
        contents.push_str(addition);
        fs::write(&manifest, contents).expect("add forbidden dependency fixture");
        refresh_fixture_lock(&workspace);

        let output = check(
            &workspace,
            &sandbox.path().join("evidence"),
            &["rust.architecture"],
        );
        assert!(
            !output.status.success(),
            "{edge_class} dependency unexpectedly passed"
        );
        let parsed = events(&output);
        let failure = node(&parsed, "rust.architecture");
        assert_eq!(failure["outcome"], "fail", "case={edge_class}");
        assert_eq!(failure["cause"]["code"], "ARCH_FORBIDDEN_DEPENDENCY");
        assert!(
            failure["cause"]["message"]
                .as_str()
                .is_some_and(|value| value.contains("domain") && value.contains("axum")),
            "case={edge_class}, failure={failure:#?}"
        );
    }
}

#[test]
fn architecture_reports_a_stable_cycle_diagnostic() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("cycle-reader");
    create_workspace(&workspace, "cycle-reader");
    let manifest = workspace.join("crates/domain/Cargo.toml");
    let mut contents = fs::read_to_string(&manifest).expect("read domain manifest");
    contents.push_str(
        "\n[dev-dependencies]\ncycle-reader-application = { path = \"../application\" }\n",
    );
    fs::write(&manifest, contents).expect("add cyclic dependency fixture");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["rust.architecture"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "rust.architecture");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(failure["cause"]["code"], "ARCH_DEPENDENCY_CYCLE");
    assert!(failure["remediation"].is_string());
}

#[test]
fn architecture_does_not_confuse_a_product_owned_yydra_prefix_with_framework_internals() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("yydra-reader");
    create_workspace(&workspace, "yydra-reader");
    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["rust.architecture"],
    );
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn failed_prerequisites_skip_dependents_while_independent_nodes_continue() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("frontier-reader");
    create_workspace(&workspace, "frontier-reader");
    let origin = workspace.join(".yydra/origin.toml");
    let contents = fs::read_to_string(&origin)
        .expect("read Origin Record")
        .replace(
            "distribution_version = \"0.1.0\"",
            "distribution_version = \"9.9.9\"",
        );
    fs::write(origin, contents).expect("write mismatched Origin Record");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["ownership.baseline-skills", "rust.format"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "origin.exact-distribution")["outcome"],
        "fail"
    );
    let skipped = node(&parsed, "ownership.baseline-skills");
    assert_eq!(skipped["outcome"], "skipped");
    assert_eq!(
        skipped["cause"]["dependencyNodeId"],
        "origin.exact-distribution"
    );
    assert_eq!(node(&parsed, "rust.format")["outcome"], "pass");
}

#[test]
fn selected_check_leaves_workspace_inputs_byte_for_byte_unchanged() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("readonly-reader");
    create_workspace(&workspace, "readonly-reader");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &[
            "origin.exact-distribution",
            "ownership.baseline-skills",
            "ownership.generated-snapshots",
            "rust.architecture",
            "rust.format",
        ],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(workspace_files(&workspace), before);
    assert_eq!(
        node(&events(&output), "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );
}

#[cfg(unix)]
#[test]
fn check_detects_a_leaf_tool_that_mutates_an_authored_input() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("mutation-reader");
    create_workspace(&workspace, "mutation-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    let fake_cargo = fake_bin.join("cargo");
    fs::write(
        &fake_cargo,
        "#!/bin/sh\nprintf 'mutated by fixture\\n' >> \"$PWD/README.md\"\nexit 0\n",
    )
    .expect("write fake cargo");
    fs::set_permissions(&fake_cargo, fs::Permissions::from_mode(0o755))
        .expect("make fake cargo executable");
    let evidence = sandbox.path().join("evidence");
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence path"),
            "--node",
            "rust.format",
        ])
        .env("PATH", &fake_bin)
        .output()
        .expect("run mutation fixture");
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(node(&parsed, "rust.format")["outcome"], "pass");
    let unchanged = node(&parsed, "ownership.authored-inputs-unchanged");
    assert_eq!(unchanged["outcome"], "fail");
    assert_eq!(unchanged["cause"]["code"], "CHECK_MUTATED_WORKSPACE_INPUTS");
    assert!(
        unchanged["cause"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("README.md"))
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[cfg(unix)]
#[test]
fn frontend_lock_mutation_has_its_own_stable_diagnostic() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("frontend-lock-reader");
    create_workspace(&workspace, "frontend-lock-reader");
    let before = fs::read(workspace.join("frontend/package-lock.json")).expect("read lock");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("npm"),
        "#!/bin/sh\nprintf '\\n' >> \"$PWD/package-lock.json\"\nexit 0\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            sandbox
                .path()
                .join("evidence")
                .to_str()
                .expect("UTF-8 evidence"),
            "--node",
            "frontend.lock",
        ])
        .env("PATH", &fake_bin)
        .output()
        .expect("run frontend lock mutation fixture");
    assert!(!output.status.success());
    let failure = node(&events(&output), "frontend.lock").clone();
    assert_eq!(failure["cause"]["code"], "FRONTEND_LOCK_MUTATED");
    assert!(failure["remediation"].is_string());
    assert_eq!(
        fs::read(workspace.join("frontend/package-lock.json")).expect("read original lock"),
        before
    );
}

#[test]
fn unavailable_required_tool_is_infrastructure_error_not_semantic_failure() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("infra-reader");
    create_workspace(&workspace, "infra-reader");
    let empty_path = sandbox.path().join("empty-path");
    fs::create_dir(&empty_path).expect("create empty PATH");
    let evidence = sandbox.path().join("evidence");
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence path"),
            "--node",
            "infrastructure.docker",
        ])
        .env("PATH", empty_path)
        .output()
        .expect("run unavailable infrastructure fixture");
    assert!(!output.status.success());
    let failure = node(&events(&output), "infrastructure.docker").clone();
    assert_eq!(failure["outcome"], "infrastructure-error");
    assert_eq!(failure["cause"]["code"], "CHECK_TOOL_UNAVAILABLE");
}

#[test]
fn rust_and_frontend_zero_test_contracts_have_discriminating_diagnostics() {
    let sandbox = tempdir().expect("create sandbox");
    let rust_workspace = sandbox.path().join("rust-zero-reader");
    create_workspace(&rust_workspace, "rust-zero-reader");
    for relative in [
        "crates/application/src/lib.rs",
        "crates/domain/src/lib.rs",
        "crates/persistence-postgres/src/lib.rs",
    ] {
        let rust_tests = rust_workspace.join(relative);
        let source = fs::read_to_string(&rust_tests).expect("read Rust test fixture");
        let without_tests = source
            .split_once("#[cfg(test)]")
            .expect("template has Rust tests")
            .0;
        fs::write(&rust_tests, without_tests).expect("remove inline Rust tests");
    }
    fs::remove_file(rust_workspace.join("crates/application/tests/reading_queue_postgres.rs"))
        .expect("remove PostgreSQL application test");
    fs::remove_file(rust_workspace.join("crates/transport-http/tests/public_api_contract.rs"))
        .expect("remove Public API Rust tests");
    let rust = check(
        &rust_workspace,
        &sandbox.path().join("rust-zero-evidence"),
        &["rust.test"],
    );
    assert!(!rust.status.success());
    let rust_events = events(&rust);
    assert_eq!(node(&rust_events, "rust.compile")["outcome"], "pass");
    let rust_failure = node(&rust_events, "rust.test");
    assert_eq!(rust_failure["cause"]["code"], "RUST_TESTS_EMPTY");
    assert!(rust_failure["remediation"].is_string());

    let frontend_workspace = sandbox.path().join("frontend-zero-reader");
    create_workspace(&frontend_workspace, "frontend-zero-reader");
    fs::remove_file(frontend_workspace.join("frontend/src/framework/runtime.test.ts"))
        .expect("remove TypeScript tests");
    fs::remove_file(frontend_workspace.join("frontend/src/framework/path-containment.test.mjs"))
        .expect("remove JavaScript tests");
    fs::remove_file(frontend_workspace.join("frontend/src/framework/api/client.test.ts"))
        .expect("remove Public API TypeScript tests");
    let frontend = check(
        &frontend_workspace,
        &sandbox.path().join("frontend-zero-evidence"),
        &["frontend.test"],
    );
    assert!(!frontend.status.success());
    let frontend_events = events(&frontend);
    assert_eq!(node(&frontend_events, "frontend.lock")["outcome"], "pass");
    let frontend_failure = node(&frontend_events, "frontend.test");
    assert_eq!(frontend_failure["cause"]["code"], "FRONTEND_TESTS_EMPTY");
    assert!(frontend_failure["remediation"].is_string());
}

#[test]
#[ignore = "requires Docker, PostgreSQL image, Node/npm, and a Playwright Chromium installation"]
fn clean_workspace_passes_the_complete_core_real_runtime_contract() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("complete-reader");
    create_workspace(&workspace, "complete-reader");
    let setup = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["setup", workspace.to_str().expect("UTF-8 workspace")])
        .output()
        .expect("install exact locked dependencies");
    assert!(
        setup.status.success(),
        "setup stderr: {}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let before = workspace_files(&workspace);

    let output = check(&workspace, &sandbox.path().join("evidence"), &[]);
    assert!(
        output.status.success(),
        "check stderr: {}\ncheck stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    let summary = parsed
        .iter()
        .find(|event| event["event"] == "check-summary")
        .expect("check summary event");
    assert_eq!(summary["status"], "pass-core");
    assert_eq!(summary["scope"], "clean-core-local");
    assert_eq!(summary["complete"], true);
    assert_eq!(summary["aggregateConformance"], false);
    assert!(
        parsed
            .iter()
            .filter(|event| event["event"] == "check-node")
            .all(|event| event["outcome"] == "pass")
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[test]
#[ignore = "requires Docker, PostgreSQL image, Node/npm, and a Playwright Chromium installation"]
fn h5_semantic_failure_is_discriminating_read_only_and_cleans_up() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("broken-h5-reader");
    create_workspace(&workspace, "broken-h5-reader");
    let route = workspace.join("frontend/app/index.tsx");
    let source = fs::read_to_string(&route).expect("read H5 route").replace(
        "<Text>Backend {health.data.status}.</Text>",
        "<Text>Service {health.data.status}.</Text>",
    );
    fs::write(&route, source).expect("break only the H5 semantic assertion");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["h5.real-runtime"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    for prerequisite in [
        "rust.architecture",
        "rust.compile",
        "frontend.lock",
        "frontend.typecheck",
        "infrastructure.docker",
        "infrastructure.playwright-chromium",
    ] {
        assert_eq!(node(&parsed, prerequisite)["outcome"], "pass");
    }
    let failure = node(&parsed, "h5.real-runtime");
    assert_eq!(failure["cause"]["code"], "H5_E2E_FAILED");
    assert!(failure["remediation"].is_string());
    assert_eq!(workspace_files(&workspace), before);
    assert_eq!(
        node(&parsed, "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );

    let project = failure["commands"]
        .as_array()
        .expect("H5 commands")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find_map(|command| {
            command
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .find_map(|pair| (pair[0] == "--project-name").then(|| pair[1].to_owned()))
        })
        .expect("Docker Compose project name");
    let cleanup = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=com.docker.compose.project={project}"),
            "--format",
            "{{.ID}}",
        ])
        .output()
        .expect("inspect Docker cleanup");
    assert!(cleanup.status.success());
    assert!(
        cleanup.stdout.is_empty(),
        "Compose project {project} survived"
    );
}

fn workspace_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(current).expect("read fixture tree") {
            let path = entry.expect("read fixture entry").path();
            let relative = path.strip_prefix(root).expect("relative fixture path");
            if relative.components().any(|component| {
                matches!(
                    component.as_os_str().to_str(),
                    Some("target" | "node_modules" | ".expo" | "dist" | "test-results")
                )
            }) {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, files);
            } else if path.is_file() {
                files.insert(
                    relative.to_path_buf(),
                    fs::read(path).expect("read fixture file"),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
