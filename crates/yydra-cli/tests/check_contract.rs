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
    create_workspace_with_inputs(
        destination,
        "Check Fixture",
        product_id,
        "MIT OR Apache-2.0",
    );
}

fn create_workspace_with_inputs(
    destination: &Path,
    product_name: &str,
    product_id: &str,
    product_source_license: &str,
) {
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            destination.to_str().expect("UTF-8 destination"),
            "--product-name",
            product_name,
            "--product-id",
            product_id,
            "--product-source-license",
            product_source_license,
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
    check_with_path(workspace, evidence, nodes, None)
}

fn check_with_path(
    workspace: &Path,
    evidence: &Path,
    nodes: &[&str],
    path: Option<&str>,
) -> Output {
    let _heavy_guard = nodes
        .iter()
        .any(|node| {
            node.starts_with("api.")
                || node.starts_with("database.runtime-")
                || node.starts_with("frontend.")
                || node.starts_with("h5.")
                || node.starts_with("runtime.")
                || node.starts_with("supply-chain.")
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
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().expect("run Mechanical Quality Contract")
}

#[cfg(unix)]
fn write_fake_osv_node(directory: &Path, mode: &str) {
    fs::create_dir_all(directory).expect("create fake OSV tool directory");
    let resolved = Command::new("sh")
        .args(["-c", "command -v node"])
        .output()
        .expect("resolve real Node");
    assert!(resolved.status.success(), "resolve real Node executable");
    let real_node = String::from_utf8(resolved.stdout)
        .expect("UTF-8 Node path")
        .trim()
        .to_owned();
    assert!(real_node.starts_with('/'));
    assert!(!real_node.contains('\''));
    let body = format!(
        r#"#!/bin/sh
case "$1" in
  *osv-query.mjs)
    if [ "{mode}" = "outage" ]; then
      printf '%s\n' 'simulated OSV outage' >&2
      exit 75
    fi
    exec '{real_node}' -e '
      const fs = require("node:fs");
      const request = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
      const mode = process.argv[3];
      const results = request.queries.map((query) =>
        mode === "vulnerable" && query.package?.name === "react"
          ? {{ vulns: [{{ id: "OSV-TEST-REACT", modified: "2026-09-03T00:00:00Z" }}] }}
          : mode === "informational" && query.package?.name === "paste"
            ? {{ vulns: [{{
                id: "RUSTSEC-TEST-UNMAINTAINED",
                modified: "2026-09-03T00:00:00Z",
                affected: [{{ database_specific: {{ informational: "unmaintained" }} }}],
                severity: []
              }}] }}
          : {{}}
      );
      fs.writeFileSync(process.argv[2], JSON.stringify({{ results }}) + "\n");
    ' "$2" "$3" '{mode}'
    ;;
  *) exec '{real_node}' "$@" ;;
esac
"#,
    );
    write_executable(&directory.join("node"), &body);
}

#[cfg(unix)]
fn write_supply_chain_npm_fixture(directory: &Path, mode: &str) {
    fs::create_dir_all(directory).expect("create fake npm tool directory");
    let resolve = |program: &str| {
        let output = Command::new("sh")
            .args(["-c", &format!("command -v {program}")])
            .output()
            .unwrap_or_else(|error| panic!("resolve {program}: {error}"));
        assert!(output.status.success(), "resolve {program}");
        let path = String::from_utf8(output.stdout)
            .expect("UTF-8 tool path")
            .trim()
            .to_owned();
        assert!(path.starts_with('/'));
        assert!(!path.contains('\''));
        path
    };
    let real_npm = resolve("npm");
    let real_node = resolve("node");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" != "ci" ]; then exec '{real_npm}' "$@"; fi
'{real_npm}' "$@" || exit $?
case '{mode}' in
  provenance)
    exec '{real_node}' -e '
      const fs = require("node:fs");
      const path = "node_modules/react/package.json";
      const value = JSON.parse(fs.readFileSync(path, "utf8"));
      value.version = "19.2.4";
      fs.writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
    '
    ;;
  notice) rm -f node_modules/react/LICENSE ;;
  license-out-of-scope)
    exec '{real_node}' -e '
      const fs = require("node:fs");
      const path = "node_modules/react/package.json";
      const value = JSON.parse(fs.readFileSync(path, "utf8"));
      delete value.license;
      fs.writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
      const duplicatePath = "node_modules/compression/node_modules/debug/package.json";
      const duplicate = JSON.parse(fs.readFileSync(duplicatePath, "utf8"));
      duplicate.license = "LicenseRef-OutOfScope";
      fs.writeFileSync(duplicatePath, JSON.stringify(duplicate, null, 2) + "\n");
    '
    ;;
  *) exit 91 ;;
esac
"#,
    );
    write_executable(&directory.join("npm"), &body);
}

#[test]
fn supply_chain_policy_does_not_interpret_legacy_notice_requirements() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("nested-notice");
    create_workspace(&workspace, "nested-notice");
    let path = workspace.join(".yydra/supply-chain-policy.json");
    let mut policy: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    policy["requiredNotices"]
        .as_array_mut()
        .unwrap()
        .retain(|notice| notice["purl"] != "pkg:cargo/regex-syntax@0.8.11");
    policy["requiredNotices"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "purl": "pkg:cargo/regex-syntax@0.8.11",
            "path": "src/unicode_tables/LICENSE-UNICODE",
            "sha256": "sha256:74db5baf44a41b1000312c673544b3374e4198af5605c7f9080a402cec42cfa3"
        }));
    let index = policy["requiredNotices"].as_array().unwrap().len() - 1;
    for (case, relative) in [
        "src/unicode_tables/LICENSE-UNICODE",
        "../LICENSE",
        "/LICENSE",
        "src/../LICENSE",
        "src//LICENSE",
        "src/./LICENSE",
        "src/LICENSE/",
        "src\\LICENSE",
        "C:/LICENSE",
        "src/\u{0}LICENSE",
    ]
    .into_iter()
    .enumerate()
    {
        policy["requiredNotices"][index]["path"] = relative.into();
        fs::write(&path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();
        let before = workspace_files(&workspace);
        let output = check(
            &workspace,
            &sandbox.path().join(format!("evidence-{case}")),
            &["supply-chain.policy"],
        );
        let parsed = events(&output);
        let result = node(&parsed, "supply-chain.policy");
        assert!(
            output.status.success(),
            "legacy notice metadata is not read or enforced: {result:?}"
        );
        assert_eq!(
            workspace_files(&workspace),
            before,
            "validation is read-only"
        );
    }
}

#[cfg(unix)]
#[test]
fn supply_chain_dependency_inventory_does_not_require_license_declarations() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("unreviewed-license-reader");
    create_workspace(&workspace, "unreviewed-license-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    write_supply_chain_npm_fixture(&fake_bin, "license-out-of-scope");
    let path = format!("{}:{}", fake_bin.display(), std::env::var("PATH").unwrap());
    let evidence = sandbox.path().join("evidence");
    let output = check_with_path(
        &workspace,
        &evidence,
        &["supply-chain.dependencies"],
        Some(&path),
    );
    let parsed = events(&output);
    assert!(
        output.status.success(),
        "missing license declaration is out of scope: {:?}",
        node(&parsed, "supply-chain.dependencies")
    );
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/supply-chain.dependencies/inventory.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["notEvaluated"],
        serde_json::json!([
            "license-review",
            "dependency-source-trust",
            "upstream-provenance",
            "notice-completeness"
        ])
    );
    let react = report["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["bomRef"] == "pkg:npm/react@19.2.3")
        .unwrap();
    assert_eq!(react["detectedLicense"], "not-evaluated");
    assert_eq!(react["version"], "19.2.3");
    let debug = report["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["bomRef"] == "pkg:npm/debug@2.6.9")
        .unwrap();
    assert!(debug["declaredLicense"].is_null());
    let observations = debug["metadataObservations"].as_array().unwrap();
    assert!(observations.iter().any(|observation| {
        observation["declaredLicense"] == "LicenseRef-OutOfScope"
            && observation["installPaths"]
                == serde_json::json!(["frontend/node_modules/compression/node_modules/debug"])
    }));
    assert!(
        observations
            .iter()
            .any(|observation| observation["declaredLicense"] == "MIT")
    );
    assert!(
        observations.iter().all(|observation| {
            observation["provenance"]["frontend.manifestSha256"].is_string()
        })
    );
    assert_eq!(report["licenseReviews"], serde_json::json!([]));
    assert_eq!(workspace_files(&workspace), before);
}

#[test]
fn supply_chain_inventory_does_not_require_source_admission_lists() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("unreviewed-source-reader");
    create_workspace(&workspace, "unreviewed-source-reader");
    let policy_path = workspace.join(".yydra/supply-chain-policy.json");
    let mut policy: serde_json::Value =
        serde_json::from_slice(&fs::read(&policy_path).unwrap()).unwrap();
    policy["allowedSources"]["cargo"] = serde_json::json!([]);
    policy["allowedSources"]["npmHosts"] = serde_json::json!([]);
    fs::write(&policy_path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();
    let before = workspace_files(&workspace);
    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["supply-chain.dependencies"],
    );
    assert!(
        output.status.success(),
        "source admission is out of scope: {:?}",
        events(&output)
            .into_iter()
            .filter(|event| event["outcome"] == "fail")
            .collect::<Vec<_>>()
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[test]
fn supply_chain_dependency_inventory_retains_exact_graphs_without_license_admission() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("supply-chain-reader");
    create_workspace(&workspace, "supply-chain-reader");
    let before = workspace_files(&workspace);

    let passing_evidence = sandbox.path().join("passing-evidence");
    let passing = check(
        &workspace,
        &passing_evidence,
        &["supply-chain.dependencies"],
    );
    let passing_failures = events(&passing)
        .into_iter()
        .filter(|event| event["outcome"] == "fail")
        .collect::<Vec<_>>();
    assert!(
        passing.status.success(),
        "stderr: {}\nfailed nodes: {:?}",
        String::from_utf8_lossy(&passing.stderr),
        passing_failures
    );
    let parsed = events(&passing);
    let result = node(&parsed, "supply-chain.dependencies");
    assert_eq!(result["outcome"], "pass");
    assert_eq!(result["attempts"].as_array().map(Vec::len), Some(1));
    assert!(
        result["doesNotProve"]
            .as_str()
            .is_some_and(|boundary| boundary.contains("legal compatibility"))
    );
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(passing_evidence.join("artifacts/supply-chain.dependencies/inventory.json"))
            .expect("read dependency inventory"),
    )
    .expect("parse dependency inventory");
    assert_eq!(report["schemaVersion"], 2);
    assert_eq!(report["distributionVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["status"], "pass");
    let components = report["components"]
        .as_array()
        .expect("dependency components");
    assert_eq!(
        report["notEvaluated"],
        serde_json::json!([
            "license-review",
            "dependency-source-trust",
            "upstream-provenance",
            "notice-completeness"
        ])
    );
    assert_eq!(report["licenseReviews"], serde_json::json!([]));
    assert!(components.iter().any(|component| component["bomRef"]
        == "pkg:cargo/regex-syntax@0.8.11"
        && component["detectedLicense"] == "not-evaluated"
        && component["exposure"] == "shipped-linked"));
    assert_eq!(
        report["coverage"],
        serde_json::json!([
            "versions",
            "features",
            "transitives",
            "dependency-kinds",
            "targets",
            "build-tool-exposure"
        ])
    );
    assert!(
        report["components"]
            .as_array()
            .is_some_and(|components| components.iter().any(|component| {
                component["ecosystem"] == "cargo"
                    && component["name"] == "supply-chain-reader-domain"
                    && component["version"] == "0.1.0"
                    && component["features"].is_array()
                    && component["dependencyKinds"].is_array()
                    && component["targets"].is_array()
                    && component["source"] == "workspace-path"
                    && component["declaredLicense"] == "MIT OR Apache-2.0"
                    && component["detectedLicense"] == "not-evaluated"
                    && component["provenance"].is_object()
            }))
    );
    assert!(
        report["targetComponents"]
            .as_array()
            .is_some_and(|components| !components.is_empty())
    );
    for target in ["cli", "server", "h5", "android"] {
        let sbom: serde_json::Value = serde_json::from_slice(
            &fs::read(passing_evidence.join(format!(
                "artifacts/supply-chain.dependencies/{target}.cdx.json"
            )))
            .unwrap_or_else(|error| panic!("read {target} SBOM: {error}")),
        )
        .unwrap_or_else(|error| panic!("parse {target} SBOM: {error}"));
        assert_eq!(sbom["bomFormat"], "CycloneDX");
        assert_eq!(sbom["specVersion"], "1.6");
        assert_eq!(sbom["metadata"]["component"]["name"], target);
        assert!(sbom["components"].is_array());
        assert!(sbom["dependencies"].is_array());
    }
    assert_eq!(workspace_files(&workspace), before);

    let bolts_source =
        workspace.join("frontend/modules/yydra-bolts-tasks/vendor/src/main/java/bolts/Task.java");
    let bolts_original = fs::read_to_string(&bolts_source).unwrap();
    fs::write(
        &bolts_source,
        format!("{bolts_original}\n// changed source\n"),
    )
    .unwrap();
    let drifted_inputs = workspace_files(&workspace);
    let drifted = check(
        &workspace,
        &sandbox.path().join("bolts-source-drift"),
        &["supply-chain.dependencies"],
    );
    assert!(
        drifted.status.success(),
        "upstream source comparison is out of scope: {:?}",
        node(&events(&drifted), "supply-chain.dependencies")
    );
    assert_eq!(
        node(&events(&drifted), "supply-chain.dependencies")["outcome"],
        "pass"
    );
    let drifted_inventory: serde_json::Value = serde_json::from_slice(
        &fs::read(
            sandbox
                .path()
                .join("bolts-source-drift/artifacts/supply-chain.dependencies/inventory.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let bolts = drifted_inventory["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == "BoltsFramework/Bolts-Android/bolts-tasks")
        .unwrap();
    assert_eq!(
        bolts["provenance"]["identityBasis"],
        "distribution-declaration"
    );
    assert_eq!(bolts["provenance"]["localSourceIdentity"], "not-evaluated");
    assert_eq!(bolts["detectedLicense"], "not-evaluated");
    assert!(bolts.get("sourceChecksum").is_none());
    assert_eq!(workspace_files(&workspace), drifted_inputs);
    fs::write(&bolts_source, bolts_original).unwrap();

    let manifest = workspace.join("Cargo.toml");
    let source = fs::read_to_string(&manifest).expect("read Workspace manifest");
    let prohibited = source.replacen(
        "license = \"MIT OR Apache-2.0\"",
        "license = \"GPL-3.0-only\"",
        1,
    );
    assert_ne!(prohibited, source, "license fixture must change");
    fs::write(&manifest, &prohibited).expect("write prohibited dependency fixture");
    let prohibited_inputs = workspace_files(&workspace);
    let failing = check(
        &workspace,
        &sandbox.path().join("prohibited-evidence"),
        &["supply-chain.dependencies"],
    );
    assert!(
        failing.status.success(),
        "license admission is out of scope: {:?}",
        node(&events(&failing), "supply-chain.dependencies")
    );
    assert_eq!(
        node(&events(&failing), "supply-chain.dependencies")["outcome"],
        "pass"
    );
    assert_eq!(workspace_files(&workspace), prohibited_inputs);
}

#[test]
fn supply_chain_policy_rejects_stale_vulnerability_exceptions_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("stale-exception-reader");
    create_workspace(&workspace, "stale-exception-reader");
    let path = workspace.join(".yydra/supply-chain-exceptions.json");
    let mut exceptions: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read exceptions"))
            .expect("parse exceptions");
    exceptions["vulnerabilityExceptions"] = serde_json::json!([{
        "advisoryId": "OSV-TEST-STALE",
        "purl": "pkg:npm/react@19.2.3",
        "version": "19.2.3",
        "target": "h5",
        "impactAnalysis": "Historical fixture",
        "owner": "Yydra release owner",
        "evidence": "https://github.com/yydcnjjw/yydra/issues/39",
        "approvedBy": "yydcnjjw",
        "approvedAt": "2025-01-01T00:00:00Z",
        "expiresAt": "2026-01-01T00:00:00Z",
        "reReviewTrigger": "Any advisory, version, target, or artifact change"
    }]);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&exceptions).expect("encode stale exception"),
    )
    .expect("write stale exception fixture");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["supply-chain.policy"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "supply-chain.policy");
    assert_eq!(failure["outcome"], "fail");
    assert_eq!(failure["cause"]["code"], "SUPPLY_CHAIN_EXCEPTION_STALE");
    assert_eq!(failure["attempts"].as_array().map(Vec::len), Some(1));
    assert_eq!(workspace_files(&workspace), before);
}

#[test]
fn supply_chain_policy_ignores_historical_license_review_expiry_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("stale-license-review-reader");
    create_workspace(&workspace, "stale-license-review-reader");
    let path = workspace.join(".yydra/supply-chain-exceptions.json");
    let mut exceptions: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read exceptions"))
            .expect("parse exceptions");
    let review = &mut exceptions["licenseReviews"][0];
    review["approvedAt"] = serde_json::json!("2025-01-01T00:00:00Z");
    review["expiresAt"] = serde_json::json!("2026-01-01T00:00:00Z");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&exceptions).expect("encode stale license review"),
    )
    .expect("write stale license review fixture");
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["supply-chain.policy"],
    );
    assert!(
        output.status.success(),
        "license review expiry is outside the current scope: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    assert_eq!(node(&parsed, "supply-chain.policy")["outcome"], "pass");
    assert_eq!(workspace_files(&workspace), before);
}

#[test]
fn supply_chain_dependencies_reject_external_cargo_path_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("external-path-reader");
    create_workspace(&workspace, "external-path-reader");
    let external = sandbox.path().join("external-crate");
    fs::create_dir_all(external.join("src")).expect("create external crate");
    fs::write(
        external.join("Cargo.toml"),
        "[package]\nname = \"external-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\nlicense = \"MIT\"\n",
    )
    .expect("write external manifest");
    fs::write(external.join("src/lib.rs"), "pub fn external() {}\n")
        .expect("write external source");
    fs::write(
        external.join("LICENSE"),
        "Permission is hereby granted, free of charge, to any person obtaining a copy.\n",
    )
    .expect("write external license");

    let domain_manifest = workspace.join("crates/domain/Cargo.toml");
    let mut domain = fs::read_to_string(&domain_manifest).expect("read domain manifest");
    domain.push_str(&format!(
        "\n[dependencies.external-crate]\npath = {:?}\n",
        external.display().to_string()
    ));
    fs::write(&domain_manifest, domain).expect("add external path dependency");
    let lock = Command::new("cargo")
        .args(["generate-lockfile", "--offline"])
        .current_dir(&workspace)
        .output()
        .expect("refresh fixture lock");
    assert!(
        lock.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&lock.stderr)
    );
    let before = workspace_files(&workspace);

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["supply-chain.dependencies"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    let failure = node(&parsed, "supply-chain.dependencies");
    assert_eq!(failure["cause"]["code"], "SUPPLY_CHAIN_SOURCE_PROHIBITED");
    assert_eq!(workspace_files(&workspace), before);
}

#[cfg(unix)]
#[test]
fn supply_chain_dependencies_reject_version_drift_but_do_not_require_license_notices() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("missing-material-reader");
    create_workspace(&workspace, "missing-material-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
    );

    write_supply_chain_npm_fixture(&fake_bin, "provenance");
    let provenance = check_with_path(
        &workspace,
        &sandbox.path().join("provenance-evidence"),
        &["supply-chain.dependencies"],
        Some(&path),
    );
    assert!(!provenance.status.success());
    assert_eq!(
        node(&events(&provenance), "supply-chain.dependencies")["cause"]["code"],
        "SUPPLY_CHAIN_PROVENANCE_MISMATCH"
    );

    write_supply_chain_npm_fixture(&fake_bin, "notice");
    let notice = check_with_path(
        &workspace,
        &sandbox.path().join("notice-evidence"),
        &["supply-chain.dependencies"],
        Some(&path),
    );
    assert!(
        notice.status.success(),
        "notice completeness is out of scope: {:?}",
        node(&events(&notice), "supply-chain.dependencies")
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[cfg(unix)]
#[test]
fn supply_chain_advisories_are_exact_fail_closed_and_never_retried() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("advisory-reader");
    create_workspace(&workspace, "advisory-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").expect("PATH")
    );

    write_fake_osv_node(&fake_bin, "clean");
    let clean_evidence = sandbox.path().join("clean-evidence");
    let clean = check_with_path(
        &workspace,
        &clean_evidence,
        &["supply-chain.advisories"],
        Some(&path),
    );
    assert!(
        clean.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&clean.stderr),
        String::from_utf8_lossy(&clean.stdout)
    );
    let clean_events = events(&clean);
    assert_eq!(
        node(&clean_events, "supply-chain.dependencies")["outcome"],
        "pass"
    );
    let clean_node = node(&clean_events, "supply-chain.advisories");
    assert_eq!(clean_node["outcome"], "pass");
    assert_eq!(clean_node["attempts"].as_array().map(Vec::len), Some(1));
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(clean_evidence.join("artifacts/supply-chain.advisories/report.json"))
            .expect("read advisory report"),
    )
    .expect("parse advisory report");
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["status"], "pass");
    assert!(report["queries"].as_u64().is_some_and(|count| count > 0));
    assert_eq!(report["vulnerabilities"], 0);
    let bolts_query = report["queryIdentities"]
        .as_array()
        .expect("reported query identities")
        .iter()
        .find(|query| query["ecosystem"] == "GIT")
        .expect("declared Bolts query");
    assert_eq!(bolts_query["identityBasis"], "distribution-declaration");
    assert_eq!(bolts_query["localSourceIdentity"], "not-evaluated");
    assert_eq!(
        bolts_query["version"],
        "5465bcc3bbea3350dbb2affb4511a5726efb321e"
    );
    assert!(
        report["reportBoundary"]
            .as_str()
            .unwrap()
            .contains("not a verified local-source identity")
    );
    let osv_script =
        fs::read_to_string(clean_evidence.join("artifacts/supply-chain.advisories/osv-query.mjs"))
            .expect("read generated OSV evidence script");
    assert!(osv_script.contains("/v1/vulns/${encodeURIComponent(compact.id)}"));
    assert!(osv_script.contains("`${responsePath}.batch`"));
    assert!(osv_script.contains("detail.modified !== compact.modified"));

    write_fake_osv_node(&fake_bin, "informational");
    let informational_evidence = sandbox.path().join("informational-evidence");
    let informational = check_with_path(
        &workspace,
        &informational_evidence,
        &["supply-chain.advisories"],
        Some(&path),
    );
    assert!(
        informational.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&informational.stderr),
        String::from_utf8_lossy(&informational.stdout)
    );
    let informational_report: serde_json::Value = serde_json::from_slice(
        &fs::read(informational_evidence.join("artifacts/supply-chain.advisories/report.json"))
            .expect("read informational advisory report"),
    )
    .expect("parse informational advisory report");
    assert_eq!(informational_report["status"], "pass");
    assert_eq!(informational_report["vulnerabilities"], 0);
    assert_eq!(informational_report["informationalAdvisories"], 1);
    assert_eq!(
        informational_report["findings"][0]["classification"],
        "informational"
    );

    write_fake_osv_node(&fake_bin, "vulnerable");
    let vulnerable = check_with_path(
        &workspace,
        &sandbox.path().join("vulnerable-evidence"),
        &["supply-chain.advisories"],
        Some(&path),
    );
    assert!(!vulnerable.status.success());
    let vulnerable_events = events(&vulnerable);
    let vulnerable_node = node(&vulnerable_events, "supply-chain.advisories");
    assert_eq!(vulnerable_node["outcome"], "fail");
    assert_eq!(
        vulnerable_node["cause"]["code"],
        "SUPPLY_CHAIN_VULNERABILITY_FOUND"
    );
    assert_eq!(
        vulnerable_node["attempts"].as_array().map(Vec::len),
        Some(1)
    );

    let exception_path = workspace.join(".yydra/supply-chain-exceptions.json");
    let original_exceptions = fs::read(&exception_path).expect("read exact exceptions");
    let mut exceptions: serde_json::Value =
        serde_json::from_slice(&original_exceptions).expect("parse exact exceptions");
    exceptions["vulnerabilityExceptions"] = serde_json::json!([
        {
            "advisoryId": "OSV-TEST-REACT",
            "purl": "pkg:npm/react@19.2.3",
            "version": "19.2.3",
            "target": "android",
            "impactAnalysis": "Exact test advisory does not affect this retained fixture artifact.",
            "owner": "Yydra release owner",
            "evidence": "https://github.com/yydcnjjw/yydra/issues/39",
            "approvedBy": "yydcnjjw",
            "approvedAt": "2026-09-03T00:00:00Z",
            "expiresAt": "2999-01-01T00:00:00Z",
            "reReviewTrigger": "Any advisory, version, target, or artifact change"
        },
        {
            "advisoryId": "OSV-TEST-REACT",
            "purl": "pkg:npm/react@19.2.3",
            "version": "19.2.3",
            "target": "h5",
            "impactAnalysis": "Exact test advisory does not affect this retained fixture artifact.",
            "owner": "Yydra release owner",
            "evidence": "https://github.com/yydcnjjw/yydra/issues/39",
            "approvedBy": "yydcnjjw",
            "approvedAt": "2026-09-03T00:00:00Z",
            "expiresAt": "2999-01-01T00:00:00Z",
            "reReviewTrigger": "Any advisory, version, target, or artifact change"
        }
    ]);
    fs::write(
        &exception_path,
        serde_json::to_vec_pretty(&exceptions).expect("encode exact exceptions"),
    )
    .expect("write exact exception fixture");
    let excepted_evidence = sandbox.path().join("excepted-evidence");
    let excepted = check_with_path(
        &workspace,
        &excepted_evidence,
        &["supply-chain.advisories"],
        Some(&path),
    );
    assert!(
        excepted.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&excepted.stderr),
        String::from_utf8_lossy(&excepted.stdout)
    );
    let excepted_report: serde_json::Value = serde_json::from_slice(
        &fs::read(excepted_evidence.join("artifacts/supply-chain.advisories/report.json"))
            .expect("read excepted report"),
    )
    .expect("parse excepted report");
    assert_eq!(excepted_report["status"], "pass");
    assert_eq!(excepted_report["vulnerabilities"], 1);
    assert_eq!(excepted_report["exceptionsApplied"], 2);

    write_fake_osv_node(&fake_bin, "outage");
    let outage = check_with_path(
        &workspace,
        &sandbox.path().join("outage-evidence"),
        &["supply-chain.advisories"],
        Some(&path),
    );
    assert!(!outage.status.success());
    let outage_events = events(&outage);
    let outage_node = node(&outage_events, "supply-chain.advisories");
    assert_eq!(outage_node["outcome"], "infrastructure-error");
    assert_eq!(
        outage_node["cause"]["code"],
        "SUPPLY_CHAIN_ADVISORY_SERVICE_UNAVAILABLE"
    );
    assert_eq!(outage_node["attempts"].as_array().map(Vec::len), Some(1));
    fs::write(&exception_path, original_exceptions).expect("restore exact exceptions");
    assert_eq!(workspace_files(&workspace), before);
}

#[test]
fn server_release_builds_the_exact_locked_product_binary_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("server-release-reader");
    create_workspace(&workspace, "server-release-reader");
    let before = workspace_files(&workspace);
    let evidence = sandbox.path().join("evidence");

    let output = check(&workspace, &evidence, &["server.release"]);
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    let release = node(&parsed, "server.release");
    assert_eq!(release["outcome"], "pass");
    assert!(
        release["commands"]
            .as_array()
            .expect("server release commands")
            .iter()
            .any(|command| command.as_str().is_some_and(|command| {
                command.contains(
                    "cargo build --locked --release --package server-release-reader-server --bin server",
                )
            }))
    );
    let binary_name = if cfg!(windows) {
        "server.exe"
    } else {
        "server"
    };
    let binary = evidence.join("artifacts/server.release").join(binary_name);
    assert!(binary.is_file(), "server release binary was not retained");
    let identity: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/server.release/artifact.json"))
            .expect("read server identity"),
    )
    .expect("parse server identity");
    assert_eq!(identity["path"], binary_name);
    assert_eq!(identity["package"], "server-release-reader-server");
    assert_eq!(identity["binary"], "server");
    assert!(identity["bytes"].as_u64().is_some_and(|bytes| bytes > 0));
    assert!(
        identity["sha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
    assert_eq!(workspace_files(&workspace), before);
}

#[test]
#[ignore = "requires OSV, Docker, PostgreSQL image, Node/npm, Playwright Chromium, Android SDK, and Gradle"]
fn supply_chain_release_artifacts_cover_exact_built_targets_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("release-evidence-reader");
    create_workspace(&workspace, "release-evidence-reader");
    let before = workspace_files(&workspace);
    let evidence = sandbox.path().join("evidence");

    let output = check(&workspace, &evidence, &["supply-chain.release-artifacts"]);
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    for node_id in [
        "supply-chain.dependencies",
        "supply-chain.advisories",
        "server.release",
        "android.release",
        "h5.real-runtime",
        "supply-chain.release-artifacts",
        "ownership.authored-inputs-unchanged",
    ] {
        assert_eq!(node(&parsed, node_id)["outcome"], "pass", "{node_id}");
    }
    let release_root = evidence.join("artifacts/supply-chain.release-artifacts");
    for target in ["cli", "server", "h5", "android"] {
        let target_root = release_root.join(target);
        for name in [
            "sbom.cdx.json",
            "THIRD-PARTY-NOTICES.txt",
            "SHA256SUMS",
            "artifact-inventory.json",
            "provenance.json",
        ] {
            assert!(target_root.join(name).is_file(), "missing {target}/{name}");
        }
        let sbom: serde_json::Value = serde_json::from_slice(
            &fs::read(target_root.join("sbom.cdx.json")).expect("read final SBOM"),
        )
        .expect("parse final SBOM");
        assert_eq!(sbom["bomFormat"], "CycloneDX");
        assert_eq!(sbom["specVersion"], "1.6");
        assert_eq!(sbom["metadata"]["component"]["name"], target);
        assert!(
            !sbom["components"]
                .as_array()
                .expect("SBOM components")
                .iter()
                .any(|component| component["purl"] == "pkg:npm/lightningcss@1.33.0")
        );
        let inventory: serde_json::Value = serde_json::from_slice(
            &fs::read(target_root.join("artifact-inventory.json"))
                .expect("read artifact inventory"),
        )
        .expect("parse artifact inventory");
        assert_eq!(inventory["schemaVersion"], 4);
        assert_eq!(
            inventory["notEvaluated"],
            serde_json::json!([
                "license-review",
                "dependency-source-trust",
                "upstream-provenance",
                "notice-completeness"
            ])
        );
        assert!(inventory.get("nativeBindings").is_none());
        assert!(inventory.get("localGradleBindings").is_none());
        if matches!(target, "h5" | "android") {
            assert!(
                inventory["sourceMaps"]
                    .as_array()
                    .is_some_and(|maps| !maps.is_empty())
            );
            assert!(
                inventory["sourceMapBindings"]
                    .as_array()
                    .is_some_and(|bindings| {
                        !bindings.is_empty()
                            && bindings.iter().all(|binding| {
                                binding["bundleSha256"].as_str().is_some_and(|digest| {
                                    digest.starts_with("sha256:") && digest.len() == 71
                                }) && binding["sourceMapSha256"].as_str().is_some_and(|digest| {
                                    digest.starts_with("sha256:") && digest.len() == 71
                                })
                            })
                    })
            );
            if target == "android" {
                let packaged_bundle = inventory["archiveEntries"]
                    .as_array()
                    .expect("Android archive entries")
                    .iter()
                    .find(|entry| entry["path"] == "assets/index.android.bundle")
                    .expect("packaged Android JavaScript bundle");
                assert!(
                    packaged_bundle["sha256"]
                        .as_str()
                        .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
                );
            }
            assert!(
                inventory["artifactLinkedNpmComponents"]
                    .as_array()
                    .is_some_and(|components| !components.is_empty())
            );
            assert!(
                sbom["components"]
                    .as_array()
                    .expect("SBOM components")
                    .iter()
                    .filter(|component| {
                        component["purl"]
                            .as_str()
                            .is_some_and(|purl| purl.starts_with("pkg:npm/"))
                    })
                    .all(|component| component["properties"].as_array().is_some_and(
                        |properties| properties.iter().any(|property| {
                            property["name"] == "yydra:artifact-link-authority"
                                && property["value"] == "retained-production-source-map"
                        }) && properties.iter().any(|property| {
                            property["name"] == "yydra:exposure"
                                && (property["value"] == "shipped-linked"
                                    || property["value"] == "product-root")
                        })
                    ))
            );
        } else {
            assert_eq!(inventory["sourceMaps"], serde_json::json!([]));
            assert_eq!(inventory["sourceMapBindings"], serde_json::json!([]));
            assert_eq!(
                inventory["artifactLinkedNpmComponents"],
                serde_json::json!([])
            );
        }
        let provenance: serde_json::Value = serde_json::from_slice(
            &fs::read(target_root.join("provenance.json")).expect("read provenance"),
        )
        .expect("parse provenance");
        assert_eq!(provenance["status"], "pass");
        assert_eq!(provenance["target"], target);
        assert_eq!(provenance["notEvaluated"], inventory["notEvaluated"]);
        assert!(provenance["dependencyInventory"]["sha256"].is_string());
        assert!(provenance["advisoryReport"]["sha256"].is_string());
        if target == "android" {
            assert!(provenance["androidMavenAdvisoryReport"]["sha256"].is_string());
            assert_eq!(
                provenance["androidMavenAdvisoryReport"]["path"],
                "artifacts/supply-chain.android-advisories/report.json"
            );
        } else {
            assert!(provenance.get("androidMavenAdvisoryReport").is_none());
        }
    }
    let android_advisory: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/supply-chain.android-advisories/report.json"))
            .expect("read Android Maven advisory report"),
    )
    .expect("parse Android Maven advisory report");
    assert_eq!(android_advisory["status"], "pass");
    assert!(
        android_advisory["queries"]
            .as_u64()
            .is_some_and(|queries| queries > 0)
    );
    let android_sbom: serde_json::Value = serde_json::from_slice(
        &fs::read(release_root.join("android/sbom.cdx.json")).expect("read Android SBOM"),
    )
    .expect("parse Android SBOM");
    assert!(
        android_sbom["components"]
            .as_array()
            .expect("Android components")
            .iter()
            .any(|component| component["type"] == "file"
                && component["properties"]
                    .as_array()
                    .is_some_and(|properties| {
                        properties.iter().any(|property| {
                            property["name"] == "yydra:apk-native-entry"
                                && property["value"]
                                    .as_str()
                                    .is_some_and(|value| value.ends_with(".so"))
                        })
                    }))
    );
    let gradle_inventory: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/android.release/gradle-materials.json"))
            .expect("read Android Gradle material inventory"),
    )
    .expect("parse Android Gradle material inventory");
    assert_eq!(gradle_inventory["schemaVersion"], 4);
    let local_components = gradle_inventory["localComponents"]
        .as_array()
        .expect("selected local Android components");
    assert!(!local_components.is_empty());
    assert!(
        local_components.iter().all(|component| {
            component["exposure"] == "runtime-build-input"
                && component["artifacts"]
                    .as_array()
                    .is_some_and(|artifacts| !artifacts.is_empty())
        }),
        "selected local runtime projects must retain their actual build-input AARs"
    );
    assert_eq!(gradle_inventory["repositories"], serde_json::json!([]));
    assert_eq!(
        gradle_inventory["bundleBinding"]["taskPath"],
        ":app:createBundleReleaseJsAndAssets"
    );
    assert_eq!(
        gradle_inventory["bundleBinding"]["bundlePath"],
        "app/build/generated/assets/react/release/index.android.bundle"
    );
    assert!(
        gradle_inventory["bundleBinding"]["bundleSha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
    assert!(
        gradle_inventory["components"]
            .as_array()
            .expect("Gradle components")
            .iter()
            .all(|component| component["detectedLicense"] == "not-evaluated"
                && component["provenance"]
                    .get("configuredRepositories")
                    .is_none()
                && component["notices"] == serde_json::json!([]))
    );
    assert_eq!(workspace_files(&workspace), before);
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
    assert!(
        node(&events(&passing), "runtime.post-commit-executor")["commands"]
            .as_array()
            .expect("post-commit commands")
            .iter()
            .any(|command| command
                .as_str()
                .is_some_and(|command| command.contains("--test-threads=1"))),
        "the canonical multi-test target must not race process-global test instrumentation"
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
    assert_eq!(result["attempts"].as_array().map(Vec::len), Some(1));
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
    assert_eq!(manifest["fixture"], "unclassified");
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
        manifest["catalogDigest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:"))
    );
    assert!(
        manifest["executorDigest"]
            .as_str()
            .is_some_and(|digest| digest.len() == 71 && digest.starts_with("sha256:"))
    );
    assert!(
        manifest["diagnosticVocabulary"]
            .as_array()
            .is_some_and(|codes| { codes.iter().any(|code| code == "CHECK_PREREQUISITE_FAILED") })
    );
    assert_eq!(manifest["exceptionPolicy"]["mode"], "deny-all");
    assert_eq!(manifest["retryPolicy"]["semanticMaxAttempts"], 1);
    assert_eq!(
        manifest["retryPolicy"]["infrastructureEstablishmentMaxAttempts"],
        2
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
    let catalog: serde_json::Value = serde_json::from_slice(
        &fs::read(human_evidence.join("artifacts/check-catalog.json")).expect("read exact catalog"),
    )
    .expect("parse exact catalog");
    assert_eq!(
        catalog["fixtureDefinitions"],
        serde_json::json!([
            {
                "id": "clean",
                "productName": "Clean Product",
                "productId": "clean-product",
                "productSourceLicense": "Apache-2.0"
            },
            {
                "id": "reading-queue",
                "productName": "Reading Queue",
                "productId": "reading-queue",
                "productSourceLicense": "Apache-2.0"
            }
        ])
    );
    assert_eq!(node(&json_events, "rust.compile")["outcome"], "not-run");
    assert_eq!(
        node(&json_events, "rust.compile")["cause"]["code"],
        "CHECK_NOT_SELECTED"
    );
}

#[test]
fn unknown_exception_configuration_fails_closed_without_masking_independent_nodes() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("exception-reader");
    create_workspace(&workspace, "exception-reader");
    let exception = workspace.join(".yydra/check-exceptions.toml");
    fs::write(
        &exception,
        "rule = \"frontend.lint\"\nexpires = \"2099-01-01\"\n",
    )
    .expect("write unsupported exception fixture");
    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["policy.exceptions", "origin.exact-distribution"],
    );
    assert!(!output.status.success());
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "policy.exceptions")["cause"]["code"],
        "CHECK_EXCEPTION_POLICY_VIOLATION"
    );
    assert_eq!(
        node(&parsed, "origin.exact-distribution")["outcome"],
        "pass"
    );
}

#[cfg(unix)]
#[test]
fn infrastructure_establishment_retries_once_and_records_both_attempts() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("infrastructure-retry-reader");
    create_workspace(&workspace, "infrastructure-retry-reader");
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    let counter = sandbox.path().join("docker-attempts");
    write_executable(
        &fake_bin.join("docker"),
        &format!(
            r#"#!/bin/sh
if [ -f '{counter}' ]; then
  printf '%s\n' '27.5.1'
  exit 0
fi
: > '{counter}'
exit 17
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
            "infrastructure.docker",
        ])
        .env("PATH", path)
        .output()
        .expect("run retryable infrastructure check");
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    let docker = node(&parsed, "infrastructure.docker");
    assert_eq!(docker["outcome"], "pass");
    assert_eq!(docker["attempts"].as_array().map(Vec::len), Some(2));
    assert_eq!(docker["attempts"][0]["outcome"], "infrastructure-error");
    assert_eq!(docker["attempts"][1]["outcome"], "pass");
    assert_eq!(docker["commands"].as_array().map(Vec::len), Some(2));
}

fn artifact_digest(path: &Path) -> String {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, (u8, Vec<u8>)>) {
        let metadata = fs::symlink_metadata(path).expect("inspect aggregate fixture artifact");
        let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        if metadata.file_type().is_symlink() {
            entries.insert(
                relative,
                (
                    3,
                    fs::read_link(path)
                        .expect("read aggregate fixture symlink")
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec(),
                ),
            );
        } else if metadata.is_file() {
            entries.insert(
                relative,
                (2, fs::read(path).expect("read aggregate fixture file")),
            );
        } else if metadata.is_dir() {
            if path != root {
                entries.insert(relative, (1, Vec::new()));
            }
            let mut children = fs::read_dir(path)
                .expect("read aggregate fixture directory")
                .collect::<std::io::Result<Vec<_>>>()
                .expect("collect aggregate fixture directory");
            children.sort_by_key(std::fs::DirEntry::file_name);
            for child in children {
                visit(root, &child.path(), entries);
            }
        }
    }

    let mut entries = BTreeMap::new();
    visit(path, path, &mut entries);
    let mut digest = sha2::Sha256::new();
    for (path, (kind, bytes)) in entries {
        digest.update(path.as_os_str().as_encoded_bytes());
        digest.update([0]);
        digest.update([kind]);
        digest.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        digest.update(bytes);
    }
    format!("sha256:{}", hex::encode(digest.finalize()))
}

fn complete_aggregate_fixture(sandbox: &Path, fixture: &str) -> PathBuf {
    let workspace = sandbox.join(format!("{fixture}-workspace"));
    let (product_name, product_id) = match fixture {
        "clean" => ("Clean Product", "clean-product"),
        "reading-queue" => ("Reading Queue", "reading-queue"),
        other => panic!("unsupported aggregate fixture {other}"),
    };
    create_workspace_with_inputs(&workspace, product_name, product_id, "Apache-2.0");
    let evidence = sandbox.join(format!("{fixture}-evidence"));
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence"),
            "--fixture",
            fixture,
            "--node",
            "origin.exact-distribution",
        ])
        .output()
        .expect("create source aggregate evidence");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest_path = evidence.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read source aggregate manifest"))
            .expect("parse source aggregate manifest");
    manifest["fixture"] = fixture.into();
    manifest["selectedNodes"] = serde_json::json!([]);
    manifest["complete"] = true.into();
    manifest["status"] = "pass-core".into();
    for node in manifest["nodes"]
        .as_array_mut()
        .expect("source aggregate nodes")
    {
        node["outcome"] = "pass".into();
        node["durationMs"] = 1.into();
        node["attempts"] = serde_json::json!([{
            "attempt": 1,
            "outcome": "pass",
            "durationMs": 1,
            "cause": null
        }]);
        node["cause"] = serde_json::Value::Null;
        node["remediation"] = serde_json::Value::Null;
        node["commands"] = serde_json::json!([]);
        node["toolVersions"] = serde_json::json!({});
    }
    let required_tool_versions = manifest["requiredToolVersions"].clone();
    manifest["nodes"][0]["toolVersions"] = required_tool_versions.clone();
    manifest["observedToolVersions"] = required_tool_versions;
    let mut diagnostics = String::new();
    for node in manifest["nodes"]
        .as_array()
        .expect("source aggregate nodes")
    {
        diagnostics.push_str(&serde_json::to_string(node).expect("encode aggregate node"));
        diagnostics.push('\n');
    }
    diagnostics.push_str(
        &serde_json::to_string(&serde_json::json!({
            "schemaVersion": 1,
            "event": "check-summary",
            "status": "pass-core",
            "scope": "clean-core-local",
            "complete": true,
            "aggregateConformance": false,
            "evidence": "manifest.json",
        }))
        .expect("encode aggregate source summary"),
    );
    diagnostics.push('\n');
    fs::write(evidence.join("diagnostics.jsonl"), diagnostics)
        .expect("write source aggregate diagnostics");
    for artifact in manifest["artifacts"]
        .as_array_mut()
        .expect("source aggregate artifacts")
    {
        let relative = artifact["path"].as_str().expect("artifact path");
        artifact["sha256"] = artifact_digest(&evidence.join(relative)).into();
    }
    let mut bytes = serde_json::to_vec_pretty(&manifest).expect("encode source aggregate manifest");
    bytes.push(b'\n');
    fs::write(&manifest_path, bytes).expect("write source aggregate manifest");
    manifest_path
}

#[test]
fn local_diagnostics_persist_the_exact_summary_before_manifest_digests() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("summary-reader");
    create_workspace(&workspace, "summary-reader");
    let evidence = sandbox.path().join("evidence");
    let output = check(&workspace, &evidence, &["origin.exact-distribution"]);
    assert!(output.status.success());
    assert_eq!(
        events(&output)
            .iter()
            .find(|event| event["event"] == "check-summary")
            .expect("stdout summary")["evidence"],
        evidence.join("manifest.json").display().to_string()
    );

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("manifest.json")).expect("read source manifest"),
    )
    .expect("parse source manifest");
    let lines = fs::read_to_string(evidence.join("diagnostics.jsonl"))
        .expect("read persisted JSON Lines")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("parse JSON Line"))
        .collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        manifest["nodes"].as_array().expect("manifest nodes").len() + 1
    );
    assert_eq!(
        lines.last().expect("summary line"),
        &serde_json::json!({
            "schemaVersion": 1,
            "event": "check-summary",
            "status": "pass-selected",
            "scope": "clean-core-local",
            "complete": false,
            "aggregateConformance": false,
            "evidence": "manifest.json"
        })
    );
    let diagnostics_artifact = manifest["artifacts"]
        .as_array()
        .expect("manifest artifacts")
        .iter()
        .find(|artifact| artifact["path"] == "diagnostics.jsonl")
        .expect("diagnostics artifact");
    assert_eq!(
        diagnostics_artifact["sha256"],
        artifact_digest(&evidence.join("diagnostics.jsonl"))
    );
}

#[test]
fn named_fixture_requires_its_exact_workspace_origin_inputs() {
    let sandbox = tempdir().expect("create sandbox");
    let impostor = sandbox.path().join("impostor");
    create_workspace(&impostor, "different-product");
    let rejected = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            impostor.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            sandbox
                .path()
                .join("impostor-evidence")
                .to_str()
                .expect("UTF-8 evidence"),
            "--fixture",
            "reading-queue",
            "--node",
            "origin.exact-distribution",
        ])
        .output()
        .expect("check mislabeled fixture");
    assert!(!rejected.status.success());
    assert_eq!(
        node(&events(&rejected), "origin.exact-distribution")["cause"]["code"],
        "FIXTURE_IDENTITY_MISMATCH"
    );

    let exact = sandbox.path().join("reading-queue");
    create_workspace_with_inputs(&exact, "Reading Queue", "reading-queue", "Apache-2.0");
    let accepted = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            exact.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            sandbox
                .path()
                .join("exact-evidence")
                .to_str()
                .expect("UTF-8 evidence"),
            "--fixture",
            "reading-queue",
            "--node",
            "origin.exact-distribution",
        ])
        .output()
        .expect("check exact fixture");
    assert!(
        accepted.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&accepted.stderr),
        String::from_utf8_lossy(&accepted.stdout)
    );
}

fn aggregate(evidence: &Path, manifests: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yydra"));
    command.args([
        "--message-format=json",
        "check",
        "--evidence-dir",
        evidence.to_str().expect("UTF-8 aggregate evidence"),
    ]);
    for manifest in manifests {
        command.args([
            "--aggregate-evidence",
            manifest.to_str().expect("UTF-8 source manifest"),
        ]);
    }
    command.output().expect("aggregate conformance evidence")
}

fn assert_aggregate_failure(output: &Output, code: &str) {
    assert!(
        !output.status.success(),
        "aggregate unexpectedly passed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(output);
    let result = parsed
        .iter()
        .find(|event| event["event"] == "check-aggregate")
        .expect("aggregate failure result");
    assert_eq!(result["outcome"], "fail");
    assert_eq!(result["cause"]["code"], code);
    let summary = parsed
        .iter()
        .find(|event| event["event"] == "check-summary")
        .expect("aggregate failure summary");
    assert_eq!(summary["complete"], false);
    assert_eq!(summary["aggregateConformance"], false);
}

#[test]
fn exact_complete_clean_and_reading_queue_evidence_produces_aggregate_conformance() {
    let sandbox = tempdir().expect("create sandbox");
    let clean = complete_aggregate_fixture(sandbox.path(), "clean");
    let reading = complete_aggregate_fixture(sandbox.path(), "reading-queue");
    let evidence = sandbox.path().join("aggregate-evidence");
    let output = aggregate(&evidence, &[&clean, &reading]);
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = events(&output);
    let result = parsed
        .iter()
        .find(|event| event["event"] == "check-aggregate")
        .expect("aggregate result event");
    assert_eq!(result["outcome"], "pass");
    let summary = parsed
        .iter()
        .find(|event| event["event"] == "check-summary")
        .expect("aggregate summary event");
    assert_eq!(summary["status"], "pass-aggregate");
    assert_eq!(summary["scope"], "clean-and-reading-queue");
    assert_eq!(summary["complete"], true);
    assert_eq!(summary["aggregateConformance"], true);
    let aggregate_manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("manifest.json")).expect("read aggregate manifest"),
    )
    .expect("parse aggregate manifest");
    assert_eq!(aggregate_manifest["catalogDigest"], {
        let source: serde_json::Value =
            serde_json::from_slice(&fs::read(&clean).expect("read clean source manifest"))
                .expect("parse clean source manifest");
        source["catalogDigest"].clone()
    });
    assert_eq!(
        aggregate_manifest["sources"].as_array().map(Vec::len),
        Some(2)
    );
}

#[test]
fn aggregate_does_not_emit_success_before_its_manifest_is_durable() {
    let sandbox = tempdir().expect("create sandbox");
    let clean = complete_aggregate_fixture(sandbox.path(), "clean");
    let reading = complete_aggregate_fixture(sandbox.path(), "reading-queue");
    let reading_root = reading.parent().expect("reading evidence root");
    let slow_artifact = reading_root.join("logs/slow-verification-fixture.bin");
    fs::File::create(&slow_artifact)
        .expect("create sparse verification fixture")
        .set_len(64 * 1024 * 1024)
        .expect("size sparse verification fixture");
    let mut reading_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&reading).expect("read reading manifest"))
            .expect("parse reading manifest");
    reading_manifest["artifacts"]
        .as_array_mut()
        .expect("manifest artifacts")
        .iter_mut()
        .find(|artifact| artifact["path"] == "logs")
        .expect("logs artifact")["sha256"] = artifact_digest(&reading_root.join("logs")).into();
    let mut reading_bytes =
        serde_json::to_vec_pretty(&reading_manifest).expect("encode reading manifest");
    reading_bytes.push(b'\n');
    fs::write(&reading, reading_bytes).expect("write reading manifest");

    let evidence = sandbox.path().join("unwritable-manifest-evidence");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "--message-format=json",
            "check",
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 aggregate evidence"),
            "--aggregate-evidence",
            clean.to_str().expect("UTF-8 clean manifest"),
            "--aggregate-evidence",
            reading.to_str().expect("UTF-8 reading manifest"),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("start aggregate verifier");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !evidence.join("diagnostics.jsonl").is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "aggregate verifier did not create its diagnostics in time"
        );
        assert!(
            child.try_wait().expect("poll aggregate verifier").is_none(),
            "aggregate verifier exited before the failure fixture was installed"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    fs::create_dir(evidence.join("manifest.json"))
        .expect("occupy aggregate manifest path before persistence");
    let output = child
        .wait_with_output()
        .expect("wait for aggregate verifier");
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 aggregate stdout");
    assert!(
        !stdout.contains("\"outcome\":\"pass\"")
            && !stdout.contains("\"status\":\"pass-aggregate\""),
        "aggregate emitted success before its manifest was durable: {stdout}"
    );
}

#[test]
fn github_quality_workflow_executes_the_distribution_graph_and_aggregates_uploaded_evidence() {
    let workflow_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/quality.yml");
    if !workflow_path.is_file() {
        return;
    }
    let workflow = fs::read_to_string(workflow_path).expect("read quality workflow");
    assert!(workflow.contains("fixture: [clean, reading-queue]"));
    assert!(workflow.contains("--fixture \"$FIXTURE\""));
    assert_eq!(workflow.matches("--aggregate-evidence").count(), 2);
    assert!(workflow.contains("name: quality-clean"));
    assert!(workflow.contains("name: quality-reading-queue"));
    assert!(workflow.contains("name: quality-aggregate"));
    assert_eq!(workflow.matches("name: quality-executor").count(), 3);
    assert!(workflow.contains("${{ runner.temp }}/executor/yydra"));
    assert!(workflow.contains("if: ${{ always() && needs.executor.result == 'success' }}"));
    assert_eq!(workflow.matches("include-hidden-files: true").count(), 2);
    assert_eq!(workflow.matches("continue-on-error: true").count(), 2);
    for mutable_action_tag in [
        "actions/checkout@v",
        "actions/setup-node@v",
        "actions/setup-java@v",
        "actions/upload-artifact@v",
        "actions/download-artifact@v",
    ] {
        assert!(
            !workflow.contains(mutable_action_tag),
            "quality workflow must pin {mutable_action_tag} by full commit SHA"
        );
    }
    for duplicate_semantic_authority in ["cargo test", "cargo clippy", "npm test", "gradlew"] {
        assert!(
            !workflow.contains(duplicate_semantic_authority),
            "CI must execute yydra check instead of duplicating {duplicate_semantic_authority}"
        );
    }
}

#[test]
fn aggregate_rejects_missing_malformed_mismatched_incomplete_and_tampered_evidence() {
    let sandbox = tempdir().expect("create sandbox");
    let clean = complete_aggregate_fixture(sandbox.path(), "clean");
    let reading = complete_aggregate_fixture(sandbox.path(), "reading-queue");

    let missing = aggregate(&sandbox.path().join("missing"), &[&clean]);
    assert_aggregate_failure(&missing, "AGGREGATE_FIXTURE_MISSING");

    let absent = sandbox.path().join("not-uploaded/manifest.json");
    let unuploaded = aggregate(&sandbox.path().join("unuploaded"), &[&clean, &absent]);
    assert_aggregate_failure(&unuploaded, "AGGREGATE_EVIDENCE_MISSING");

    let duplicate = aggregate(&sandbox.path().join("duplicate"), &[&clean, &clean]);
    assert_aggregate_failure(&duplicate, "AGGREGATE_DUPLICATE_FIXTURE");

    let malformed_path = sandbox.path().join("malformed.json");
    fs::write(&malformed_path, "{not-json\n").expect("write malformed manifest");
    let malformed = aggregate(
        &sandbox.path().join("malformed-result"),
        &[&clean, &malformed_path],
    );
    assert_aggregate_failure(&malformed, "AGGREGATE_EVIDENCE_MALFORMED");

    let original = fs::read(&reading).expect("read original reading manifest");
    let mut altered: serde_json::Value =
        serde_json::from_slice(&original).expect("parse reading manifest");
    altered["catalogDigest"] = "sha256:stale".into();
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&altered).expect("encode mismatched manifest"),
    )
    .expect("write mismatched manifest");
    let mismatched = aggregate(&sandbox.path().join("mismatched"), &[&clean, &reading]);
    assert_aggregate_failure(&mismatched, "AGGREGATE_CATALOG_MISMATCH");

    for outcome in ["fail", "skipped", "not-run"] {
        let mut incomplete: serde_json::Value =
            serde_json::from_slice(&original).expect("parse reading manifest");
        incomplete["nodes"][0]["outcome"] = outcome.into();
        fs::write(
            &reading,
            serde_json::to_vec_pretty(&incomplete).expect("encode incomplete manifest"),
        )
        .expect("write incomplete manifest");
        let incomplete_output = aggregate(
            &sandbox.path().join(format!("incomplete-{outcome}")),
            &[&clean, &reading],
        );
        assert_aggregate_failure(&incomplete_output, "AGGREGATE_EVIDENCE_INCOMPLETE");
    }

    let mut identity: serde_json::Value =
        serde_json::from_slice(&original).expect("parse reading manifest");
    identity["distributionVersion"] = "0.0.3".into();
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&identity).expect("encode identity mismatch"),
    )
    .expect("write identity mismatch");
    let identity_output = aggregate(&sandbox.path().join("identity"), &[&clean, &reading]);
    assert_aggregate_failure(&identity_output, "AGGREGATE_IDENTITY_MISMATCH");

    let mut executor_identity: serde_json::Value =
        serde_json::from_slice(&original).expect("parse reading manifest");
    executor_identity["executorDigest"] = format!("sha256:{}", "0".repeat(64)).into();
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&executor_identity).expect("encode executor identity mismatch"),
    )
    .expect("write executor identity mismatch");
    let executor_identity_output = aggregate(
        &sandbox.path().join("executor-identity"),
        &[&clean, &reading],
    );
    assert_aggregate_failure(&executor_identity_output, "AGGREGATE_IDENTITY_MISMATCH");

    let mut digest: serde_json::Value =
        serde_json::from_slice(&original).expect("parse reading manifest");
    digest["inputDigest"] = "sha256:not-a-complete-digest".into();
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&digest).expect("encode invalid input digest"),
    )
    .expect("write invalid input digest");
    let digest_output = aggregate(&sandbox.path().join("input-digest"), &[&clean, &reading]);
    assert_aggregate_failure(&digest_output, "AGGREGATE_IDENTITY_MISMATCH");

    let clean_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&clean).expect("read clean manifest"))
            .expect("parse clean manifest");
    let mut reused: serde_json::Value =
        serde_json::from_slice(&original).expect("parse reading manifest");
    reused["inputDigest"] = clean_manifest["inputDigest"].clone();
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&reused).expect("encode reused input identity"),
    )
    .expect("write reused input identity");
    let reused_output = aggregate(&sandbox.path().join("reused-input"), &[&clean, &reading]);
    assert_aggregate_failure(&reused_output, "AGGREGATE_IDENTITY_MISMATCH");

    let mut observed: serde_json::Value =
        serde_json::from_slice(&original).expect("parse reading manifest");
    observed["observedToolVersions"] = serde_json::json!({"invented": "1.0.0"});
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&observed).expect("encode observed identity mismatch"),
    )
    .expect("write observed identity mismatch");
    let observed_output = aggregate(
        &sandbox.path().join("observed-identity"),
        &[&clean, &reading],
    );
    assert_aggregate_failure(&observed_output, "AGGREGATE_IDENTITY_MISMATCH");

    let mut exception: serde_json::Value =
        serde_json::from_slice(&original).expect("parse reading manifest");
    exception["exceptions"] = serde_json::json!(["waive-required-node"]);
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&exception).expect("encode exception"),
    )
    .expect("write exception manifest");
    let exception_output = aggregate(&sandbox.path().join("exception"), &[&clean, &reading]);
    assert_aggregate_failure(&exception_output, "AGGREGATE_EXCEPTION_REJECTED");

    fs::write(&reading, &original).expect("restore reading manifest again");
    let diagnostics = reading
        .parent()
        .expect("reading evidence root")
        .join("diagnostics.jsonl");
    let original_diagnostics = fs::read(&diagnostics).expect("read source diagnostics");
    fs::write(&diagnostics, "{}\n").expect("corrupt source diagnostics");
    let diagnostics_output = aggregate(&sandbox.path().join("diagnostics"), &[&clean, &reading]);
    assert_aggregate_failure(&diagnostics_output, "AGGREGATE_DIAGNOSTICS_INVALID");
    let mut diagnostic_lines = String::from_utf8(original_diagnostics.clone())
        .expect("UTF-8 source diagnostics")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("parse diagnostic"))
        .collect::<Vec<_>>();
    diagnostic_lines.last_mut().expect("summary diagnostic")["unexpected"] = true.into();
    let mut encoded_diagnostics = diagnostic_lines
        .iter()
        .map(|line| serde_json::to_string(line).expect("encode diagnostic"))
        .collect::<Vec<_>>()
        .join("\n");
    encoded_diagnostics.push('\n');
    fs::write(&diagnostics, encoded_diagnostics).expect("write unknown summary field");
    let mut strict_manifest: serde_json::Value =
        serde_json::from_slice(&original).expect("parse strict summary manifest");
    strict_manifest["artifacts"]
        .as_array_mut()
        .expect("manifest artifacts")
        .iter_mut()
        .find(|artifact| artifact["path"] == "diagnostics.jsonl")
        .expect("diagnostics artifact")["sha256"] = artifact_digest(&diagnostics).into();
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&strict_manifest).expect("encode strict summary manifest"),
    )
    .expect("write strict summary manifest");
    let unknown_summary = aggregate(
        &sandbox.path().join("unknown-summary-field"),
        &[&clean, &reading],
    );
    assert_aggregate_failure(&unknown_summary, "AGGREGATE_DIAGNOSTICS_INVALID");

    diagnostic_lines
        .last_mut()
        .expect("summary diagnostic")
        .as_object_mut()
        .expect("summary object")
        .remove("unexpected");
    diagnostic_lines.last_mut().expect("summary diagnostic")["evidence"] =
        "arbitrary-location.json".into();
    let mut encoded_diagnostics = diagnostic_lines
        .iter()
        .map(|line| serde_json::to_string(line).expect("encode diagnostic"))
        .collect::<Vec<_>>()
        .join("\n");
    encoded_diagnostics.push('\n');
    fs::write(&diagnostics, encoded_diagnostics).expect("write arbitrary summary evidence");
    strict_manifest["artifacts"]
        .as_array_mut()
        .expect("manifest artifacts")
        .iter_mut()
        .find(|artifact| artifact["path"] == "diagnostics.jsonl")
        .expect("diagnostics artifact")["sha256"] = artifact_digest(&diagnostics).into();
    fs::write(
        &reading,
        serde_json::to_vec_pretty(&strict_manifest).expect("encode strict summary manifest"),
    )
    .expect("write strict summary manifest");
    let arbitrary_summary = aggregate(
        &sandbox.path().join("arbitrary-summary-evidence"),
        &[&clean, &reading],
    );
    assert_aggregate_failure(&arbitrary_summary, "AGGREGATE_DIAGNOSTICS_INVALID");

    fs::write(&reading, &original).expect("restore source manifest");
    fs::write(&diagnostics, original_diagnostics).expect("restore source diagnostics");

    let log = reading
        .parent()
        .expect("reading evidence root")
        .join("logs/origin.exact-distribution.log");
    fs::write(log, "tampered\n").expect("tamper source raw log");
    let tampered = aggregate(&sandbox.path().join("tampered"), &[&clean, &reading]);
    assert_aggregate_failure(&tampered, "AGGREGATE_ARTIFACT_INVALID");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        fs::write(&reading, &original).expect("restore reading manifest for symlink fixtures");
        let linked_manifest = sandbox.path().join("linked-manifest.json");
        symlink(&reading, &linked_manifest).expect("link uploaded manifest");
        let linked_manifest_output = aggregate(
            &sandbox.path().join("linked-manifest-result"),
            &[&clean, &linked_manifest],
        );
        assert_aggregate_failure(&linked_manifest_output, "AGGREGATE_EVIDENCE_MISSING");

        let log = reading
            .parent()
            .expect("reading evidence root")
            .join("logs/origin.exact-distribution.log");
        fs::remove_file(&log).expect("remove uploaded log before linking");
        let external_log = sandbox.path().join("external.log");
        fs::write(&external_log, "external\n").expect("write external log");
        symlink(&external_log, &log).expect("link uploaded raw log");
        let linked_log_output = aggregate(
            &sandbox.path().join("linked-log-result"),
            &[&clean, &reading],
        );
        assert_aggregate_failure(&linked_log_output, "AGGREGATE_ARTIFACT_INVALID");
    }
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
fn frontend_lint_accepts_commonjs_plugin_but_rejects_undefined_globals_read_only() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("commonjs-plugin-reader");
    create_workspace(&workspace, "commonjs-plugin-reader");
    let before = workspace_files(&workspace);
    let passing = check(
        &workspace,
        &sandbox.path().join("passing-evidence"),
        &["frontend.lint"],
    );
    assert!(
        passing.status.success(),
        "CommonJS plugin must pass the canonical lint node: {}",
        String::from_utf8_lossy(&passing.stdout)
    );
    assert_eq!(node(&events(&passing), "frontend.lint")["outcome"], "pass");
    assert_eq!(workspace_files(&workspace), before);

    let plugin_path = workspace.join("frontend/modules/yydra-android-supply-chain/app.plugin.js");
    let mut plugin = fs::read_to_string(&plugin_path).expect("read config plugin");
    plugin.push_str("\nyydraUndefinedPluginGlobal();\n");
    fs::write(&plugin_path, plugin).expect("add undefined plugin global fixture");
    let mutated = workspace_files(&workspace);
    let evidence = sandbox.path().join("failing-evidence");
    let failing = check(&workspace, &evidence, &["frontend.lint"]);
    assert!(!failing.status.success());
    assert_eq!(
        node(&events(&failing), "frontend.lint")["cause"]["code"],
        "FRONTEND_LINT_FAILED"
    );
    let log = fs::read_to_string(evidence.join("logs/frontend.lint.log"))
        .expect("read canonical lint diagnostic");
    assert!(log.contains("yydraUndefinedPluginGlobal"));
    assert!(log.contains("no-undef"));
    assert_eq!(workspace_files(&workspace), mutated);
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
fn distribution_owned_frontend_test_resolves_committed_native_compatibility_aliases() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("frontend-positive-reader");
    create_workspace(&workspace, "frontend-positive-reader");

    let output = check(
        &workspace,
        &sandbox.path().join("evidence"),
        &["frontend.test"],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(node(&events(&output), "frontend.test")["outcome"], "pass");
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
fn baseline_skill_inventory_accepts_exact_bytes_and_rejects_missing_or_modified_snapshots() {
    let sandbox = tempdir().expect("create sandbox");
    let exact = sandbox.path().join("exact-skill-reader");
    create_workspace(&exact, "exact-skill-reader");

    let output = check(
        &exact,
        &sandbox.path().join("exact-evidence"),
        &["ownership.baseline-skills"],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        node(&events(&output), "ownership.baseline-skills")["outcome"],
        "pass"
    );

    for (name, mutation) in [("missing", "remove"), ("modified", "modify")] {
        let workspace = sandbox.path().join(format!("{name}-skill-reader"));
        create_workspace(&workspace, &format!("{name}-skill-reader"));
        let skill = workspace.join(".agents/skills/yydra-diagnose/SKILL.md");
        if mutation == "remove" {
            fs::remove_file(&skill).expect("remove exact Skill snapshot");
        } else {
            fs::write(
                &skill,
                "---\nname: yydra-diagnose\ndescription: edited\n---\n",
            )
            .expect("modify exact Skill snapshot");
        }

        let output = check(
            &workspace,
            &sandbox.path().join(format!("{name}-evidence")),
            &["ownership.baseline-skills"],
        );
        assert!(!output.status.success(), "accepted {name} Skill snapshot");
        let parsed = events(&output);
        assert_eq!(
            node(&parsed, "ownership.baseline-skills")["cause"]["code"],
            "BASELINE_SKILL_INVENTORY_DRIFT"
        );
    }
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
fn check_preserves_the_requested_cargo_job_limit_without_leaking_secrets() {
    let sandbox = tempdir().expect("create sandbox");
    let workspace = sandbox.path().join("serial-reader");
    create_workspace(&workspace, "serial-reader");
    let before = workspace_files(&workspace);
    let fake_bin = sandbox.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("create fake tool directory");
    write_executable(
        &fake_bin.join("cargo"),
        "#!/bin/sh\n[ \"$CARGO_BUILD_JOBS\" = 1 ] || exit 31\n[ -z \"$YYDRA_FIXTURE_SECRET\" ] || exit 32\nexit 0\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["--message-format=json", "check"])
        .arg(&workspace)
        .arg("--evidence-dir")
        .arg(sandbox.path().join("evidence"))
        .args(["--node", "rust.format"])
        .env("PATH", &fake_bin)
        .env("CARGO_BUILD_JOBS", "1")
        .env("YYDRA_FIXTURE_SECRET", "non-sensitive-test-sentinel")
        .output()
        .expect("run resource-limit contract fixture");
    let parsed = events(&output);
    assert_eq!(
        node(&parsed, "rust.format")["outcome"],
        "pass",
        "{}",
        node(&parsed, "rust.format")
    );
    assert!(output.status.success());
    assert_eq!(workspace_files(&workspace), before);
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
        "frontend/src/framework/android-supply-chain-plugin.test.mjs",
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
    assert_eq!(
        node(&parsed, "native.android-generation")["attempts"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        node(&parsed, "native.android-generation")["commands"]
            .as_array()
            .map(Vec::len),
        Some(1)
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
fn android_release_records_artifact_identity_without_license_or_source_review_read_only() {
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
    let dependency_seed = sandbox.path().join("gradle-dependency-seed");
    fs::create_dir_all(dependency_seed.join("modules-2/files-2.1"))
        .expect("create Gradle dependency cache seed");
    fs::write(
        dependency_seed.join("modules-2/files-2.1/yydra-seed-marker"),
        "public dependency cache seed",
    )
    .expect("write Gradle dependency cache seed marker");
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
  printf "%s\n" "allprojects { repositories { maven { url 'https://unreviewed.invalid/repository' } } }" > android/build.gradle
  printf '%s\n' 'generated build' > android/app/build.gradle
  printf '%s\n' \
    '#!/bin/sh' \
    'if [ "${EXPO_TOKEN+x}" = x ] || [ "${EAS_TOKEN+x}" = x ]; then exit 9; fi' \
    'if [ -f "$HOME/.expo/account.json" ] || [ -f "$XDG_CONFIG_HOME/expo/account.json" ]; then exit 9; fi' \
    'if [ "$CMAKE_BUILD_PARALLEL_LEVEL" != "1" ]; then exit 9; fi' \
    'if [ ! -f "$GRADLE_USER_HOME/caches/modules-2/files-2.1/yydra-seed-marker" ]; then exit 9; fi' \
    'if [ "$1" = "--version" ]; then printf "%s\\n" "Gradle 9.0.0"; exit 0; fi' \
    'case " $* " in *" assembleRelease "*) : ;; *) exit 9 ;; esac' \
    'case " $* " in *" :app:dependencies "*) : ;; *) exit 9 ;; esac' \
    'case " $* " in *" :app:yydraReleaseRuntimeMaterials "*) : ;; *) exit 9 ;; esac' \
    'previous=""' \
    'concurrency_init=""' \
    'for argument in "$@"; do' \
    '  if [ "$previous" = "-I" ] && grep -q "CMAKE_JOB_POOL_COMPILE=yydra_compile" "$argument"; then concurrency_init="$argument"; fi' \
    '  previous="$argument"' \
    'done' \
    'if [ -z "$concurrency_init" ]; then exit 9; fi' \
    '  material_dir="$PWD/unreviewed/cache"' \
    '  mkdir -p "$material_dir"' \
    '  printf "%s\\n" "fake runtime material" > "$material_dir/native-1.0.0.bin"' \
    '  printf "{\\\"schemaVersion\\\":3,\\\"configuration\\\":\\\"releaseRuntimeClasspath\\\",\\\"bundleBinding\\\":{\\\"taskPath\\\":\\\":app:createBundleReleaseJsAndAssets\\\",\\\"bundleFile\\\":\\\"%s\\\",\\\"sourceMapFile\\\":\\\"%s\\\"},\\\"repositories\\\":[{\\\"kind\\\":\\\"unreviewed\\\",\\\"url\\\":\\\"https://unreviewed.invalid/repository/\\\"}],\\\"components\\\":[{\\\"componentType\\\":\\\"module\\\",\\\"group\\\":\\\"com.example\\\",\\\"name\\\":\\\"native\\\",\\\"version\\\":\\\"1.0.0\\\",\\\"projectPath\\\":null,\\\"sourceDirectory\\\":null,\\\"artifacts\\\":[{\\\"file\\\":\\\"%s\\\",\\\"extension\\\":\\\"bin\\\",\\\"classifier\\\":\\\"\\\"}]},{\\\"componentType\\\":\\\"project\\\",\\\"group\\\":\\\"com.example\\\",\\\"name\\\":\\\"local\\\",\\\"version\\\":\\\"workspace\\\",\\\"projectPath\\\":\\\":local\\\",\\\"sourceDirectory\\\":null,\\\"artifacts\\\":[]}],\\\"dependencies\\\":[{\\\"from\\\":\\\"urn:yydra:gradle-project::app\\\",\\\"to\\\":\\\"pkg:maven/com.example/native@1.0.0\\\",\\\"selectedVariant\\\":\\\"runtimeElements\\\",\\\"selectedVariantAttributes\\\":{\\\"org.gradle.usage\\\":\\\"java-runtime\\\"}},{\\\"from\\\":\\\"urn:yydra:gradle-project::app\\\",\\\"to\\\":\\\"urn:yydra:gradle-project::local\\\",\\\"selectedVariant\\\":\\\"runtimeElements\\\",\\\"selectedVariantAttributes\\\":{\\\"org.gradle.usage\\\":\\\"java-runtime\\\"}}]}\\n" "$PWD/app/build/generated/assets/react/release/index.android.bundle" "$PWD/app/build/generated/sourcemaps/react/release/index.android.bundle.map" "$material_dir/native-1.0.0.bin" > "$YYDRA_GRADLE_MATERIALS_RAW"' \
    '  printf "%s\\n" "releaseRuntimeClasspath - Runtime classpath of /release" "+--- com.example:native:1.0.0"' \
    'mkdir -p app/build/outputs/apk/release' \
    'mkdir -p app/build/generated/assets/react/release' \
    'mkdir -p app/build/generated/sourcemaps/react/release' \
    "printf '%s\\n' 'account-free release' > app/build/outputs/apk/release/app-release.apk" \
    "printf '%s\\n' 'fake release JavaScript bundle' > app/build/generated/assets/react/release/index.android.bundle" \
    'printf "{\\\"version\\\":3,\\\"file\\\":\\\"index.android.bundle\\\",\\\"sources\\\":[\\\"/workspace/frontend/node_modules/expo/AppEntry.js\\\"],\\\"names\\\":[],\\\"mappings\\\":\\\"\\\"}\\n" > app/build/generated/sourcemaps/react/release/index.android.bundle.map' \
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
    let evidence = sandbox.path().join("evidence");
    let run = |evidence: &Path| {
        Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args([
                "--message-format=json",
                "check",
                workspace.to_str().expect("UTF-8 workspace"),
                "--evidence-dir",
                evidence.to_str().expect("UTF-8 evidence"),
                "--node",
                "android.release",
            ])
            .env("PATH", &path)
            .env("EXPO_TOKEN", "must-not-reach-build")
            .env("EAS_TOKEN", "must-not-reach-build")
            .env("YYDRA_GRADLE_DEPENDENCY_CACHE_SEED", &dependency_seed)
            .env("HOME", &poisoned_home)
            .env("XDG_CONFIG_HOME", &poisoned_config)
            .output()
            .expect("run account-free Android release check")
    };
    let output = run(&evidence);
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
                command.contains(
                    "./gradlew --no-daemon assembleRelease :app:dependencies --configuration releaseRuntimeClasspath :app:yydraReleaseRuntimeMaterials"
                )
                    && command.contains("gradle-concurrency.init.gradle")
                    && command.contains("-Pkotlin.compiler.execution.strategy=in-process")
                    && command.contains("--max-workers=1")
            }))
    );
    let concurrency_init = fs::read_to_string(
        evidence.join("artifacts/android.release/gradle-concurrency.init.gradle"),
    )
    .expect("read Gradle concurrency init script");
    assert!(concurrency_init.contains("CMAKE_JOB_POOLS=yydra_compile=1;yydra_link=1"));
    assert!(concurrency_init.contains("CMAKE_JOB_POOL_COMPILE=yydra_compile"));
    assert!(concurrency_init.contains("CMAKE_JOB_POOL_LINK=yydra_link"));
    let material_init =
        fs::read_to_string(evidence.join("artifacts/android.release/gradle-materials.init.gradle"))
            .expect("read Gradle material init script");
    assert!(!material_init.contains("repository.url"));
    assert!(!material_init.contains("createArtifactResolutionQuery"));
    assert!(material_init.contains("createBundleReleaseJsAndAssets"));
    assert!(material_init.contains("bundleBinding"));
    assert!(
        release["commands"]
            .as_array()
            .expect("release commands")
            .iter()
            .any(|command| command.as_str().is_some_and(|command| {
                command.contains(":app:dependencies")
                    && command.contains(":app:yydraReleaseRuntimeMaterials")
                    && command.contains(
                        ":app:dependencies --configuration releaseRuntimeClasspath :app:yydraReleaseRuntimeMaterials"
                    )
            }))
    );
    let materials: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("artifacts/android.release/gradle-materials.json"))
            .expect("read material inventory"),
    )
    .expect("decode material inventory");
    assert_eq!(materials["repositories"], serde_json::json!([]));
    assert_eq!(
        materials["notEvaluated"],
        serde_json::json!([
            "license-review",
            "dependency-source-trust",
            "upstream-provenance",
            "notice-completeness"
        ])
    );
    assert_eq!(
        materials["components"][0]["detectedLicense"],
        "not-evaluated"
    );
    assert_eq!(
        materials["components"][0]["exposure"],
        "runtime-build-input"
    );
    assert_eq!(
        materials["localComponents"][0]["exposure"],
        "dependency-graph-only"
    );
    assert_eq!(
        materials["localComponents"][0]["sourceAuthority"],
        "not-evaluated"
    );
    assert_eq!(
        materials["localComponents"][0]["sourcePath"],
        "not-recorded"
    );
    assert!(
        materials["components"][0]["provenance"]
            .get("configuredRepositories")
            .is_none()
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
    assert!(
        identity["resolvedDependencyGraphSha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
    assert!(
        identity["gradleMaterialInventorySha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
    assert!(
        evidence
            .join("artifacts/android.release/gradle-materials.json")
            .is_file()
    );
    assert!(
        evidence
            .join("artifacts/android.release/index.android.bundle.map")
            .is_file()
    );
    assert_eq!(identity["bundlePath"], "index.android.bundle");
    assert_eq!(identity["bundleBytes"], 31);
    assert!(
        identity["bundleSha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
    assert_eq!(identity["sourceMapPath"], "index.android.bundle.map");
    assert_eq!(identity["sourceMapBytes"], 133);
    assert!(
        identity["sourceMapSha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
    assert_eq!(
        identity["sha256"],
        format!(
            "sha256:{}",
            hex::encode(sha2::Sha256::digest(b"account-free release\n"))
        )
    );
    assert_eq!(workspace_files(&workspace), before);
    assert!(!workspace.join("frontend/android").exists());

    let tool = fake_bin.join("npm");
    let fixture = fs::read_to_string(&tool).expect("read external tool fixture");
    let wrong_map = fixture.replace(
        r#"\\\"file\\\":\\\"index.android.bundle\\\""#,
        r#"\\\"file\\\":\\\"wrong.bundle\\\""#,
    );
    assert_ne!(
        wrong_map, fixture,
        "replace the emitted source-map declaration"
    );
    write_executable(&tool, &wrong_map);
    let wrong = run(&sandbox.path().join("wrong-map-evidence"));
    assert!(
        !wrong.status.success(),
        "wrong source-map bundle declaration must fail"
    );
    assert_eq!(
        node(&events(&wrong), "android.release")["cause"]["code"],
        "SUPPLY_CHAIN_PROVENANCE_MISMATCH"
    );
    assert_eq!(workspace_files(&workspace), before);

    let no_file = fixture.replace(r#"\\\"file\\\":\\\"index.android.bundle\\\","#, "");
    assert_ne!(
        no_file, fixture,
        "remove the optional Hermes source-map file field"
    );
    write_executable(&tool, &no_file);
    let missing = run(&sandbox.path().join("hermes-map-evidence"));
    assert!(
        missing.status.success(),
        "Hermes maps use the captured Gradle output and hash binding"
    );
    assert_eq!(workspace_files(&workspace), before);

    write_executable(
        &tool,
        &fixture.replace("$PWD/unreviewed/cache", "$PWD/../../../../outside-build"),
    );
    let rejected = run(&sandbox.path().join("outside-build-evidence"));
    assert!(!rejected.status.success());
    let rejected_events = events(&rejected);
    let release = node(&rejected_events, "android.release");
    assert_eq!(
        release["cause"]["code"],
        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED"
    );
    assert!(
        release["cause"]["message"]
            .as_str()
            .unwrap()
            .contains("escapes the isolated cache")
    );
    assert_eq!(workspace_files(&workspace), before);
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
  printf "%s\n" "buildscript { repositories { google(); mavenCentral() } } allprojects { repositories { google(); mavenCentral(); maven { url 'https://www.jitpack.io' } } }" > android/build.gradle
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
  printf "%s\n" "buildscript { repositories { google(); mavenCentral() } } allprojects { repositories { google(); mavenCentral(); maven { url 'https://www.jitpack.io' } } }" > android/build.gradle
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
