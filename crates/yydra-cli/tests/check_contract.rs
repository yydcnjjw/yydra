// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use sha2::Digest;
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

#[cfg(unix)]
fn write_fake_native_toolchain(directory: &Path, npm_body: &str) {
    fs::create_dir(directory).expect("create fake native tool directory");
    write_executable(&directory.join("npm"), npm_body);
    write_executable(
        &directory.join("node"),
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' 'v26.8.1'
  exit 0
fi
case "$2" in
  *node_modules/expo/package.json*) printf '%s\n' '57.0.19' ;;
  *node_modules/@react-native-community/netinfo/package.json*) printf '%s\n' '12.0.1' ;;
  *node_modules/@playwright/test/package.json*) printf '%s\n' '1.62.1' ;;
  *node_modules/@testing-library/dom/package.json*) printf '%s\n' '10.4.1' ;;
  *node_modules/@testing-library/react/package.json*) printf '%s\n' '16.3.3' ;;
  *node_modules/@types/react-dom/package.json*) printf '%s\n' '19.2.5' ;;
  *node_modules/@eslint/js/package.json*) printf '%s\n' '10.0.1' ;;
  *node_modules/eslint/package.json*) printf '%s\n' '10.9.1' ;;
  *node_modules/jsdom/package.json*) printf '%s\n' '30.0.1' ;;
  *node_modules/orval/package.json*) printf '%s\n' '8.27.0' ;;
  *node_modules/prettier/package.json*) printf '%s\n' '3.9.6' ;;
  *node_modules/typescript/package.json*) printf '%s\n' '6.0.3' ;;
  *node_modules/typescript-eslint/package.json*) printf '%s\n' '8.69.0' ;;
  *node_modules/vitest/package.json*) printf '%s\n' '4.1.11' ;;
  *) exit 2 ;;
esac
"#,
    );
}

fn check(workspace: &Path, evidence: &Path, nodes: &[&str]) -> Output {
    let _heavy_guard = nodes
        .iter()
        .any(|node| {
            node.starts_with("api.")
                || node.starts_with("database.runtime-")
                || node.starts_with("frontend.")
                || node.starts_with("h5.")
                || node.starts_with("runtime.")
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
fn migration_history_node_rejects_distribution_and_comparison_base_changes_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("migration-policy-reader");
    create_workspace(&workspace, "migration-policy-reader");

    let clean = check(
        &workspace,
        &sandbox.path().join("clean-evidence"),
        &["database.migration-history"],
    );
    assert!(
        clean.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&clean.stderr)
    );
    assert_eq!(
        node(&events(&clean), "database.migration-history")["outcome"],
        "pass"
    );

    let baseline = workspace.join("migrations/0001_baseline.sql");
    let baseline_source = fs::read(&baseline).expect("read baseline migration");
    fs::write(&baseline, b"-- edited migration\n").expect("edit baseline migration");
    let mutated = check(
        &workspace,
        &sandbox.path().join("mutated-evidence"),
        &["database.migration-history"],
    );
    assert!(!mutated.status.success());
    assert_eq!(
        node(&events(&mutated), "database.migration-history")["cause"]["code"],
        "DB_MIGRATION_DISTRIBUTION_BASE_MUTATED"
    );
    assert_eq!(
        fs::read(&baseline).expect("read unchanged mutation"),
        b"-- edited migration\n"
    );
    fs::write(&baseline, &baseline_source).expect("restore baseline fixture");
    fs::remove_file(&baseline).expect("delete baseline migration");
    let deleted = check(
        &workspace,
        &sandbox.path().join("deleted-evidence"),
        &["database.migration-history"],
    );
    assert!(!deleted.status.success());
    assert_eq!(
        node(&events(&deleted), "database.migration-history")["cause"]["code"],
        "DB_MIGRATION_DISTRIBUTION_BASE_DELETED"
    );
    assert!(
        !baseline.exists(),
        "check mode must not restore a deleted migration"
    );
    fs::write(&baseline, &baseline_source).expect("restore deleted baseline fixture");

    let git = |arguments: &[&str]| {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&workspace)
            .output()
            .expect("run git fixture command");
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Yydra Check",
        "-c",
        "user.email=check@example.test",
        "commit",
        "--quiet",
        "-m",
        "comparison base",
    ]);
    let product_migration = workspace.join("migrations/0006_product_change.sql");
    fs::write(&product_migration, b"SELECT 1;\n").expect("add product migration");
    git(&["add", "migrations/0006_product_change.sql"]);
    git(&[
        "-c",
        "user.name=Yydra Check",
        "-c",
        "user.email=check@example.test",
        "commit",
        "--quiet",
        "-m",
        "add product migration",
    ]);
    fs::write(&product_migration, b"SELECT 2;\n").expect("mutate product migration");

    let comparison = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            sandbox
                .path()
                .join("comparison-evidence")
                .to_str()
                .expect("UTF-8 evidence path"),
            "--comparison-base",
            "HEAD",
            "--node",
            "database.migration-history",
        ])
        .output()
        .expect("run comparison-base migration check");
    assert!(!comparison.status.success());
    assert_eq!(
        node(&events(&comparison), "database.migration-history")["cause"]["code"],
        "DB_MIGRATION_COMPARISON_BASE_MUTATED"
    );
    assert_eq!(
        fs::read(&product_migration).expect("read unchanged product migration"),
        b"SELECT 2;\n"
    );
    fs::remove_file(&product_migration).expect("delete comparison-base migration");
    let comparison_deleted = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            sandbox
                .path()
                .join("comparison-deleted-evidence")
                .to_str()
                .expect("UTF-8 evidence path"),
            "--comparison-base",
            "HEAD",
            "--node",
            "database.migration-history",
        ])
        .output()
        .expect("run deleted comparison-base migration check");
    assert!(!comparison_deleted.status.success());
    assert_eq!(
        node(&events(&comparison_deleted), "database.migration-history")["cause"]["code"],
        "DB_MIGRATION_COMPARISON_BASE_DELETED"
    );
    assert!(
        !product_migration.exists(),
        "check mode must not restore the deleted comparison-base migration"
    );
}

#[test]
#[ignore = "requires Docker and the pinned PostgreSQL image"]
fn database_runtime_invariant_failure_is_discriminating_and_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("database-invariant-reader");
    create_workspace(&workspace, "database-invariant-reader");
    let before = fs::read(workspace.join("crates/persistence-postgres/src/lib.rs"))
        .expect("read persistence fixture");

    let passing = check(
        &workspace,
        &sandbox.path().join("database-pass-evidence"),
        &["database.runtime-invariants"],
    );
    assert!(
        passing.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&passing.stderr)
    );
    assert_eq!(
        node(&events(&passing), "database.runtime-invariants")["outcome"],
        "pass"
    );

    let source = String::from_utf8(before.clone()).expect("UTF-8 persistence fixture");
    let broken = source.replacen("        FOR UPDATE\n", "        FOR UPDATE NOWAIT\n", 1);
    assert_ne!(broken, source, "row-lock fixture must change source");
    fs::write(
        workspace.join("crates/persistence-postgres/src/lib.rs"),
        broken,
    )
    .expect("write missing row-lock fixture");
    let failing = check(
        &workspace,
        &sandbox.path().join("database-fail-evidence"),
        &["database.runtime-invariants"],
    );
    assert!(!failing.status.success());
    assert_eq!(
        node(&events(&failing), "database.runtime-invariants")["cause"]["code"],
        "DATABASE_RUNTIME_INVARIANTS_FAILED"
    );
    assert_ne!(
        fs::read(workspace.join("crates/persistence-postgres/src/lib.rs"))
            .expect("read unchanged negative fixture"),
        before,
        "check mode must not repair the authored negative fixture"
    );
    fs::write(
        workspace.join("crates/persistence-postgres/src/lib.rs"),
        &before,
    )
    .expect("restore contention fixture");

    let application_path = workspace.join("crates/application/src/lib.rs");
    let application = fs::read_to_string(&application_path).expect("read application fixture");
    let committed_failure = application.replacen(
        "if let Err(error) = adjust_reading_progress(&mut transaction, completed_delta).await {\n            transaction\n                .rollback()",
        "if let Err(error) = adjust_reading_progress(&mut transaction, completed_delta).await {\n            transaction\n                .commit()",
        1,
    );
    assert_ne!(
        committed_failure, application,
        "rollback fixture must change the source transaction"
    );
    fs::write(&application_path, &committed_failure).expect("commit a failed orchestration");
    let transaction_failure = check(
        &workspace,
        &sandbox.path().join("database-transaction-fail-evidence"),
        &["database.runtime-invariants"],
    );
    assert!(!transaction_failure.status.success());
    assert_eq!(
        node(&events(&transaction_failure), "database.runtime-invariants")["cause"]["code"],
        "DATABASE_RUNTIME_INVARIANTS_FAILED"
    );
    assert_eq!(
        fs::read_to_string(&application_path).expect("read unchanged transaction fixture"),
        committed_failure,
        "check mode must not repair the broken rollback behavior"
    );
    fs::write(&application_path, application).expect("restore transaction fixture");

    let tests_path = workspace.join("crates/application/tests/reading_queue_postgres.rs");
    let tests = fs::read_to_string(&tests_path).expect("read database tests fixture");
    let renamed = tests.replacen(
        "applied_migration_history_rejects_mutation_and_deletion",
        "renamed_database_test_that_must_not_satisfy_the_distribution_rule",
        1,
    );
    assert_ne!(renamed, tests, "required database test name must change");
    fs::write(&tests_path, &renamed).expect("rename required database fixture");
    let missing_test = check(
        &workspace,
        &sandbox.path().join("database-missing-test-evidence"),
        &["database.runtime-invariants"],
    );
    assert!(!missing_test.status.success());
    assert_eq!(
        node(&events(&missing_test), "database.runtime-invariants")["cause"]["code"],
        "DATABASE_RUNTIME_INVARIANT_TEST_MISSING"
    );
    assert_eq!(
        fs::read_to_string(&tests_path).expect("read unchanged missing-test fixture"),
        renamed,
        "check mode must not repair or recreate the required database test"
    );
}

#[test]
fn post_commit_executor_node_rejects_behavior_and_missing_tests_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("post-commit-executor-reader");
    create_workspace(&workspace, "post-commit-executor-reader");

    let passing = check(
        &workspace,
        &sandbox.path().join("post-commit-pass-evidence"),
        &["runtime.post-commit-executor"],
    );
    assert!(
        passing.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&passing.stderr)
    );
    assert_eq!(
        node(&events(&passing), "runtime.post-commit-executor")["outcome"],
        "pass"
    );

    let source_path = workspace.join("crates/application/src/post_commit.rs");
    let source = fs::read_to_string(&source_path).expect("read post-commit executor fixture");
    let broken = source.replacen(
        "Err(_) => PostCommitTaskOutcome::TimedOut,",
        "Err(_) => PostCommitTaskOutcome::Completed,",
        1,
    );
    assert_ne!(broken, source, "timeout fixture must change source");
    fs::write(&source_path, &broken).expect("remove timeout outcome fixture");
    let failing = check(
        &workspace,
        &sandbox.path().join("post-commit-fail-evidence"),
        &["runtime.post-commit-executor"],
    );
    assert!(!failing.status.success());
    assert_eq!(
        node(&events(&failing), "runtime.post-commit-executor")["cause"]["code"],
        "POST_COMMIT_EXECUTOR_FAILED"
    );
    assert_eq!(
        fs::read_to_string(&source_path).expect("read unchanged negative fixture"),
        broken,
        "check mode must not repair the authored negative fixture"
    );
    fs::write(&source_path, source).expect("restore executor fixture");

    let tests_path = workspace.join("crates/application/tests/post_commit_executor.rs");
    let tests = fs::read_to_string(&tests_path).expect("read executor tests fixture");
    let renamed = tests.replacen(
        "bounded_lossy_executor_reports_admission_deadline_timeout_failure_and_crash_without_retry",
        "renamed_executor_test_that_must_not_satisfy_the_distribution_rule",
        1,
    );
    assert_ne!(renamed, tests, "required test name fixture must change");
    fs::write(&tests_path, &renamed).expect("rename required executor fixture");
    let missing_test = check(
        &workspace,
        &sandbox.path().join("post-commit-missing-test-evidence"),
        &["runtime.post-commit-executor"],
    );
    assert!(!missing_test.status.success());
    assert_eq!(
        node(&events(&missing_test), "runtime.post-commit-executor")["cause"]["code"],
        "POST_COMMIT_EXECUTOR_TEST_MISSING"
    );
    assert_eq!(
        fs::read_to_string(&tests_path).expect("read unchanged missing-test fixture"),
        renamed,
        "check mode must not repair or recreate the required test"
    );
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
    fs::remove_file(rust_workspace.join("crates/application/tests/post_commit_executor.rs"))
        .expect("remove post-commit application test");
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
    for relative in [
        "frontend/src/framework/runtime.test.ts",
        "frontend/src/framework/runtime-render.test.tsx",
        "frontend/src/framework/path-containment.test.mjs",
        "frontend/src/framework/api/client.test.ts",
        "frontend/src/product-presentation/reading-queue/queries.test.ts",
        "frontend/src/product-presentation/reading-queue/screen.test.tsx",
    ] {
        fs::remove_file(frontend_workspace.join(relative)).expect("remove canonical frontend test");
    }
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
fn accessibility_node_rejects_a_missing_product_semantics_spec_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("missing-accessibility-spec-reader");
    create_workspace(&workspace, "missing-accessibility-spec-reader");
    let spec = workspace.join("frontend/e2e/product-presentation.accessibility.spec.ts");
    fs::remove_file(&spec).expect("remove Product-owned semantic spec");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["h5.product-presentation-accessibility"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "h5.product-presentation-accessibility");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(failure["cause"]["code"], "ACCESSIBILITY_SPEC_MISSING");
    assert!(failure["remediation"].is_string());
    assert_eq!(workspace_files(&workspace), before);
    assert_eq!(
        node(&parsed, "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );
}

#[test]
#[ignore = "requires Docker, PostgreSQL image, Node/npm, and a Playwright Chromium installation"]
fn accessibility_node_passes_and_rejects_a_heading_only_regression_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("accessibility-heading-reader");
    create_workspace(&workspace, "accessibility-heading-reader");
    let setup = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["setup", workspace.to_str().expect("UTF-8 workspace")])
        .output()
        .expect("install exact locked dependencies");
    assert!(
        setup.status.success(),
        "setup stderr: {}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let pristine = workspace_files(&workspace);

    let passing = check(
        &workspace,
        &sandbox.path().join("accessibility-pass-evidence"),
        &["h5.product-presentation-accessibility"],
    );
    assert!(
        passing.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&passing.stderr),
        String::from_utf8_lossy(&passing.stdout)
    );
    assert_eq!(
        node(&events(&passing), "h5.product-presentation-accessibility")["outcome"],
        "pass"
    );
    assert_eq!(workspace_files(&workspace), pristine);

    let screen = workspace.join("frontend/src/product-presentation/reading-queue/screen.tsx");
    let source = fs::read_to_string(&screen).expect("read Product Presentation fixture");
    let broken = source.replacen(
        "<Text accessibilityRole=\"header\" style={styles.entryTitle}>",
        "<Text style={styles.entryTitle}>",
        1,
    );
    assert_ne!(
        broken, source,
        "dynamic Reading Queue heading fixture must change"
    );
    fs::write(&screen, &broken).expect("remove only the dynamic heading semantic");
    let broken_inputs = workspace_files(&workspace);

    let failing = check(
        &workspace,
        &sandbox.path().join("accessibility-fail-evidence"),
        &["h5.product-presentation-accessibility"],
    );
    assert!(!failing.status.success());
    let parsed = events(&failing);
    assert_eq!(
        node(&parsed, "h5.product-presentation-accessibility")["cause"]["code"],
        "ACCESSIBILITY_ASSERTION_FAILED"
    );
    assert_eq!(workspace_files(&workspace), broken_inputs);
    assert_eq!(
        node(&parsed, "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );
}

#[test]
#[ignore = "requires Docker, PostgreSQL image, Node/npm, and a Playwright Chromium installation"]
fn h5_semantic_failure_is_discriminating_read_only_and_cleans_up() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("broken-h5-reader");
    create_workspace(&workspace, "broken-h5-reader");
    let screen = workspace.join("frontend/src/product-presentation/reading-queue/screen.tsx");
    let original = fs::read_to_string(&screen).expect("read H5 Product Presentation");
    let source = original.replace(
        "<Text>Backend {health.data.status}.</Text>",
        "<Text>Service {health.data.status}.</Text>",
    );
    assert_ne!(source, original, "H5 backend-status fixture must change");
    fs::write(&screen, source).expect("break only the H5 semantic assertion");
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

#[cfg(unix)]
#[test]
fn android_generation_repeats_the_complete_inventory_and_is_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("native-reader");
    create_workspace(&workspace, "native-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' '12.0.2'
  exit 0
fi
if [ "$1" = "ci" ]; then
  exit 0
fi
if [ "$1" = "run" ]; then
  case " $* " in
    *" --ignore-scripts "*) ;;
    *) exit 90 ;;
  esac
  mkdir -p android/app/src/main
  printf '%s\n' 'generated settings' > android/settings.gradle
  printf '%s\n' 'generated application' > android/app/src/main/AndroidManifest.xml
  printf '%s\n' 'generated build' > android/app/build.gradle
  printf '%s\n' '#!/bin/sh' 'exit 0' > android/gradlew
  chmod 755 android/gradlew
  exit 0
fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
    );
    let evidence = sandbox.path().join("evidence");
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence"),
            "--node",
            "native.android-generation",
        ])
        .env("PATH", path)
        .output()
        .expect("run Android generation check");
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    let generation = node(&parsed, "native.android-generation");
    assert_eq!(generation["outcome"], "pass");
    assert!(
        generation["commands"]
            .as_array()
            .expect("generation commands")
            .iter()
            .any(|command| command.as_str().is_some_and(|command| {
                command.contains("npm run --ignore-scripts generate:android")
            }))
    );
    let first =
        fs::read(evidence.join("artifacts/native.android-generation/generation-1-inventory.json"))
            .expect("read first native inventory");
    let second =
        fs::read(evidence.join("artifacts/native.android-generation/generation-2-inventory.json"))
            .expect("read second native inventory");
    assert_eq!(first, second);
    assert!(
        String::from_utf8(first)
            .expect("UTF-8 native inventory")
            .contains("android/gradlew")
    );
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());
    assert_eq!(
        node(&parsed, "ownership.authored-inputs-unchanged")["outcome"],
        "pass"
    );
}

#[cfg(unix)]
#[test]
fn android_generation_rejects_inventory_nondeterminism_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("nondeterministic-native-reader");
    create_workspace(&workspace, "nondeterministic-native-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    let counter = sandbox.path().join("generation-counter");
    write_fake_native_toolchain(
        &fake_bin,
        &format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then
  mkdir -p android/app
  printf '%s\n' 'generated settings' > android/settings.gradle
  printf '%s\n' '#!/bin/sh' 'exit 0' > android/gradlew
  chmod 755 android/gradlew
  if [ -f '{counter}' ]; then
    printf '%s\n' 'second generation' > android/app/build.gradle
  else
    printf '%s\n' 'first generation' > android/app/build.gradle
    : > '{counter}'
  fi
  exit 0
fi
exit 2
"#,
            counter = counter.display()
        ),
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
    );
    let evidence = sandbox.path().join("evidence");
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence"),
            "--node",
            "native.android-generation",
        ])
        .env("PATH", path)
        .output()
        .expect("run nondeterministic Android generation check");
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "native.android-generation");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(
        failure["cause"]["code"],
        "NATIVE_GENERATION_NONDETERMINISTIC"
    );
    assert_ne!(
        fs::read(evidence.join("artifacts/native.android-generation/generation-1-inventory.json"))
            .expect("read first inventory"),
        fs::read(evidence.join("artifacts/native.android-generation/generation-2-inventory.json"))
            .expect("read second inventory")
    );
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());
}

#[cfg(unix)]
#[test]
fn android_generation_inventory_includes_root_mode_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("root-mode-native-reader");
    create_workspace(&workspace, "root-mode-native-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    let counter = sandbox.path().join("generation-counter");
    write_fake_native_toolchain(
        &fake_bin,
        &format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then
  mkdir -p android/app
  printf '%s\n' 'generated settings' > android/settings.gradle
  printf '%s\n' 'generated build' > android/app/build.gradle
  printf '%s\n' '#!/bin/sh' 'exit 0' > android/gradlew
  chmod 755 android/gradlew
  if [ -f '{counter}' ]; then
    chmod 700 android
  else
    chmod 755 android
    : > '{counter}'
  fi
  exit 0
fi
exit 2
"#,
            counter = counter.display()
        ),
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
    );
    let evidence = sandbox.path().join("evidence");
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence"),
            "--node",
            "native.android-generation",
        ])
        .env("PATH", path)
        .output()
        .expect("run root-mode Android generation check");
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "native.android-generation")["cause"]["code"],
        "NATIVE_GENERATION_NONDETERMINISTIC"
    );
    let first: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/native.android-generation/generation-1-inventory.json"))
            .expect("read first inventory"),
    )
    .expect("parse first inventory");
    let second: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/native.android-generation/generation-2-inventory.json"))
            .expect("read second inventory"),
    )
    .expect("parse second inventory");
    assert_eq!(first[0]["path"], "android");
    assert_eq!(first[0]["mode"], "0755");
    assert_eq!(second[0]["path"], "android");
    assert_eq!(second[0]["mode"], "0700");
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());
}

#[cfg(unix)]
#[test]
fn android_generation_rejects_authored_input_mutation_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("mutating-native-reader");
    create_workspace(&workspace, "mutating-native-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then
  mkdir -p android/app
  printf '%s\n' 'generated build' > android/app/build.gradle
  printf '%s\n' 'mutated by native generator' >> app.json
  exit 0
fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
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
            "native.android-generation",
        ])
        .env("PATH", path)
        .output()
        .expect("run mutating Android generation check");
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "native.android-generation")["cause"]["code"],
        "NATIVE_GENERATION_MUTATED_AUTHORED_INPUTS"
    );
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());
}

#[cfg(unix)]
#[test]
fn android_generation_rejects_missing_output_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("missing-native-reader");
    create_workspace(&workspace, "missing-native-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then mkdir -p android; exit 0; fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
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
            "native.android-generation",
        ])
        .env("PATH", path)
        .output()
        .expect("run missing Android generation check");
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "native.android-generation")["cause"]["code"],
        "NATIVE_GENERATION_OUTPUT_MISSING"
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[cfg(unix)]
#[test]
fn android_generation_rejects_an_undeclared_config_plugin_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("undeclared-plugin-reader");
    create_workspace(&workspace, "undeclared-plugin-reader");
    let app_config = workspace.join("frontend/app.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&app_config).expect("read Expo app configuration"))
            .expect("parse Expo app configuration");
    config["expo"]["plugins"] = serde_json::json!(["expo-router", "unreviewed-plugin"]);
    fs::write(
        &app_config,
        serde_json::to_vec_pretty(&config).expect("encode Expo app configuration"),
    )
    .expect("write undeclared config plugin fixture");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then
  mkdir -p android/app
  printf '%s\n' 'generated build' > android/app/build.gradle
  exit 0
fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
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
            "native.android-generation",
        ])
        .env("PATH", path)
        .output()
        .expect("run undeclared config plugin check");
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "native.android-generation");
    assert_eq!(
        failure["cause"]["code"],
        "NATIVE_GENERATION_INPUT_POLICY_FAILED"
    );
    assert!(
        failure["cause"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unreviewed-plugin"))
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[cfg(unix)]
#[test]
fn android_generation_rejects_local_module_symlink_escape_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("symlink-plugin-reader");
    create_workspace(&workspace, "symlink-plugin-reader");
    let modules = workspace.join("frontend/modules");
    fs::create_dir(&modules).expect("create local modules directory");
    std::os::unix::fs::symlink("../src", modules.join("escape"))
        .expect("create escaping local module symlink");
    let app_config = workspace.join("frontend/app.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&app_config).expect("read Expo app configuration"))
            .expect("parse Expo app configuration");
    config["expo"]["plugins"] = serde_json::json!(["expo-router", "./modules/escape"]);
    fs::write(
        &app_config,
        serde_json::to_vec_pretty(&config).expect("encode Expo app configuration"),
    )
    .expect("write escaping local plugin fixture");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
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
            "native.android-generation",
        ])
        .env("PATH", path)
        .output()
        .expect("run escaping local module check");
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "native.android-generation")["cause"]["code"],
        "NATIVE_GENERATION_INPUT_POLICY_FAILED"
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[cfg(unix)]
#[test]
fn android_release_builds_account_free_and_records_artifact_identity_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("android-release-reader");
    create_workspace(&workspace, "android-release-reader");
    let before = workspace_files(&workspace);
    let poisoned_home = sandbox.path().join("poisoned-home");
    let poisoned_config = sandbox.path().join("poisoned-config");
    fs::create_dir_all(poisoned_home.join(".expo")).expect("create poisoned Expo home");
    fs::create_dir_all(poisoned_config.join("expo")).expect("create poisoned Expo config");
    fs::write(poisoned_home.join(".expo/account.json"), "must-not-be-read")
        .expect("write poisoned Expo home");
    fs::write(
        poisoned_config.join("expo/account.json"),
        "must-not-be-read",
    )
    .expect("write poisoned Expo config");
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "${EXPO_TOKEN+x}" = x ] || [ "${EAS_TOKEN+x}" = x ]; then exit 9; fi
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then
  if [ -f "$HOME/.expo/account.json" ] || [ -f "$XDG_CONFIG_HOME/expo/account.json" ]; then exit 9; fi
  mkdir -p android/app
  printf '%s\n' 'generated settings' > android/settings.gradle
  printf '%s\n' 'generated build' > android/app/build.gradle
  printf '%s\n' \
    '#!/bin/sh' \
    'if [ "${EXPO_TOKEN+x}" = x ] || [ "${EAS_TOKEN+x}" = x ]; then exit 9; fi' \
    'if [ -f "$HOME/.expo/account.json" ] || [ -f "$XDG_CONFIG_HOME/expo/account.json" ]; then exit 9; fi' \
    'if [ "$1" = "--version" ]; then printf "%s\\n" "Gradle 9.0.0"; exit 0; fi' \
    'mkdir -p app/build/outputs/apk/release' \
    "printf '%s\\n' 'account-free release' > app/build/outputs/apk/release/app-release.apk" \
    > android/gradlew
  chmod 755 android/gradlew
  exit 0
fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
    );
    let evidence = sandbox.path().join("evidence");
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence"),
            "--node",
            "android.release",
        ])
        .env("PATH", path)
        .env("EXPO_TOKEN", "must-not-reach-build")
        .env("EAS_TOKEN", "must-not-reach-build")
        .env("HOME", &poisoned_home)
        .env("XDG_CONFIG_HOME", &poisoned_config)
        .output()
        .expect("run account-free Android release check");
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "native.android-generation")["outcome"],
        "pass"
    );
    let release = node(&parsed, "android.release");
    assert_eq!(release["outcome"], "pass");
    assert!(
        release["commands"]
            .as_array()
            .expect("release commands")
            .iter()
            .any(|command| command.as_str().is_some_and(|command| {
                command.contains("./gradlew --no-daemon assembleRelease")
            }))
    );
    let apk = evidence.join("artifacts/android.release/app-release.apk");
    assert_eq!(
        fs::read(&apk).expect("read release artifact"),
        b"account-free release\n"
    );
    let identity: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/android.release/artifact.json"))
            .expect("read release artifact identity"),
    )
    .expect("parse release artifact identity");
    assert_eq!(identity["path"], "app-release.apk");
    assert_eq!(identity["bytes"], 21);
    assert_eq!(
        identity["sha256"],
        format!(
            "sha256:{}",
            hex::encode(sha2::Sha256::digest(b"account-free release\n"))
        )
    );
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());
}

#[cfg(unix)]
#[test]
fn android_release_rejects_gradle_failure_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("failing-android-release-reader");
    create_workspace(&workspace, "failing-android-release-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then
  mkdir -p android/app
  printf '%s\n' 'generated settings' > android/settings.gradle
  printf '%s\n' 'generated build' > android/app/build.gradle
  printf '%s\n' \
    '#!/bin/sh' \
    'if [ "$1" = "--version" ]; then printf "%s\\n" "Gradle 9.0.0"; exit 0; fi' \
    'exit 42' \
    > android/gradlew
  chmod 755 android/gradlew
  exit 0
fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
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
            "android.release",
        ])
        .env("PATH", path)
        .output()
        .expect("run failing Android release check");
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "native.android-generation")["outcome"],
        "pass"
    );
    let failure = node(&parsed, "android.release");
    assert_eq!(failure["cause"]["code"], "ANDROID_RELEASE_BUILD_FAILED");
    assert!(failure["remediation"].is_string());
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());
}

#[cfg(unix)]
#[test]
fn android_release_rejects_missing_apk_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("missing-android-release-reader");
    create_workspace(&workspace, "missing-android-release-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_fake_native_toolchain(
        &fake_bin,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' '12.0.2'; exit 0; fi
if [ "$1" = "ci" ]; then exit 0; fi
if [ "$1" = "run" ]; then
  mkdir -p android/app
  printf '%s\n' 'generated settings' > android/settings.gradle
  printf '%s\n' 'generated build' > android/app/build.gradle
  printf '%s\n' \
    '#!/bin/sh' \
    'if [ "$1" = "--version" ]; then printf "%s\\n" "Gradle 9.0.0"; exit 0; fi' \
    'exit 0' \
    > android/gradlew
  chmod 755 android/gradlew
  exit 0
fi
exit 2
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
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
            "android.release",
        ])
        .env("PATH", path)
        .output()
        .expect("run missing APK check");
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "android.release")["cause"]["code"],
        "ANDROID_RELEASE_OUTPUT_MISSING"
    );
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());
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
