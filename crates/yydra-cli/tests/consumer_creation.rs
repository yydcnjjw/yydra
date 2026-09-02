// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
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
    assert_eq!(tracked_inventory(&destination).len(), 7);
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
