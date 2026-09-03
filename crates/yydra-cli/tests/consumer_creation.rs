// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::tempdir;

#[test]
fn creates_workspace_from_the_normalized_flag_model() {
    let sandbox = tempdir().expect("create test sandbox");
    let destination = sandbox.path().join("acme-reader");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            destination.to_str().expect("UTF-8 destination"),
            "--product-name",
            "  Acme Reader  ",
            "--product-id",
            "acme-reader",
            "--product-source-license",
            "Apache-2.0",
        ])
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .output()
        .expect("run packaged-consumer CLI seam");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(destination.join("README.md").is_file());

    let origin = fs::read_to_string(destination.join(".yydra/origin.toml"))
        .expect("read Workspace Origin Record");
    assert!(origin.contains("distribution_version = \"0.1.0\""));
    assert!(origin.contains("template_identity = \"yydra-v0-product-workspace\""));
    assert!(origin.contains("product_name = \"Acme Reader\""));
    assert!(origin.contains("product_id = \"acme-reader\""));
}

#[test]
fn materializes_the_public_api_authority_chain() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("api-reader");
    create_with_flags(&workspace, "API Reader", "api-reader");

    for relative in [
        "contracts/openapi.json",
        ".yydra/api-generation.json",
        ".yydra/api-generation-history.json",
        ".yydra/api-generation.lock",
        "crates/application/src/post_commit.rs",
        "crates/application/tests/post_commit_executor.rs",
        "crates/application/tests/reading_queue_postgres.rs",
        "crates/transport-http/src/bin/export-openapi.rs",
        "crates/transport-http/tests/public_api_contract.rs",
        "migrations/0002_reading_queue.sql",
        "migrations/0003_reading_entry_transitions.sql",
        "migrations/0004_reading_queue_pagination.sql",
        "migrations/0005_reading_progress.sql",
        "frontend/orval.config.mjs",
        "frontend/src/generated/public-api/fetch/client.ts",
        "frontend/src/generated/public-api/fetch/schemas/index.ts",
        "frontend/src/framework/api/client.ts",
        "frontend/src/framework/api/client.test.ts",
    ] {
        assert!(
            workspace.join(relative).is_file(),
            "missing Public API authority-chain artifact {relative}"
        );
    }

    let transport = fs::read_to_string(workspace.join("crates/transport-http/src/lib.rs"))
        .expect("read transport source");
    assert!(transport.contains("OpenApiRouter"));
    assert!(transport.contains("pub fn public_routes"));
    assert!(transport.contains("operation_id = \"getFrameworkContractProfile\""));
    assert!(transport.contains("operation_id = \"createReadingQueueEntry\""));
    assert!(transport.contains("operation_id = \"listReadingQueueEntries\""));
    assert!(transport.contains("operation_id = \"changeReadingQueueEntryState\""));
    assert!(transport.contains("operation_id = \"getFrameworkProtectedContract\""));
    assert!(transport.contains("RouteAccess::Anonymous"));
    assert!(transport.contains("RouteAccess::Protected"));

    let domain = fs::read_to_string(workspace.join("crates/domain/src/lib.rs"))
        .expect("read Product Domain source");
    assert!(domain.contains("pub struct ReadingEntryTitle"));
    assert!(domain.contains("pub enum ReadingEntryState"));
    assert!(domain.contains("pub fn complete"));
    assert!(domain.contains("pub fn reopen"));

    let application = fs::read_to_string(workspace.join("crates/application/src/lib.rs"))
        .expect("read application source");
    assert!(application.contains("pub struct CreateReadingEntry"));
    assert!(application.contains("pub struct ListReadingEntries"));
    assert!(application.contains("pub struct ChangeReadingEntryStateAndRecordProgress"));
    assert!(application.contains("pub type ChangeReadingEntryState"));
    assert!(application.contains("pub struct GetReadingProgress"));
    assert!(application.contains("SET TRANSACTION READ ONLY"));

    let post_commit = fs::read_to_string(workspace.join("crates/application/src/post_commit.rs"))
        .expect("read post-commit executor source");
    assert!(post_commit.contains("pub struct LossyPostCommitTask"));
    assert!(post_commit.contains("pub struct PostCommitExecutor"));
    assert!(post_commit.contains("retry = false"));
    assert!(post_commit.contains("This is not a durable queue"));

    let persistence = fs::read_to_string(workspace.join("crates/persistence-postgres/src/lib.rs"))
        .expect("read PostgreSQL persistence source");
    assert!(persistence.contains("pub async fn insert_reading_entry"));
    assert!(persistence.contains("pub async fn list_reading_entries"));
    assert!(persistence.contains("FOR UPDATE"));
    assert!(persistence.contains("pub async fn update_reading_entry_state"));
    assert!(persistence.contains("pub async fn adjust_reading_progress"));

    let generated =
        fs::read_to_string(workspace.join("frontend/src/generated/public-api/fetch/client.ts"))
            .expect("read generated Fetch client");
    assert!(generated.contains("getFrameworkContractProfile"));
    assert!(generated.contains("createReadingQueueEntry"));
    assert!(generated.contains("listReadingQueueEntries"));
    assert!(generated.contains("changeReadingQueueEntryState"));
    assert!(generated.contains("getFrameworkProtectedContract"));
    assert!(generated.contains("fetchFn"));
    let create_schema =
        fs::read_to_string(workspace.join(
            "frontend/src/generated/public-api/request/schemas/frameworkContractCreate.zod.ts",
        ))
        .expect("read generated create schema");
    assert!(create_schema.contains("ExactAmountRegExp"));
    assert!(create_schema.contains(".regex("));

    let facade = fs::read_to_string(workspace.join("frontend/src/framework/api/client.ts"))
        .expect("read handwritten Framework facade");
    for outcome in ["problem", "transport", "cancelled", "contractViolation"] {
        assert!(facade.contains(outcome), "missing stable {outcome} outcome");
    }

    let contract_fixture =
        fs::read_to_string(workspace.join("crates/transport-http/tests/public_api_contract.rs"))
            .expect("read runtime contract fixture");
    assert!(contract_fixture.contains("an undocumented Product Domain state must fail"));
    assert!(contract_fixture.contains("invalid_requests_transitions_and_auth"));
    assert!(contract_fixture.contains(r#""state":"invented""#));
}

#[test]
fn records_one_normalized_license_choice_for_future_product_owned_source_only() {
    let sandbox = tempdir().expect("create test sandbox");
    let destination = sandbox.path().join("licensed-reader");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            destination.to_str().expect("UTF-8 destination"),
            "--product-name",
            "Licensed Reader",
            "--product-id",
            "licensed-reader",
            "--product-source-license",
            "  MPL-2.0   OR   Apache-2.0  ",
        ])
        .output()
        .expect("create licensed Product Workspace");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let origin = fs::read_to_string(destination.join(".yydra/origin.toml"))
        .expect("read Workspace Origin Record");
    assert!(origin.contains("product_source_license = \"MPL-2.0 OR Apache-2.0\""));

    let policy = fs::read_to_string(destination.join(".yydra/product-source-license.toml"))
        .expect("read product source license policy");
    assert!(policy.contains("license_expression = \"MPL-2.0 OR Apache-2.0\""));
    assert!(policy.contains("applies_to = [\"product-owned-source\"]"));
    assert!(policy.contains("copied_yydra_bytes = \"MIT OR Apache-2.0\""));
    assert!(policy.contains("third_party_bytes = \"retain-original-terms-and-notices\""));
    let app: serde_json::Value = serde_json::from_slice(
        &fs::read(destination.join("frontend/app.json")).expect("read Expo app config"),
    )
    .expect("parse Expo app config");
    assert_eq!(
        app["expo"]["android"]["package"],
        "dev.yydra.licensed_reader"
    );
}

#[test]
fn reading_queue_named_product_keeps_product_and_section_heading_proofs_distinct() {
    let sandbox = tempdir().expect("create test sandbox");
    let destination = sandbox.path().join("reading-queue");
    create_with_flags(&destination, "Reading Queue", "reading-queue");

    let semantics = fs::read_to_string(
        destination.join("frontend/e2e/product-presentation.accessibility.spec.ts"),
    )
    .expect("read rendered accessibility semantics");
    assert!(semantics.contains("const productName: string = \"Reading Queue\";"));
    assert!(semantics.contains("productHeadings.first()"));
    assert!(semantics.contains("productName === \"Reading Queue\" ? 2 : 1"));
    assert!(semantics.contains("queueHeadings.nth(1)"));
}

#[test]
fn emits_a_sorted_inventory_with_all_five_lifecycles_and_yydra_provenance() {
    let sandbox = tempdir().expect("create test sandbox");
    let destination = sandbox.path().join("inventoried-reader");
    create_with_flags(&destination, "Inventoried Reader", "inventoried-reader");

    let inventory: serde_json::Value = serde_json::from_slice(
        &fs::read(destination.join(".yydra/distribution-inventory.json"))
            .expect("read Distribution inventory"),
    )
    .expect("parse Distribution inventory");
    assert_eq!(inventory["schema_version"], 1);
    assert_eq!(inventory["distribution_version"], "0.1.0");
    assert_eq!(
        inventory["lifecycles"],
        serde_json::json!([
            "product-owned-source",
            "exact-distribution-snapshot",
            "committed-generated-output",
            "ephemeral-generated-output",
            "evidence-build-output"
        ])
    );
    assert_eq!(
        inventory["path_rule_match_policy"],
        "highest-priority-match"
    );

    let artifacts = inventory["artifacts"]
        .as_array()
        .expect("artifact inventory array");
    let paths = artifacts
        .iter()
        .map(|artifact| artifact["path"].as_str().expect("artifact path"))
        .collect::<Vec<_>>();
    let mut sorted_paths = paths.clone();
    sorted_paths.sort_unstable();
    assert_eq!(paths, sorted_paths, "artifact inventory must be sorted");
    assert!(paths.contains(&"README.md"));
    assert!(paths.contains(&"LICENSE-MIT"));
    assert!(paths.contains(&"LICENSE-APACHE"));
    assert!(paths.contains(&".yydra/origin.toml"));
    for artifact in artifacts {
        assert_eq!(artifact["mode"], "0644");
        assert_eq!(artifact["spdx"], "MIT OR Apache-2.0");
        let digest = artifact["source_sha256"]
            .as_str()
            .expect("artifact source digest");
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    let rules = inventory["path_rules"]
        .as_array()
        .expect("lifecycle path rules");
    let patterns = rules
        .iter()
        .flat_map(|rule| {
            rule["path_patterns"]
                .as_array()
                .expect("path pattern array")
        })
        .map(|pattern| pattern.as_str().expect("path pattern"))
        .collect::<Vec<_>>();
    for canonical in [
        "crates/**",
        "frontend/src/**",
        "contracts/openapi.json",
        "frontend/src/generated/public-api/**",
    ] {
        assert!(
            patterns.contains(&canonical),
            "missing canonical {canonical}"
        );
    }
    assert!(!patterns.contains(&"product/**"));
    assert!(!patterns.contains(&"backend/openapi.json"));
    for lifecycle in [
        "product-owned-source",
        "exact-distribution-snapshot",
        "committed-generated-output",
        "ephemeral-generated-output",
        "evidence-build-output",
    ] {
        assert!(
            rules.iter().any(|rule| rule["lifecycle"] == lifecycle),
            "missing path rule for {lifecycle}"
        );
        assert!(
            rules
                .iter()
                .filter(|rule| rule["lifecycle"] == lifecycle)
                .all(|rule| rule["license_notice_authority"].is_string()),
            "missing notice authority for {lifecycle}"
        );
    }
    let product_rule = rules
        .iter()
        .find(|rule| rule["lifecycle"] == "product-owned-source")
        .expect("product-owned source rule");
    assert_eq!(product_rule["hand_editable"], true);
    assert_eq!(
        product_rule["new_bytes_license_authority"],
        "workspace-origin-record.product_source_license"
    );
    for protected in [
        "exact-distribution-snapshot",
        "committed-generated-output",
        "ephemeral-generated-output",
        "evidence-build-output",
    ] {
        assert!(
            rules
                .iter()
                .filter(|rule| rule["lifecycle"] == protected)
                .all(|rule| rule["hand_editable"] == false
                    && rule["workspace_source_authority"] == false)
        );
    }
    for generated in [
        "contracts/openapi.json",
        ".yydra/api-generation.json",
        ".yydra/api-generation-history.json",
        ".yydra/api-generation.lock",
        "frontend/src/generated/public-api/fetch/client.ts",
    ] {
        let artifact = artifacts
            .iter()
            .find(|artifact| artifact["path"] == generated)
            .unwrap_or_else(|| panic!("missing generated artifact {generated}"));
        assert_eq!(artifact["lifecycle"], "committed-generated-output");
        assert_eq!(artifact["hand_editable_after_creation"], false);
    }

    let ignore = fs::read_to_string(destination.join(".gitignore"))
        .expect("read generated-output ignore authority");
    for generated_host in ["/frontend/android/", "/frontend/ios/"] {
        assert!(
            ignore.lines().any(|line| line == generated_host),
            "generated native host {generated_host} must be disposable ignored output"
        );
    }
}

#[cfg(unix)]
#[test]
fn setup_uses_both_committed_locks_and_emits_versioned_json_lines() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("setup-reader");
    create_with_flags(&workspace, "Setup Reader", "setup-reader");
    let cargo_lock = workspace.join("Cargo.lock");
    let npm_lock = workspace.join("frontend/package-lock.json");
    let locks_before = [
        fs::read(&cargo_lock).expect("read committed Cargo lock"),
        fs::read(&npm_lock).expect("read committed npm lock"),
    ];

    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    let fake_tool = r#"#!/bin/sh
printf '%s:%s\n' "$PWD" "$*" >> "$YYDRA_TOOL_LOG"
"#;
    for tool in ["cargo", "npm"] {
        write_executable(&fake_bin.join(tool), fake_tool);
    }
    let path = std::env::join_paths(std::iter::once(fake_bin.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").expect("PATH")),
    ))
    .expect("join fake PATH");
    let tool_log = sandbox.path().join("tool.log");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "setup",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_TOOL_LOG", &tool_log)
        .output()
        .expect("run setup");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        locks_before[0],
        fs::read(&cargo_lock).expect("reread Cargo lock")
    );
    assert_eq!(
        locks_before[1],
        fs::read(&npm_lock).expect("reread npm lock")
    );
    let calls = fs::read_to_string(tool_log).expect("read tool calls");
    assert!(
        calls.contains(&format!("{}:fetch --locked", workspace.display())),
        "calls: {calls}"
    );
    assert!(
        calls.contains(&format!("{}:ci", workspace.join("frontend").display())),
        "calls: {calls}"
    );

    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    for event in &events {
        assert_eq!(event["schemaVersion"], 1);
        assert!(event["phase"].is_string());
        assert!(event["code"].is_string());
        assert!(event["severity"].is_string());
        assert!(event["status"].is_string());
        assert!(event.get("location").is_some());
        assert!(event.get("remediation").is_some());
    }
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "SETUP_CARGO_FETCH" && event["status"] == "pass" })
    );
    assert!(
        events
            .iter()
            .any(|event| event["code"] == "SETUP_NPM_CI" && event["status"] == "pass")
    );
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "SETUP_LOCK_INTEGRITY" && event["status"] == "pass" })
    );
}

#[cfg(unix)]
#[test]
fn json_mode_forwards_complete_large_leaf_output_before_exit() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("output-reader");
    create_with_flags(&workspace, "Output Reader", "output-reader");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
printf 'BEGIN-LEAF-OUTPUT\n'
/usr/bin/head -c 131072 /dev/zero | /usr/bin/tr '\000' x
printf '\nEND-LEAF-OUTPUT\n'
"#,
    );
    write_executable(&fake_bin.join("npm"), "#!/bin/sh\nexit 0\n");
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "setup",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .output()
        .expect("run setup with large leaf output");
    assert!(output.status.success());
    let diagnostics = String::from_utf8(output.stdout).expect("UTF-8 diagnostics");
    for line in diagnostics.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("stdout remains JSON Lines only");
    }
    let forwarded = String::from_utf8(output.stderr).expect("UTF-8 forwarded detail");
    assert!(forwarded.starts_with("BEGIN-LEAF-OUTPUT\n"));
    assert!(forwarded.ends_with("\nEND-LEAF-OUTPUT\n"));
    assert_eq!(
        forwarded.bytes().filter(|byte| *byte == b'x').count(),
        131072
    );
}

#[test]
fn doctor_human_and_json_views_share_the_stable_phase_code() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("diagnostic-reader");
    create_with_flags(&workspace, "Diagnostic Reader", "diagnostic-reader");

    let human = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["doctor", workspace.to_str().expect("UTF-8 workspace")])
        .output()
        .expect("run human doctor");
    let json = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "doctor",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .output()
        .expect("run JSON doctor");

    assert!(human.status.success());
    assert!(json.status.success());
    let human_stdout = String::from_utf8_lossy(&human.stdout);
    assert!(human_stdout.contains("[doctor.verify] pass"));
    assert!(human_stdout.contains("code=DOCTOR_WORKSPACE_VERIFY"));
    assert!(human_stdout.contains(&format!("location={}", workspace.display())));
    let events = String::from_utf8(json.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| {
            event["code"] == "DOCTOR_WORKSPACE_VERIFY" && event["status"] == "pass"
        })
    );
}

#[cfg(unix)]
#[test]
fn setup_fails_loudly_and_restores_both_committed_locks_after_tool_drift() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("drift-reader");
    create_with_flags(&workspace, "Drift Reader", "drift-reader");
    let cargo_lock = workspace.join("Cargo.lock");
    let npm_lock = workspace.join("frontend/package-lock.json");
    let cargo_before = fs::read(&cargo_lock).expect("read committed Cargo lock");
    let npm_before = fs::read(&npm_lock).expect("read committed npm lock");

    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        "#!/bin/sh\nprintf 'rewritten by cargo\\n' > Cargo.lock\n",
    );
    write_executable(
        &fake_bin.join("npm"),
        "#!/bin/sh\nprintf 'rewritten by npm\\n' > package-lock.json\n",
    );
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "setup",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .output()
        .expect("run drifting setup");

    assert!(!output.status.success(), "setup accepted rewritten locks");
    assert_eq!(
        cargo_before,
        fs::read(&cargo_lock).expect("restored Cargo lock")
    );
    assert_eq!(npm_before, fs::read(&npm_lock).expect("restored npm lock"));
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    let failure = events
        .iter()
        .find(|event| event["code"] == "SETUP_LOCK_INTEGRITY" && event["status"] == "fail")
        .expect("lock integrity failure event");
    assert_eq!(failure["severity"], "error");
    assert!(failure["location"].is_string());
    assert!(failure["remediation"].is_string());
}

#[cfg(unix)]
#[test]
fn setup_restores_a_lock_even_when_the_mutating_tool_itself_fails() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("failed-tool-reader");
    create_with_flags(&workspace, "Failed Tool Reader", "failed-tool-reader");
    let cargo_lock = workspace.join("Cargo.lock");
    let cargo_before = fs::read(&cargo_lock).expect("read committed Cargo lock");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        "#!/bin/sh\nprintf 'rewritten before failure\\n' > Cargo.lock\nexit 12\n",
    );
    write_executable(&fake_bin.join("npm"), "#!/bin/sh\nexit 99\n");
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "setup",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .output()
        .expect("run failed setup tool");

    assert!(!output.status.success());
    assert_eq!(
        cargo_before,
        fs::read(cargo_lock).expect("restored Cargo lock after tool failure")
    );
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "SETUP_CARGO_FETCH" && event["status"] == "fail" })
    );
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "SETUP_LOCK_INTEGRITY" && event["status"] == "fail" })
    );
}

#[cfg(unix)]
#[test]
fn setup_shutdown_terminates_tool_group_and_restores_the_committed_lock() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("setup-shutdown-reader");
    create_with_flags(&workspace, "Setup Shutdown Reader", "setup-shutdown-reader");
    let cargo_lock = workspace.join("Cargo.lock");
    let cargo_before = fs::read(&cargo_lock).expect("read committed Cargo lock");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
printf 'rewritten while setup runs\n' > Cargo.lock
printf '%s\n' "$$" > "$YYDRA_SETUP_PID"
sleep 30 &
printf '%s\n' "$!" > "$YYDRA_SETUP_WORKER_PID"
wait
"#,
    );
    write_executable(&fake_bin.join("npm"), "#!/bin/sh\nexit 99\n");
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");
    let setup_pid = sandbox.path().join("setup.pid");
    let setup_worker_pid = sandbox.path().join("setup-worker.pid");

    let child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "setup",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_SETUP_PID", &setup_pid)
        .env("YYDRA_SETUP_WORKER_PID", &setup_worker_pid)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start setup orchestration");
    for _ in 0..100 {
        if setup_pid.is_file() && setup_worker_pid.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(setup_pid.is_file() && setup_worker_pid.is_file());
    assert_ne!(
        cargo_before,
        fs::read(&cargo_lock).expect("read drifted Cargo lock")
    );
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .expect("signal setup")
            .success()
    );
    let output = child.wait_with_output().expect("wait for setup shutdown");
    assert!(!output.status.success());
    assert_eq!(
        cargo_before,
        fs::read(&cargo_lock).expect("read restored Cargo lock")
    );
    for pid_path in [&setup_pid, &setup_worker_pid] {
        let pid = fs::read_to_string(pid_path)
            .expect("read setup process pid")
            .trim()
            .to_owned();
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .output()
                .expect("probe setup process")
                .status
                .success(),
            "setup process {pid} survived shutdown"
        );
    }
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "SETUP_CARGO_FETCH" && event["status"] == "fail" })
    );
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "SETUP_LOCK_INTEGRITY" && event["status"] == "fail" })
    );
}

#[test]
fn migration_add_creates_the_next_product_owned_sql_file_without_applying_it() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("migration-reader");
    create_with_flags(&workspace, "Migration Reader", "migration-reader");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "db",
            "migration",
            "add",
            "add_reading_notes",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .output()
        .expect("add migration stub");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(workspace.join("migrations/0001_baseline.sql").is_file());
    let added = workspace.join("migrations/0006_add_reading_notes.sql");
    let contents = fs::read_to_string(&added).expect("read migration stub");
    assert!(contents.starts_with("-- SPDX-License-Identifier: Apache-2.0\n"));
    assert!(contents.contains("-- Add migration SQL here."));
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(events.iter().any(|event| {
        event["code"] == "DB_MIGRATION_STUB_CREATE"
            && event["status"] == "pass"
            && event["location"] == added.display().to_string()
    }));
}

#[test]
fn migration_add_rejects_incompatible_existing_version_histories_without_writing() {
    let sandbox = tempdir().expect("create test sandbox");
    for (case, filename, expected) in [
        ("invalid", "abc_bad.sql", "invalid version"),
        ("duplicate", "1_duplicate.sql", "duplicate version 1"),
        (
            "overflow",
            "9223372036854775807_last.sql",
            "migration version overflow",
        ),
    ] {
        let workspace = sandbox.path().join(case);
        create_with_flags(&workspace, "Migration Reader", "migration-reader");
        fs::write(
            workspace.join("migrations").join(filename),
            "-- SPDX-License-Identifier: MIT OR Apache-2.0\n",
        )
        .expect("write incompatible migration fixture");

        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args([
                "--message-format=json",
                "db",
                "migration",
                "add",
                "must_not_exist",
                workspace.to_str().expect("UTF-8 workspace"),
            ])
            .output()
            .expect("reject incompatible history");
        assert!(!output.status.success(), "accepted {case} history");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !workspace
                .join("migrations/0002_must_not_exist.sql")
                .exists()
        );
        let events = String::from_utf8(output.stdout)
            .expect("UTF-8 diagnostics")
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
            .collect::<Vec<_>>();
        assert!(
            events
                .iter()
                .any(|event| { event["code"] == "DB_MIGRATION_PLAN" && event["status"] == "fail" })
        );
    }
}

#[cfg(unix)]
#[test]
fn db_migrate_is_an_explicit_locked_leaf_command() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("migrate-reader");
    create_with_flags(&workspace, "Migrate Reader", "migrate-reader");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
printf '%s:%s\n' "$PWD" "$*" > "$YYDRA_TOOL_LOG"
"#,
    );
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");
    let tool_log = sandbox.path().join("tool.log");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "db",
            "migrate",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_TOOL_LOG", &tool_log)
        .output()
        .expect("run explicit migration command");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(tool_log).expect("read migration command"),
        format!("{}:run --locked --bin migrate\n", workspace.display())
    );
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "DB_MIGRATE_APPLY" && event["status"] == "pass" })
    );
}

#[cfg(unix)]
#[test]
fn db_migrate_shutdown_terminates_the_migration_process_group() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("migrate-shutdown-reader");
    create_with_flags(
        &workspace,
        "Migrate Shutdown Reader",
        "migrate-shutdown-reader",
    );
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
printf '%s\n' "$$" > "$YYDRA_DB_MIGRATE_PID"
sleep 30 &
printf '%s\n' "$!" > "$YYDRA_DB_MIGRATE_WORKER_PID"
wait
"#,
    );
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");
    let migrate_pid = sandbox.path().join("db-migrate.pid");
    let migrate_worker_pid = sandbox.path().join("db-migrate-worker.pid");

    let child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "db",
            "migrate",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_DB_MIGRATE_PID", &migrate_pid)
        .env("YYDRA_DB_MIGRATE_WORKER_PID", &migrate_worker_pid)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start explicit migration");
    for _ in 0..100 {
        if migrate_pid.is_file() && migrate_worker_pid.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(migrate_pid.is_file() && migrate_worker_pid.is_file());
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .expect("signal explicit migration")
            .success()
    );
    let output = child
        .wait_with_output()
        .expect("wait for migration shutdown");
    assert!(!output.status.success());
    for pid_path in [&migrate_pid, &migrate_worker_pid] {
        let pid = fs::read_to_string(pid_path)
            .expect("read migration process pid")
            .trim()
            .to_owned();
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .output()
                .expect("probe migration process")
                .status
                .success(),
            "migration process {pid} survived shutdown"
        );
    }
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "DB_MIGRATE_APPLY" && event["status"] == "fail" })
    );
}

#[cfg(unix)]
#[test]
fn dev_sequences_migration_then_terminates_the_peer_when_a_child_fails() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("dev-reader");
    create_with_flags(&workspace, "Dev Reader", "dev-reader");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
case "$*" in
  *"--bin migrate"*)
    printf 'migration\n' >> "$YYDRA_DEV_LOG"
    exit 0
    ;;
  *"--bin server"*)
    printf '%s\n' "$$" > "$YYDRA_BACKEND_PID"
    printf 'backend\n' >> "$YYDRA_DEV_LOG"
    sleep 30 &
    printf '%s\n' "$!" > "$YYDRA_BACKEND_WORKER_PID"
    wait
    ;;
esac
exit 91
"#,
    );
    write_executable(
        &fake_bin.join("npm"),
        r#"#!/bin/sh
printf 'frontend\n' >> "$YYDRA_DEV_LOG"
printf '%s\n' "$$" > "$YYDRA_FRONTEND_PID"
sleep 30 &
printf '%s\n' "$!" > "$YYDRA_FRONTEND_WORKER_PID"
exit 17
"#,
    );
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");
    let dev_log = sandbox.path().join("dev.log");
    let backend_pid = sandbox.path().join("backend.pid");
    let backend_worker_pid = sandbox.path().join("backend-worker.pid");
    let frontend_pid = sandbox.path().join("frontend.pid");
    let frontend_worker_pid = sandbox.path().join("frontend-worker.pid");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "dev",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_DEV_LOG", &dev_log)
        .env("YYDRA_BACKEND_PID", &backend_pid)
        .env("YYDRA_BACKEND_WORKER_PID", &backend_worker_pid)
        .env("YYDRA_FRONTEND_PID", &frontend_pid)
        .env("YYDRA_FRONTEND_WORKER_PID", &frontend_worker_pid)
        .output()
        .expect("run dev orchestration");

    assert!(!output.status.success(), "dev accepted a failed frontend");
    let calls = fs::read_to_string(dev_log).expect("read dev sequence");
    assert!(calls.starts_with("migration\n"), "calls: {calls}");
    assert!(calls.contains("backend\n"), "calls: {calls}");
    assert!(calls.contains("frontend\n"), "calls: {calls}");
    for pid_path in [
        &backend_pid,
        &backend_worker_pid,
        &frontend_pid,
        &frontend_worker_pid,
    ] {
        let pid = fs::read_to_string(pid_path)
            .expect("read development process pid")
            .trim()
            .to_owned();
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .output()
                .expect("probe development process")
                .status
                .success(),
            "development process {pid} survived frontend failure"
        );
    }

    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    let position = |code: &str, status: &str| {
        events
            .iter()
            .position(|event| event["code"] == code && event["status"] == status)
            .unwrap_or_else(|| panic!("missing {code}/{status}"))
    };
    assert!(position("DEV_MIGRATION", "pass") < position("DEV_BACKEND", "started"));
    assert!(position("DEV_BACKEND", "started") < position("DEV_FRONTEND", "started"));
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "DEV_FRONTEND" && event["status"] == "fail" })
    );
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "DEV_PEER_SHUTDOWN" && event["status"] == "pass" })
    );
}

#[cfg(unix)]
#[test]
fn dev_shutdown_during_migration_terminates_its_group_before_other_phases_start() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("migration-shutdown-reader");
    create_with_flags(
        &workspace,
        "Migration Shutdown Reader",
        "migration-shutdown-reader",
    );
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
case "$*" in
  *"--bin migrate"*)
    printf '%s\n' "$$" > "$YYDRA_MIGRATION_PID"
    sleep 30 &
    printf '%s\n' "$!" > "$YYDRA_MIGRATION_WORKER_PID"
    wait
    ;;
esac
printf 'unexpected backend\n' >> "$YYDRA_UNEXPECTED_PHASES"
exit 99
"#,
    );
    write_executable(
        &fake_bin.join("npm"),
        "#!/bin/sh\nprintf 'unexpected frontend\\n' >> \"$YYDRA_UNEXPECTED_PHASES\"\nexit 99\n",
    );
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");
    let migration_pid = sandbox.path().join("migration.pid");
    let migration_worker_pid = sandbox.path().join("migration-worker.pid");
    let unexpected_phases = sandbox.path().join("unexpected-phases.log");

    let child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "dev",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_MIGRATION_PID", &migration_pid)
        .env("YYDRA_MIGRATION_WORKER_PID", &migration_worker_pid)
        .env("YYDRA_UNEXPECTED_PHASES", &unexpected_phases)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start dev during migration");
    for _ in 0..100 {
        if migration_pid.is_file() && migration_worker_pid.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(migration_pid.is_file() && migration_worker_pid.is_file());

    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .expect("signal dev during migration")
            .success()
    );
    let output = child
        .wait_with_output()
        .expect("wait for migration shutdown");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for pid_path in [&migration_pid, &migration_worker_pid] {
        let pid = fs::read_to_string(pid_path)
            .expect("read migration process pid")
            .trim()
            .to_owned();
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .output()
                .expect("probe migration process")
                .status
                .success(),
            "migration process {pid} survived shutdown"
        );
    }
    assert!(
        !unexpected_phases.exists(),
        "backend or frontend started after migration shutdown"
    );
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "DEV_PEER_SHUTDOWN" && event["status"] == "pass" })
    );
    assert!(!events.iter().any(|event| event["code"] == "DEV_BACKEND"));
    assert!(!events.iter().any(|event| event["code"] == "DEV_FRONTEND"));
}

#[cfg(unix)]
#[test]
fn dev_backend_spawn_failure_uses_the_stable_diagnostic_contract() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("spawn-failure-reader");
    create_with_flags(&workspace, "Spawn Failure Reader", "spawn-failure-reader");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        "#!/bin/sh\n/bin/rm -- \"$0\"\nexit 0\n",
    );
    write_executable(&fake_bin.join("npm"), "#!/bin/sh\nexit 99\n");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "dev",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", fake_bin)
        .output()
        .expect("run dev with missing backend executable");
    assert!(!output.status.success());
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    let failure = events
        .iter()
        .find(|event| event["code"] == "DEV_BACKEND" && event["status"] == "fail")
        .expect("stable backend spawn failure");
    assert_eq!(failure["location"], workspace.display().to_string());
    assert!(failure["remediation"].is_string());
}

#[cfg(unix)]
#[test]
fn production_h5_runner_signal_terminates_export_process_group() {
    if !Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("runner-shutdown-reader");
    create_with_flags(
        &workspace,
        "Runner Shutdown Reader",
        "runner-shutdown-reader",
    );
    let fake_npm_cli = sandbox.path().join("fake-npm-cli.mjs");
    fs::write(
        &fake_npm_cli,
        r#"// SPDX-License-Identifier: MIT OR Apache-2.0
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';
writeFileSync(process.env.YYDRA_RUNNER_CHILD_PID, String(process.pid));
const worker = spawn('sleep', ['30'], { stdio: 'ignore' });
writeFileSync(process.env.YYDRA_RUNNER_WORKER_PID, String(worker.pid));
await new Promise((resolve) => worker.once('exit', resolve));
"#,
    )
    .expect("write fake npm CLI");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake npm directory");
    write_executable(
        &fake_bin.join("npm"),
        "#!/bin/sh\nexec node \"$YYDRA_FAKE_NPM_CLI\"\n",
    );
    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let fake_path = std::env::join_paths(
        std::iter::once(fake_bin.clone()).chain(std::env::split_paths(&inherited_path)),
    )
    .expect("compose fake npm PATH");
    let child_pid = sandbox.path().join("runner-child.pid");
    let worker_pid = sandbox.path().join("runner-worker.pid");
    let child = Command::new("node")
        .arg("scripts/run-h5-e2e.mjs")
        .current_dir(workspace.join("frontend"))
        .env("PATH", fake_path)
        .env("YYDRA_FAKE_NPM_CLI", fake_npm_cli)
        .env("YYDRA_RUNNER_CHILD_PID", &child_pid)
        .env("YYDRA_RUNNER_WORKER_PID", &worker_pid)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start production H5 runner");
    for _ in 0..100 {
        if child_pid.is_file() && worker_pid.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(child_pid.is_file() && worker_pid.is_file());
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .expect("signal production H5 runner")
            .success()
    );
    let output = child.wait_with_output().expect("wait for H5 runner");
    assert!(!output.status.success(), "signal should be observable");
    for pid_path in [&child_pid, &worker_pid] {
        let pid = fs::read_to_string(pid_path)
            .expect("read runner process pid")
            .trim()
            .to_owned();
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .output()
                .expect("probe runner process")
                .status
                .success(),
            "runner process {pid} survived shutdown"
        );
    }
}

#[cfg(unix)]
#[test]
fn dev_broken_diagnostic_pipe_still_terminates_managed_child_groups() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("broken-pipe-reader");
    create_with_flags(&workspace, "Broken Pipe Reader", "broken-pipe-reader");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
case "$*" in
  *"--bin migrate"*) exit 0 ;;
esac
printf '%s\n' "$$" > "$YYDRA_BACKEND_PID"
sleep 30 &
printf '%s\n' "$!" > "$YYDRA_BACKEND_WORKER_PID"
wait
"#,
    );
    write_executable(
        &fake_bin.join("npm"),
        r#"#!/bin/sh
printf '%s\n' "$$" > "$YYDRA_FRONTEND_PID"
while [ ! -f "$YYDRA_FRONTEND_EXIT" ]; do :; done
printf 'frontend detail after consumer closes pipes\n'
exit 17
"#,
    );
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");
    let pid_paths = [
        sandbox.path().join("broken-backend.pid"),
        sandbox.path().join("broken-backend-worker.pid"),
        sandbox.path().join("broken-frontend.pid"),
    ];
    let exit_marker = sandbox.path().join("frontend-exit");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "dev",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_BACKEND_PID", &pid_paths[0])
        .env("YYDRA_BACKEND_WORKER_PID", &pid_paths[1])
        .env("YYDRA_FRONTEND_PID", &pid_paths[2])
        .env("YYDRA_FRONTEND_EXIT", &exit_marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start dev with diagnostic pipe");
    let stdout = child.stdout.take().expect("capture diagnostic stdout");
    let stderr = child.stderr.take().expect("capture forwarded detail");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        assert_ne!(
            reader.read_line(&mut line).expect("read diagnostic line"),
            0,
            "dev exited before frontend started"
        );
        if line.contains(r#""code":"DEV_FRONTEND""#) && line.contains(r#""status":"started""#) {
            break;
        }
    }
    for _ in 0..100 {
        if pid_paths.iter().all(|path| path.is_file()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(pid_paths.iter().all(|path| path.is_file()));
    drop(reader);
    drop(stderr);
    let shutdown_started = std::time::Instant::now();
    fs::write(&exit_marker, b"exit\n").expect("trigger frontend failure");
    assert!(!child.wait().expect("wait for broken-pipe dev").success());
    assert!(
        shutdown_started.elapsed() < std::time::Duration::from_secs(5),
        "managed children were not terminated promptly"
    );
    for pid_path in pid_paths {
        let pid = fs::read_to_string(pid_path)
            .expect("read managed process pid")
            .trim()
            .to_owned();
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .output()
                .expect("probe managed process")
                .status
                .success(),
            "managed process {pid} survived diagnostic panic"
        );
    }
}

#[cfg(unix)]
#[test]
fn dev_shutdown_signal_terminates_both_child_process_groups() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("shutdown-reader");
    create_with_flags(&workspace, "Shutdown Reader", "shutdown-reader");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    let long_lived_tool = r#"#!/bin/sh
case "$*" in
  *"--bin migrate"*) exit 0 ;;
esac
prefix="$YYDRA_PROCESS_PREFIX"
case "$0" in
  *cargo) prefix="backend" ;;
  *npm) prefix="frontend" ;;
esac
printf '%s\n' "$$" > "$YYDRA_PID_DIR/$prefix.pid"
sleep 30 &
printf '%s\n' "$!" > "$YYDRA_PID_DIR/$prefix-worker.pid"
wait
"#;
    for tool in ["cargo", "npm"] {
        write_executable(&fake_bin.join(tool), long_lived_tool);
    }
    let path = std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    )))
    .expect("join fake PATH");

    let child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "dev",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", path)
        .env("YYDRA_PID_DIR", sandbox.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start dev orchestration");
    let pid_paths = [
        sandbox.path().join("backend.pid"),
        sandbox.path().join("backend-worker.pid"),
        sandbox.path().join("frontend.pid"),
        sandbox.path().join("frontend-worker.pid"),
    ];
    for _ in 0..100 {
        if pid_paths.iter().all(|path| path.is_file()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        pid_paths.iter().all(|path| path.is_file()),
        "development children did not start"
    );

    let signal = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("signal dev process");
    assert!(signal.success());
    let output = child.wait_with_output().expect("wait for dev shutdown");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for pid_path in pid_paths {
        let pid = fs::read_to_string(pid_path)
            .expect("read development process pid")
            .trim()
            .to_owned();
        let probe = Command::new("kill")
            .args(["-0", &pid])
            .output()
            .expect("probe development process");
        assert!(
            !probe.status.success(),
            "development process {pid} survived shutdown"
        );
    }
    let events = String::from_utf8(output.stdout)
        .expect("UTF-8 diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON Line event"))
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| { event["code"] == "DEV_PEER_SHUTDOWN" && event["status"] == "pass" })
    );
}

#[test]
fn doctor_rejects_hand_edits_to_snapshot_and_generated_authorities_without_mutation() {
    let sandbox = tempdir().expect("create test sandbox");
    for (name, relative, diagnostic) in [
        (
            "snapshot-drift",
            "LICENSE-MIT",
            "exact Distribution snapshot drift",
        ),
        (
            "generated-drift",
            ".yydra/distribution-inventory.json",
            "committed generated inventory drift",
        ),
        (
            "provenance-drift",
            ".yydra/product-source-license.toml",
            "committed generated provenance drift",
        ),
    ] {
        let workspace = sandbox.path().join(name);
        create_with_flags(&workspace, "Authority Reader", "authority-reader");
        let authority = workspace.join(relative);
        let mut contents = fs::read(&authority).expect("read protected authority");
        contents.extend_from_slice(b"\nhand edited\n");
        fs::write(&authority, contents).expect("mutate protected authority fixture");
        let before = byte_inventory(&workspace);

        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args(["doctor", workspace.to_str().expect("UTF-8 workspace")])
            .output()
            .expect("run doctor");

        assert!(!output.status.success(), "doctor accepted {name}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(before, byte_inventory(&workspace));
    }
}

#[test]
fn doctor_rejects_coordinated_origin_and_policy_edits_without_mutation() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("edited-origin-reader");
    create_with_flags(&workspace, "Authority Reader", "authority-reader");

    let origin_path = workspace.join(".yydra/origin.toml");
    let origin = fs::read_to_string(&origin_path).expect("read origin");
    fs::write(
        &origin_path,
        origin
            .replace(
                "product_name = \"Authority Reader\"",
                "product_name = \"Edited Reader\"",
            )
            .replace(
                "product_source_license = \"Apache-2.0\"",
                "product_source_license = \"MIT\"",
            ),
    )
    .expect("edit generated origin fields");
    let policy_path = workspace.join(".yydra/product-source-license.toml");
    let policy = fs::read_to_string(&policy_path).expect("read policy");
    fs::write(
        &policy_path,
        policy.replace(
            "license_expression = \"Apache-2.0\"",
            "license_expression = \"MIT\"",
        ),
    )
    .expect("edit generated policy consistently");
    let before = byte_inventory(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["doctor", workspace.to_str().expect("UTF-8 workspace")])
        .output()
        .expect("run doctor");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Workspace Origin Record creation fingerprint mismatch")
    );
    assert_eq!(before, byte_inventory(&workspace));
}

#[test]
fn resulting_workspace_makes_no_sync_upgrade_or_version_override_promise() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("one-shot-reader");
    create_with_flags(&workspace, "One-shot Reader", "one-shot-reader");

    let readme = fs::read_to_string(workspace.join("README.md")).expect("read Workspace README");
    for excluded_contract in [
        "no template rerun",
        "no template sync",
        "no upgrade",
        "no compatibility-range selection",
        "no Distribution-version override",
    ] {
        assert!(
            readme.contains(excluded_contract),
            "README must state {excluded_contract:?}"
        );
    }
}

#[test]
fn interactive_answers_and_flags_share_the_normalized_input_model() {
    let sandbox = tempdir().expect("create test sandbox");
    let from_flags = sandbox.path().join("from-flags");
    let from_answers = sandbox.path().join("from-answers");

    let flags = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            from_flags.to_str().expect("UTF-8 destination"),
            "--product-name",
            " Acme Reader ",
            "--product-id",
            " acme-reader ",
            "--product-source-license",
            " Apache-2.0 ",
        ])
        .output()
        .expect("create from flags");
    assert!(flags.status.success());

    let mut child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["new", from_answers.to_str().expect("UTF-8 destination")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start interactive creation");
    child
        .stdin
        .as_mut()
        .expect("interactive stdin")
        .write_all(b" Acme Reader \n acme-reader \n Apache-2.0 \n")
        .expect("write interactive answers");
    let answers = child
        .wait_with_output()
        .expect("finish interactive creation");
    assert!(
        answers.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&answers.stderr)
    );

    assert_eq!(
        fs::read(from_flags.join(".yydra/origin.toml")).expect("flag origin"),
        fs::read(from_answers.join(".yydra/origin.toml")).expect("answer origin")
    );
}

#[test]
fn doctor_fails_closed_on_distribution_mismatch_without_mutating_the_workspace() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("workspace");
    create_with_flags(&workspace, "Acme Reader", "acme-reader");

    let origin_path = workspace.join(".yydra/origin.toml");
    let origin = fs::read_to_string(&origin_path).expect("read origin");
    fs::write(
        &origin_path,
        origin.replace(
            "distribution_version = \"0.1.0\"",
            "distribution_version = \"9.8.7\"",
        ),
    )
    .expect("write mismatched origin fixture");
    let before = byte_inventory(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["doctor", workspace.to_str().expect("UTF-8 workspace")])
        .output()
        .expect("run doctor");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("cargo install yydra-cli --version 9.8.7 --locked")
    );
    assert_eq!(before, byte_inventory(&workspace));
}

#[test]
fn doctor_fails_closed_on_template_digest_mismatch_without_mutation() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("workspace");
    create_with_flags(&workspace, "Acme Reader", "acme-reader");

    let origin_path = workspace.join(".yydra/origin.toml");
    let origin = fs::read_to_string(&origin_path).expect("read origin");
    let digest_line = origin
        .lines()
        .find(|line| line.starts_with("template_sha256 = "))
        .expect("template digest line");
    fs::write(
        &origin_path,
        origin.replace(
            digest_line,
            &format!("template_sha256 = \"{}\"", "0".repeat(64)),
        ),
    )
    .expect("write mismatched digest fixture");
    let before = byte_inventory(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["doctor", workspace.to_str().expect("UTF-8 workspace")])
        .output()
        .expect("run doctor");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("template digest mismatch"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("cargo install yydra-cli --version 0.1.0 --locked"));
    assert_eq!(before, byte_inventory(&workspace));
}

#[test]
fn product_input_is_rendered_as_data_not_as_a_template_token() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("workspace");
    create_with_flags(&workspace, "Reader __PRODUCT_ID__", "acme-reader");

    let origin = fs::read_to_string(workspace.join(".yydra/origin.toml")).expect("read origin");
    let readme = fs::read_to_string(workspace.join("README.md")).expect("read README");
    assert!(origin.contains("product_name = \"Reader __PRODUCT_ID__\""));
    assert!(readme.contains("# Reader __PRODUCT_ID__"));
}

#[test]
fn product_name_is_encoded_as_data_in_json_jsx_and_test_literals() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("encoded-name-workspace");
    let product_name = "Reader \"quote\" 'apostrophe' {__PRODUCT_ID__} </Text>";

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            workspace.to_str().expect("UTF-8 workspace"),
            "--product-name",
            product_name,
            "--product-id",
            "encoded-reader",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .expect("create Workspace with source-sensitive name");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let app: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("frontend/app.json")).expect("read app JSON"),
    )
    .expect("product name must not break app JSON");
    assert_eq!(app["expo"]["name"], product_name);
    let encoded = serde_json::to_string(product_name).expect("encode JS string literal");
    let route =
        fs::read_to_string(workspace.join("frontend/app/index.tsx")).expect("read product route");
    assert!(
        route.contains(&format!("{{{encoded}}}")),
        "route must embed the name as a JSX expression string: {route}"
    );
    let e2e = fs::read_to_string(workspace.join("frontend/e2e/clean-workspace.spec.ts"))
        .expect("read H5 spec");
    assert!(e2e.contains(&format!("name: {encoded}")));
}

#[test]
fn identical_inputs_produce_identical_path_mode_and_byte_inventories() {
    let sandbox = tempdir().expect("create test sandbox");
    let first = sandbox.path().join("first");
    let second = sandbox.path().join("second");
    create_with_flags(&first, "Acme Reader", "acme-reader");
    create_with_flags(&second, "Acme Reader", "acme-reader");

    assert_eq!(tracked_inventory(&first), tracked_inventory(&second));
}

#[test]
fn non_empty_destination_is_refused_without_overwrite_or_staging_debris() {
    let sandbox = tempdir().expect("create test sandbox");
    let destination = sandbox.path().join("existing");
    fs::create_dir(&destination).expect("create destination");
    fs::write(destination.join("keep.txt"), b"user-owned\n").expect("write sentinel");

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            destination.to_str().expect("UTF-8 destination"),
            "--product-name",
            "Acme Reader",
            "--product-id",
            "acme-reader",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .expect("run refused creation");

    assert!(!output.status.success());
    assert_eq!(
        fs::read(destination.join("keep.txt")).expect("read sentinel"),
        b"user-owned\n"
    );
    assert_eq!(
        fs::read_dir(&destination)
            .expect("list destination")
            .count(),
        1
    );
    assert!(
        fs::read_dir(sandbox.path())
            .expect("list sandbox")
            .all(|entry| !entry
                .expect("sandbox entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".yydra-stage-"))
    );
}

#[test]
fn an_existing_empty_destination_is_safely_materialized() {
    let sandbox = tempdir().expect("create test sandbox");
    let destination = sandbox.path().join("existing-empty");
    fs::create_dir(&destination).expect("create empty destination");

    create_with_flags(&destination, "Acme Reader", "acme-reader");

    assert!(destination.join(".yydra/origin.toml").is_file());
    let inventory = tracked_inventory(&destination);
    for required in [
        PathBuf::from("Cargo.lock"),
        PathBuf::from("crates/server/src/main.rs"),
        PathBuf::from("frontend/app/index.tsx"),
        PathBuf::from("migrations/0001_baseline.sql"),
    ] {
        assert!(inventory.contains_key(&required), "missing {required:?}");
    }
}

#[test]
fn invalid_product_identity_is_rejected_consistently_for_flags_and_answers() {
    let sandbox = tempdir().expect("create test sandbox");
    let from_flags = sandbox.path().join("flags");
    let from_answers = sandbox.path().join("answers");

    let flags = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            from_flags.to_str().expect("UTF-8 destination"),
            "--product-name",
            "Acme Reader",
            "--product-id",
            "Invalid_ID",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .expect("run invalid flags");

    let mut child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["new", from_answers.to_str().expect("UTF-8 destination")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start invalid interactive creation");
    child
        .stdin
        .as_mut()
        .expect("interactive stdin")
        .write_all(b"Acme Reader\nInvalid_ID\nApache-2.0\n")
        .expect("write invalid answers");
    let answers = child.wait_with_output().expect("finish invalid answers");

    let diagnostic = "product id must start with a lowercase letter";
    assert!(!flags.status.success());
    assert!(!answers.status.success());
    assert!(String::from_utf8_lossy(&flags.stderr).contains(diagnostic));
    assert!(String::from_utf8_lossy(&answers.stderr).contains(diagnostic));
    assert!(!from_flags.exists());
    assert!(!from_answers.exists());
}

#[test]
fn product_source_license_rejects_empty_or_control_bearing_values() {
    let sandbox = tempdir().expect("create test sandbox");
    for (name, license) in [
        ("empty", "   "),
        ("control", "MIT\u{0007}"),
        ("incomplete", "MIT OR"),
        ("unknown", "not-a-license"),
    ] {
        let destination = sandbox.path().join(name);
        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args([
                "new",
                destination.to_str().expect("UTF-8 destination"),
                "--product-name",
                "Invalid License Reader",
                "--product-id",
                "invalid-license-reader",
                "--product-source-license",
                license,
            ])
            .output()
            .expect("run invalid product source license");

        assert!(!output.status.success(), "accepted invalid value {name}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("product source license"),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!destination.exists());
    }
}

#[test]
fn doctor_rejects_origin_schema_and_template_identity_mismatch() {
    let sandbox = tempdir().expect("create test sandbox");
    for (name, old, replacement, diagnostic) in [
        (
            "schema",
            "schema_version = 1",
            "schema_version = 2",
            "origin schema mismatch",
        ),
        (
            "identity",
            "template_identity = \"yydra-v0-product-workspace\"",
            "template_identity = \"mutable-remote-template\"",
            "template identity mismatch",
        ),
    ] {
        let workspace = sandbox.path().join(name);
        create_with_flags(&workspace, "Acme Reader", "acme-reader");
        let origin_path = workspace.join(".yydra/origin.toml");
        let origin = fs::read_to_string(&origin_path).expect("read origin");
        fs::write(&origin_path, origin.replace(old, replacement)).expect("write mismatch fixture");
        let before = byte_inventory(&workspace);

        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args(["doctor", workspace.to_str().expect("UTF-8 workspace")])
            .output()
            .expect("run doctor");

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(before, byte_inventory(&workspace));
    }
}

#[test]
fn doctor_rejects_malformed_ambiguous_or_non_semver_origin_records() {
    let sandbox = tempdir().expect("create test sandbox");
    for (name, mutate, diagnostic) in [
        (
            "duplicate",
            fn_append_duplicate_version as fn(String) -> String,
            "invalid Workspace Origin Record",
        ),
        (
            "malformed",
            fn_append_malformed_table as fn(String) -> String,
            "invalid Workspace Origin Record",
        ),
        (
            "non-semver",
            fn_replace_with_non_semver as fn(String) -> String,
            "invalid distribution version",
        ),
    ] {
        let workspace = sandbox.path().join(name);
        create_with_flags(&workspace, "Acme Reader", "acme-reader");
        let origin_path = workspace.join(".yydra/origin.toml");
        let origin = fs::read_to_string(&origin_path).expect("read origin");
        fs::write(&origin_path, mutate(origin)).expect("write invalid origin fixture");
        let before = byte_inventory(&workspace);

        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args(["doctor", workspace.to_str().expect("UTF-8 workspace")])
            .output()
            .expect("run doctor");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(diagnostic), "stderr: {stderr}");
        assert!(!stderr.contains("--version not-semver --locked"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("status=pass"));
        assert_eq!(before, byte_inventory(&workspace));
    }
}

#[cfg(unix)]
#[test]
fn api_generation_staged_failure_preserves_every_committed_output() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("atomic-api-reader");
    create_with_flags(&workspace, "Atomic API Reader", "atomic-api-reader");
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("openapi.json");
    let mut proposed_openapi: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
    )
    .expect("parse OpenAPI fixture");
    proposed_openapi["components"]["schemas"]["FrameworkContractProfile"]["properties"]["futureOptional"] =
        serde_json::json!({ "type": "string" });
    let mut proposed_openapi =
        serde_json::to_vec_pretty(&proposed_openapi).expect("serialize proposed OpenAPI");
    proposed_openapi.push(b'\n');
    fs::write(&fixture_openapi, proposed_openapi).expect("write proposed OpenAPI fixture");
    let fixture_generated = sandbox.path().join("generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);
    let before = byte_inventory(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .env("YYDRA_TEST_API_GENERATION_FAIL_AFTER_STAGE", "1")
        .output()
        .expect("run staged API failure fixture");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("API_STAGED_FAILURE_INJECTED"));
    assert_eq!(before, byte_inventory(&workspace));
}

#[cfg(unix)]
#[test]
fn api_generation_swap_failure_restores_the_complete_previous_output_set() {
    let sandbox = tempdir().expect("create test sandbox");
    let fake_bin = fake_api_generation_tools(&sandbox);
    for index in 0..4 {
        let workspace = sandbox.path().join(format!("rollback-api-reader-{index}"));
        create_with_flags(
            &workspace,
            "Rollback API Reader",
            &format!("rollback-api-reader-{index}"),
        );
        install_fake_api_tool_authority(&workspace, "8.27.0");
        let fixture_openapi = sandbox
            .path()
            .join(format!("rollback-openapi-{index}.json"));
        let mut proposed_openapi: serde_json::Value = serde_json::from_slice(
            &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
        )
        .expect("parse OpenAPI fixture");
        proposed_openapi["components"]["schemas"]["FrameworkContractProfile"]["properties"]["futureOptional"] =
            serde_json::json!({ "type": "string" });
        let mut proposed_openapi =
            serde_json::to_vec_pretty(&proposed_openapi).expect("serialize proposed OpenAPI");
        proposed_openapi.push(b'\n');
        fs::write(&fixture_openapi, proposed_openapi).expect("write proposed OpenAPI fixture");
        let fixture_generated = sandbox.path().join(format!("rollback-generated-{index}"));
        copy_directory(
            &workspace.join("frontend/src/generated/public-api"),
            &fixture_generated,
        );
        let before = byte_inventory(&workspace);

        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args([
                "generate",
                "api",
                workspace.to_str().expect("UTF-8 workspace"),
            ])
            .env("PATH", &fake_bin)
            .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
            .env("YYDRA_FAKE_GENERATED", fixture_generated)
            .env(
                "YYDRA_TEST_API_GENERATION_FAIL_REPLACE_INDEX",
                index.to_string(),
            )
            .output()
            .expect("run API swap failure fixture");

        assert!(!output.status.success(), "index={index}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("API_GENERATION_TRANSACTION_FAILED"),
            "index={index}, stderr={stderr}"
        );
        assert!(
            stderr.contains("rollback completed"),
            "index={index}, stderr={stderr}"
        );
        assert_same_byte_inventory(&before, &byte_inventory(&workspace));
    }
}

#[cfg(unix)]
#[test]
fn api_generation_lock_excludes_a_concurrent_reader_without_mutation() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("locked-api-reader");
    create_with_flags(&workspace, "Locked API Reader", "locked-api-reader");
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("locked-openapi.json");
    fs::copy(workspace.join("contracts/openapi.json"), &fixture_openapi)
        .expect("copy OpenAPI fixture");
    let fixture_generated = sandbox.path().join("locked-generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
: > "$YYDRA_LOCK_READY"
while [ ! -f "$YYDRA_LOCK_RELEASE" ]; do /bin/sleep 0.01; done
last=""
for argument in "$@"; do last="$argument"; done
/bin/mkdir -p "$(/usr/bin/dirname "$last")"
/bin/cp "$YYDRA_FAKE_OPENAPI" "$last"
"#,
    );
    let ready = sandbox.path().join("lock-ready");
    let release = sandbox.path().join("lock-release");
    let before = byte_inventory(&workspace);

    let mut writer = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .env("YYDRA_LOCK_READY", &ready)
        .env("YYDRA_LOCK_RELEASE", &release)
        .spawn()
        .expect("start locked API generation fixture");
    for _ in 0..500 {
        if ready.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        ready.is_file(),
        "writer did not acquire the generation lock"
    );

    let reader = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--check",
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .output()
        .expect("run concurrent API reader fixture");
    assert!(!reader.status.success());
    assert!(String::from_utf8_lossy(&reader.stderr).contains("API_GENERATION_BUSY"));
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));

    fs::write(&release, b"release\n").expect("release API generation writer");
    let writer_status = writer.wait().expect("wait for API generation writer");
    assert!(writer_status.success());
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));
}

#[cfg(unix)]
#[test]
fn api_generation_recovers_an_abruptly_terminated_transaction_before_writing_again() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("crash-recovery-reader");
    create_with_flags(&workspace, "Crash Recovery Reader", "crash-recovery-reader");
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("crash-openapi.json");
    let mut proposed_openapi: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
    )
    .expect("parse OpenAPI fixture");
    proposed_openapi["components"]["schemas"]["FrameworkContractProfile"]["properties"]["futureOptional"] =
        serde_json::json!({ "type": "string" });
    let mut proposed_openapi =
        serde_json::to_vec_pretty(&proposed_openapi).expect("serialize proposed OpenAPI");
    proposed_openapi.push(b'\n');
    fs::write(&fixture_openapi, proposed_openapi).expect("write proposed OpenAPI fixture");
    let fixture_generated = sandbox.path().join("crash-generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);
    let before = byte_inventory(&workspace);

    let crashed = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .env("YYDRA_TEST_API_GENERATION_CRASH_REPLACE_INDEX", "1")
        .output()
        .expect("run abrupt API transaction fixture");
    assert_eq!(crashed.status.code(), Some(86));
    let interrupted = byte_inventory(&workspace);
    assert_ne!(before, interrupted);

    let read_only = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--check",
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .output()
        .expect("check interrupted API transaction fixture");
    assert!(!read_only.status.success());
    assert!(
        String::from_utf8_lossy(&read_only.stderr).contains("API_GENERATION_RECOVERY_REQUIRED")
    );
    assert_same_byte_inventory(&interrupted, &byte_inventory(&workspace));

    let recovered = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .env("YYDRA_TEST_API_GENERATION_FAIL_AFTER_STAGE", "1")
        .output()
        .expect("recover interrupted API transaction fixture");
    assert!(!recovered.status.success());
    assert!(String::from_utf8_lossy(&recovered.stderr).contains("API_STAGED_FAILURE_INJECTED"));
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));
}

#[cfg(unix)]
#[test]
fn api_generation_rolls_back_a_crash_before_the_commit_marker_is_published() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("commit-marker-crash-reader");
    create_with_flags(
        &workspace,
        "Commit Marker Crash Reader",
        "commit-marker-crash-reader",
    );
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("commit-marker-crash-openapi.json");
    let mut proposed_openapi: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
    )
    .expect("parse OpenAPI fixture");
    proposed_openapi["components"]["schemas"]["FrameworkContractProfile"]["properties"]["futureOptional"] =
        serde_json::json!({ "type": "string" });
    let mut proposed_openapi =
        serde_json::to_vec_pretty(&proposed_openapi).expect("serialize proposed OpenAPI");
    proposed_openapi.push(b'\n');
    fs::write(&fixture_openapi, &proposed_openapi).expect("write proposed OpenAPI fixture");
    let fixture_generated = sandbox.path().join("commit-marker-crash-generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);
    let before = byte_inventory(&workspace);

    let crashed = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .env("YYDRA_TEST_API_GENERATION_CRASH_COMMIT_MARKER", "1")
        .output()
        .expect("run commit-marker crash fixture");
    assert_eq!(crashed.status.code(), Some(87));
    assert_eq!(
        fs::read(workspace.join("contracts/openapi.json")).expect("read interrupted OpenAPI"),
        proposed_openapi
    );

    let recovered = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .env("YYDRA_TEST_API_GENERATION_FAIL_AFTER_STAGE", "1")
        .output()
        .expect("recover commit-marker crash fixture");
    assert!(!recovered.status.success());
    assert!(String::from_utf8_lossy(&recovered.stderr).contains("API_STAGED_FAILURE_INJECTED"));
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));
}

#[cfg(unix)]
#[test]
fn api_generation_cleanup_failure_never_rolls_back_a_committed_output_set() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("committed-cleanup-reader");
    create_with_flags(
        &workspace,
        "Committed Cleanup Reader",
        "committed-cleanup-reader",
    );
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("committed-cleanup-openapi.json");
    let mut proposed_openapi: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
    )
    .expect("parse OpenAPI fixture");
    proposed_openapi["components"]["schemas"]["FrameworkContractProfile"]["properties"]["futureOptional"] =
        serde_json::json!({ "type": "string" });
    let mut proposed_openapi =
        serde_json::to_vec_pretty(&proposed_openapi).expect("serialize proposed OpenAPI");
    proposed_openapi.push(b'\n');
    fs::write(&fixture_openapi, &proposed_openapi).expect("write proposed OpenAPI fixture");
    let fixture_generated = sandbox.path().join("committed-cleanup-generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);

    let failed_cleanup = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .env("YYDRA_TEST_API_GENERATION_FAIL_CLEANUP", "1")
        .output()
        .expect("run committed cleanup failure fixture");
    assert!(!failed_cleanup.status.success());
    assert!(
        String::from_utf8_lossy(&failed_cleanup.stderr)
            .contains("committed outputs but cleanup was intentionally rejected")
    );
    assert_eq!(
        fs::read(workspace.join("contracts/openapi.json")).expect("read committed OpenAPI"),
        proposed_openapi
    );
    let interrupted = byte_inventory(&workspace);
    let committed_record = fs::read(workspace.join(".yydra/api-generation.json"))
        .expect("read committed generation record");
    let committed_history = fs::read(workspace.join(".yydra/api-generation-history.json"))
        .expect("read committed generation history");
    let committed_client = byte_inventory(&workspace.join("frontend/src/generated/public-api"));

    let read_only = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--check",
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .output()
        .expect("check committed cleanup failure fixture");
    assert!(!read_only.status.success());
    assert!(
        String::from_utf8_lossy(&read_only.stderr).contains("API_GENERATION_RECOVERY_REQUIRED")
    );
    assert_same_byte_inventory(&interrupted, &byte_inventory(&workspace));

    let recovered = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .output()
        .expect("clean committed transaction fixture");
    assert!(
        recovered.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_eq!(
        fs::read(workspace.join("contracts/openapi.json")).expect("read recovered OpenAPI"),
        proposed_openapi
    );
    assert_eq!(
        fs::read(workspace.join(".yydra/api-generation.json"))
            .expect("read recovered generation record"),
        committed_record
    );
    assert_eq!(
        fs::read(workspace.join(".yydra/api-generation-history.json"))
            .expect("read recovered generation history"),
        committed_history
    );
    assert_same_byte_inventory(
        &committed_client,
        &byte_inventory(&workspace.join("frontend/src/generated/public-api")),
    );
    assert!(
        fs::read_dir(&workspace)
            .expect("read recovered Workspace")
            .all(|entry| !entry
                .expect("read recovered entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".yydra-api-transaction-"))
    );
}

#[cfg(unix)]
#[test]
fn api_generation_write_is_idempotent_when_outputs_are_current() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("idempotent-api-reader");
    create_with_flags(&workspace, "Idempotent API Reader", "idempotent-api-reader");
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("openapi.json");
    fs::copy(workspace.join("contracts/openapi.json"), &fixture_openapi)
        .expect("copy OpenAPI fixture");
    let fixture_generated = sandbox.path().join("generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);
    let before = byte_inventory(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .output()
        .expect("run idempotent API generation fixture");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));
}

#[cfg(unix)]
#[test]
fn api_generation_allows_product_owned_reading_queue_replacement() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("product-api-reader");
    create_with_flags(&workspace, "Product API Reader", "product-api-reader");
    install_fake_api_tool_authority(&workspace, "8.27.0");

    let fixture_openapi = sandbox.path().join("product-openapi.json");
    let mut openapi: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
    )
    .expect("parse OpenAPI fixture");
    openapi["paths"]
        .as_object_mut()
        .expect("OpenAPI paths")
        .retain(|path, _| !path.starts_with("/api/v1/reading-queue"));
    let schemas = openapi["components"]["schemas"]
        .as_object_mut()
        .expect("OpenAPI schemas");
    for schema in [
        "CreateReadingEntryRequest",
        "ChangeReadingEntryStateRequest",
        "ReadingQueueEntryResponse",
        "ReadingQueueEntryState",
        "ReadingQueueResponse",
        "ReadingQueueSort",
        "ReadingQueueStatusFilter",
    ] {
        schemas.remove(schema);
    }
    let mut openapi = serde_json::to_vec_pretty(&openapi).expect("serialize OpenAPI fixture");
    openapi.push(b'\n');
    fs::write(&fixture_openapi, openapi).expect("write product-owned OpenAPI fixture");

    let fixture_generated = sandbox.path().join("product-generated");
    for (relative, source) in [
        (
            "fetch/client.ts",
            "// SPDX-License-Identifier: MIT OR Apache-2.0\n// Do not edit manually.\n// getFrameworkContractProfile fetchFn FrameworkContractProfile.parse\n",
        ),
        (
            "fetch/schemas/frameworkContractProfile.zod.ts",
            "export const FrameworkContractProfile = zod.object({});\n",
        ),
        (
            "fetch/schemas/problemDetails.zod.ts",
            "export const ProblemDetails = zod.object({});\n",
        ),
        (
            "request/schemas/frameworkContractCreate.zod.ts",
            "export const FrameworkContractCreate = zod.strictObject({});\n",
        ),
        (
            "request/schemas/frameworkContractPatch.zod.ts",
            "export const FrameworkContractPatch = zod.strictObject({});\n",
        ),
    ] {
        let path = fixture_generated.join(relative);
        fs::create_dir_all(path.parent().expect("generated parent"))
            .expect("create generated fixture directory");
        fs::write(path, source).expect("write generated fixture");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--acknowledge-breaking-change",
            "issue-32-product-evolution",
        ])
        .env("PATH", fake_api_generation_tools(&sandbox))
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .output()
        .expect("replace the bounded Reading Queue fixture");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let committed = fs::read_to_string(workspace.join("contracts/openapi.json"))
        .expect("read evolved Product API");
    assert!(!committed.contains("reading-queue"));
    assert!(!committed.contains("ReadingQueue"));
}

#[cfg(unix)]
#[test]
fn api_check_discriminates_compatible_breaking_and_client_drift_read_only() {
    let sandbox = tempdir().expect("create test sandbox");
    for case in [
        "compatible-openapi",
        "breaking-openapi",
        "breaking-required-relaxation",
        "breaking-required-parameter",
        "breaking-constraint",
        "unsupported-path-parameters",
        "unsupported-global-security",
        "unsupported-component-parameters",
        "client",
    ] {
        let workspace = sandbox.path().join(case);
        create_with_flags(&workspace, "Contract Drift Reader", "contract-drift-reader");
        install_fake_api_tool_authority(&workspace, "8.27.0");
        let fixture_openapi = sandbox.path().join(format!("{case}-openapi.json"));
        let mut openapi: serde_json::Value = serde_json::from_slice(
            &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
        )
        .expect("parse OpenAPI fixture");
        match case {
            "compatible-openapi" => {
                openapi["components"]["schemas"]["FrameworkContractProfile"]["properties"]["futureOptional"] =
                    serde_json::json!({ "type": "string" });
            }
            "breaking-openapi" => {
                openapi["components"]["schemas"]["ProblemDetails"]["properties"]
                    .as_object_mut()
                    .expect("Problem properties")
                    .remove("detail");
            }
            "breaking-required-relaxation" => {
                openapi["components"]["schemas"]["ProblemDetails"]["required"]
                    .as_array_mut()
                    .expect("Problem required array")
                    .retain(|field| field != "title");
            }
            "breaking-required-parameter" => {
                openapi["paths"]["/api/v1/framework-contract"]["get"]["parameters"] = serde_json::json!([{
                    "in": "query",
                    "name": "requiredMode",
                    "required": true,
                    "schema": { "type": "string" }
                }]);
            }
            "breaking-constraint" => {
                openapi["components"]["schemas"]["FrameworkContractProfile"]["properties"]["opaqueId"]
                    ["minLength"] = serde_json::json!(1);
            }
            "unsupported-path-parameters" => {
                openapi["paths"]["/api/v1/framework-contract"]["parameters"] =
                    serde_json::json!([]);
            }
            "unsupported-global-security" => {
                openapi["security"] = serde_json::json!([]);
            }
            "unsupported-component-parameters" => {
                openapi["components"]["parameters"] = serde_json::json!({});
            }
            "client" => {}
            _ => unreachable!(),
        }
        let mut openapi_bytes = serde_json::to_vec_pretty(&openapi).expect("serialize fixture");
        openapi_bytes.push(b'\n');
        fs::write(&fixture_openapi, openapi_bytes).expect("write OpenAPI fixture");

        let fixture_generated = sandbox.path().join(format!("{case}-generated"));
        copy_directory(
            &workspace.join("frontend/src/generated/public-api"),
            &fixture_generated,
        );
        if case == "client" {
            let path = workspace.join("frontend/src/generated/public-api/fetch/client.ts");
            let mut bytes = fs::read(&path).expect("read client fixture");
            bytes.extend_from_slice(b"\n// hand edit\n");
            fs::write(path, bytes).expect("drift committed client");
        }
        let fake_bin = fake_api_generation_tools(&sandbox);
        let before = byte_inventory(&workspace);
        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args([
                "generate",
                "api",
                workspace.to_str().expect("UTF-8 workspace"),
                "--check",
            ])
            .env("PATH", fake_bin)
            .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
            .env("YYDRA_FAKE_GENERATED", &fixture_generated)
            .output()
            .expect("run API drift fixture");

        assert!(!output.status.success(), "case {case} unexpectedly passed");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let expected = match case {
            "compatible-openapi" => "API_GENERATED_DRIFT",
            "breaking-openapi"
            | "breaking-required-relaxation"
            | "breaking-required-parameter"
            | "breaking-constraint" => "API_BREAKING_CHANGE_UNACKNOWLEDGED",
            "unsupported-path-parameters"
            | "unsupported-global-security"
            | "unsupported-component-parameters" => "API_OPENAPI_PROFILE_INVALID",
            "client" => "API_CLIENT_DRIFT",
            _ => unreachable!(),
        };
        assert!(stderr.contains(expected), "case={case}, stderr={stderr}");
        assert_eq!(before, byte_inventory(&workspace), "case={case}");
    }
}

#[cfg(unix)]
#[test]
fn api_check_rejects_generation_record_authority_drift_read_only() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("record-drift-reader");
    create_with_flags(&workspace, "Record Drift Reader", "record-drift-reader");
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("record-openapi.json");
    fs::copy(workspace.join("contracts/openapi.json"), &fixture_openapi)
        .expect("copy OpenAPI fixture");
    let fixture_generated = sandbox.path().join("record-generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let record_path = workspace.join(".yydra/api-generation.json");
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(&record_path).expect("read generation record"))
            .expect("parse generation record");
    record["sourceAuthority"] = serde_json::json!("hand-edited-authority");
    let mut record = serde_json::to_vec_pretty(&record).expect("serialize generation record");
    record.push(b'\n');
    fs::write(&record_path, record).expect("drift generation record");
    let fake_bin = fake_api_generation_tools(&sandbox);
    let before = byte_inventory(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--check",
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .output()
        .expect("run generation-record drift fixture");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("API_GENERATION_RECORD_DRIFT"));
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));
}

#[cfg(unix)]
#[test]
fn api_check_recomputes_the_recorded_breaking_decision_from_history() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("history-reader");
    create_with_flags(&workspace, "History Reader", "history-reader");
    install_fake_api_tool_authority(&workspace, "8.27.0");
    let fixture_openapi = sandbox.path().join("history-openapi.json");
    let mut openapi: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("contracts/openapi.json")).expect("read OpenAPI fixture"),
    )
    .expect("parse OpenAPI fixture");
    openapi["components"]["schemas"]["ProblemDetails"]["properties"]
        .as_object_mut()
        .expect("Problem properties")
        .remove("detail");
    let mut openapi = serde_json::to_vec_pretty(&openapi).expect("serialize OpenAPI fixture");
    openapi.push(b'\n');
    fs::write(&fixture_openapi, openapi).expect("write OpenAPI fixture");
    let fixture_generated = sandbox.path().join("history-generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);
    let initial_history_len = serde_json::from_slice::<serde_json::Value>(
        &fs::read(workspace.join(".yydra/api-generation-history.json"))
            .expect("read initial generation history"),
    )
    .expect("parse initial generation history")
    .as_array()
    .map(Vec::len)
    .expect("generation history array");

    let generated = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--acknowledge-breaking-change",
            "issue-31",
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .output()
        .expect("generate acknowledged breaking fixture");
    assert!(
        generated.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let history: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join(".yydra/api-generation-history.json"))
            .expect("read generation history"),
    )
    .expect("parse generation history");
    assert_eq!(
        history.as_array().map(Vec::len),
        Some(initial_history_len + 1)
    );

    let clean = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--check",
        ])
        .env("PATH", &fake_bin)
        .env("YYDRA_FAKE_OPENAPI", &fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", &fixture_generated)
        .output()
        .expect("check acknowledged breaking fixture");
    assert!(
        clean.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&clean.stderr)
    );

    let record_path = workspace.join(".yydra/api-generation.json");
    let record = fs::read_to_string(&record_path).expect("read generation record");
    let decision_start = record
        .find("  \"breakingChanges\": [")
        .expect("breaking decision start");
    let replacement = "  \"breakingChanges\": [],\n  \"acknowledgements\": []\n";
    let mut tampered = record[..decision_start].to_owned();
    tampered.push_str(replacement);
    tampered.push_str("}\n");
    fs::write(&record_path, tampered).expect("clear recorded breaking decision");
    let before = byte_inventory(&workspace);

    let rejected = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--check",
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .output()
        .expect("check tampered breaking fixture");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("API_GENERATION_RECORD_DRIFT"));
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));
}

#[cfg(unix)]
#[test]
fn api_generation_rejects_a_different_installed_orval_version_read_only() {
    let sandbox = tempdir().expect("create test sandbox");
    let workspace = sandbox.path().join("orval-drift-reader");
    create_with_flags(&workspace, "Orval Drift Reader", "orval-drift-reader");
    install_fake_api_tool_authority(&workspace, "8.26.0");
    let fixture_openapi = sandbox.path().join("orval-openapi.json");
    fs::copy(workspace.join("contracts/openapi.json"), &fixture_openapi)
        .expect("copy OpenAPI fixture");
    let fixture_generated = sandbox.path().join("orval-generated");
    copy_directory(
        &workspace.join("frontend/src/generated/public-api"),
        &fixture_generated,
    );
    let fake_bin = fake_api_generation_tools(&sandbox);
    let before = byte_inventory(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "generate",
            "api",
            workspace.to_str().expect("UTF-8 workspace"),
            "--check",
        ])
        .env("PATH", fake_bin)
        .env("YYDRA_FAKE_OPENAPI", fixture_openapi)
        .env("YYDRA_FAKE_GENERATED", fixture_generated)
        .output()
        .expect("run Orval authority drift fixture");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("API_CLIENT_TOOL_VERSION_INVALID"));
    assert_same_byte_inventory(&before, &byte_inventory(&workspace));
}

fn fn_append_duplicate_version(mut origin: String) -> String {
    origin.push_str("distribution_version = \"9.9.9\"\n");
    origin
}

fn fn_append_malformed_table(mut origin: String) -> String {
    origin.push_str("[unfinished\n");
    origin
}

fn fn_replace_with_non_semver(origin: String) -> String {
    origin.replace(
        "distribution_version = \"0.1.0\"",
        "distribution_version = \"not-semver\"",
    )
}

#[cfg(unix)]
#[test]
fn creation_modes_do_not_depend_on_the_callers_umask() {
    let sandbox = tempdir().expect("create test sandbox");
    let normal = sandbox.path().join("normal");
    let restrictive = sandbox.path().join("restrictive");
    create_with_flags(&normal, "Acme Reader", "acme-reader");

    let output = Command::new("/bin/sh")
        .args([
            "-c",
            "umask 077; exec \"$YYDRA_BIN\" new \"$YYDRA_DEST\" --product-name 'Acme Reader' --product-id acme-reader --product-source-license Apache-2.0",
        ])
        .env("YYDRA_BIN", env!("CARGO_BIN_EXE_yydra"))
        .env("YYDRA_DEST", &restrictive)
        .output()
        .expect("create with restrictive umask");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(tracked_inventory(&normal), tracked_inventory(&restrictive));
}

fn create_with_flags(destination: &Path, product_name: &str, product_id: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            destination.to_str().expect("UTF-8 destination"),
            "--product-name",
            product_name,
            "--product-id",
            product_id,
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .expect("create Product Workspace");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create copied directory");
    for entry in fs::read_dir(source).expect("read copied directory") {
        let entry = entry.expect("read copied entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("read copied type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy fixture file");
        }
    }
}

#[cfg(unix)]
fn fake_api_generation_tools(sandbox: &tempfile::TempDir) -> PathBuf {
    let fake_bin = sandbox.path().join("fake-api-bin");
    fs::create_dir_all(&fake_bin).expect("create fake API tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        r#"#!/bin/sh
last=""
for argument in "$@"; do last="$argument"; done
/bin/mkdir -p "$(/usr/bin/dirname "$last")"
/bin/cp "$YYDRA_FAKE_OPENAPI" "$last"
"#,
    );
    write_executable(
        &fake_bin.join("npm"),
        r#"#!/bin/sh
if [ -n "$YYDRA_GENERATED_API_OUTPUT" ]; then
  /bin/mkdir -p "$YYDRA_GENERATED_API_OUTPUT"
  /bin/cp -R "$YYDRA_FAKE_GENERATED/." "$YYDRA_GENERATED_API_OUTPUT/"
fi
"#,
    );
    fake_bin
}

fn install_fake_api_tool_authority(workspace: &Path, version: &str) {
    let package = workspace.join("frontend/node_modules/orval/package.json");
    fs::create_dir_all(package.parent().expect("Orval package parent"))
        .expect("create fake Orval package directory");
    fs::write(
        package,
        serde_json::to_vec(&serde_json::json!({ "version": version }))
            .expect("serialize fake Orval package metadata"),
    )
    .expect("write fake Orval package metadata");
}

fn byte_inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).expect("read inventory directory") {
            let path = entry.expect("read inventory entry").path();
            if path.is_dir() {
                visit(root, &path, files);
            } else if path.is_file() {
                files.insert(
                    path.strip_prefix(root)
                        .expect("relative path")
                        .to_path_buf(),
                    fs::read(path).expect("read inventory file"),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

fn assert_same_byte_inventory(
    before: &BTreeMap<PathBuf, Vec<u8>>,
    after: &BTreeMap<PathBuf, Vec<u8>>,
) {
    let changed = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(changed.is_empty(), "changed paths: {changed:?}");
}

fn tracked_inventory(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    byte_inventory(root)
        .into_iter()
        .map(|(relative, bytes)| {
            let metadata = fs::metadata(root.join(&relative)).expect("read inventory metadata");
            #[cfg(unix)]
            let mode = metadata.permissions().mode() & 0o777;
            #[cfg(not(unix))]
            let mode = u32::from(metadata.permissions().readonly());
            (relative, (mode, bytes))
        })
        .collect()
}

#[cfg(unix)]
fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .expect("set executable fixture mode");
}
