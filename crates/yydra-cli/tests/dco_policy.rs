// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

#[test]
fn dco_check_requires_matching_signoff_on_every_submitted_commit() {
    let sandbox = tempdir().expect("create git sandbox");
    let repository = sandbox.path();
    git(repository, &["init", "--quiet"]);
    git(repository, &["config", "user.name", "Test Contributor"]);
    git(repository, &["config", "user.email", "test@example.com"]);

    fs::write(repository.join("seed.txt"), b"seed\n").expect("write seed");
    git(repository, &["add", "seed.txt"]);
    git(
        repository,
        &["commit", "--quiet", "--signoff", "-m", "seed"],
    );
    let base = git_stdout(repository, &["rev-parse", "HEAD"]);

    fs::write(repository.join("signed.txt"), b"signed\n").expect("write signed change");
    git(repository, &["add", "signed.txt"]);
    git(
        repository,
        &["commit", "--quiet", "--signoff", "-m", "signed"],
    );
    fs::write(repository.join("unsigned.txt"), b"unsigned\n").expect("write unsigned change");
    git(repository, &["add", "unsigned.txt"]);
    git(repository, &["commit", "--quiet", "-m", "unsigned"]);
    let unsigned = git_stdout(repository, &["rev-parse", "HEAD"]);

    let rejected = dco(repository, &base, &unsigned);
    assert!(!rejected.status.success());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains(&unsigned), "stderr: {stderr}");
    assert!(
        stderr.contains("Signed-off-by: Test Contributor <test@example.com>"),
        "stderr: {stderr}"
    );

    git(
        repository,
        &["commit", "--quiet", "--amend", "--no-edit", "--signoff"],
    );
    let signed_head = git_stdout(repository, &["rev-parse", "HEAD"]);
    let accepted = dco(repository, &base, &signed_head);
    assert!(
        accepted.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
}

#[test]
fn pull_request_entrypoint_checks_the_full_range_and_documents_dco_without_a_cla() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workflow = fs::read_to_string(repository.join(".github/workflows/dco.yml"))
        .expect("read DCO workflow");
    assert!(workflow.contains("pull_request_target:"));
    assert!(workflow.contains("fetch-depth: 0"));
    assert!(workflow.contains("github.event.pull_request.base.sha"));
    assert!(workflow.contains("github.event.pull_request.head.sha"));
    assert!(workflow.contains("ref: ${{ github.event.pull_request.base.sha }}"));
    assert!(workflow.contains("pull/$PR_NUMBER/head"));
    assert!(workflow.contains("/bin/sh scripts/check-dco \"$BASE_SHA\" \"$HEAD_SHA\""));
    assert!(!workflow.contains("$HEAD_SHA:scripts/check-dco"));

    let contributing =
        fs::read_to_string(repository.join("CONTRIBUTING.md")).expect("read contribution policy");
    assert!(contributing.contains("Developer Certificate of Origin 1.1"));
    assert!(contributing.contains("Signed-off-by:"));
    assert!(contributing.contains("MIT OR Apache-2.0"));
    assert!(contributing.contains("does not use a Contributor License Agreement (CLA)"));
}

fn dco(repository: &Path, base: &str, head: &str) -> Output {
    Command::new("/bin/sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-dco"))
        .args([base, head])
        .current_dir(repository)
        .output()
        .expect("run DCO check")
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git {arguments:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git {arguments:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 git output")
        .trim()
        .to_owned()
}
