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
        .write_all(b" Acme Reader \n acme-reader \n")
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
    assert_eq!(tracked_inventory(&destination).len(), 3);
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
        .write_all(b"Acme Reader\nInvalid_ID\n")
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
            "umask 077; exec \"$YYDRA_BIN\" new \"$YYDRA_DEST\" --product-name 'Acme Reader' --product-id acme-reader",
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
