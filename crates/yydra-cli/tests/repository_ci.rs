// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;

#[test]
fn github_ci_builds_and_tests_only_the_cli() {
    let workflow_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/quality.yml");
    if !workflow_path.is_file() {
        return;
    }
    let workflow = fs::read_to_string(workflow_path).expect("read CLI workflow");
    assert!(workflow.contains("name: Yydra CLI CI"));
    assert!(workflow.contains("name: Build and test yydra-cli"));
    for required in [
        "pull_request:",
        "push:",
        "branches: [main]",
        "workflow_dispatch:",
        "moon run repo:ci-build",
        "moon run repo:test",
    ] {
        assert!(
            workflow.contains(required),
            "missing CLI CI requirement: {required}"
        );
    }
    for consumer_command in [
        "--fixture",
        "--aggregate-evidence",
        "--include-ignored",
        "--ignored",
        "npm ",
        "gradlew",
    ] {
        assert!(
            !workflow.contains(consumer_command),
            "consumer acceptance is outside CLI CI: {consumer_command}"
        );
    }
    assert!(!workflow.contains("actions/checkout@v"));
    let tasks = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../moon.yml"))
        .expect("read repository moon tasks");
    assert!(tasks.contains("command: cargo build --locked --release --package yydra-cli"));
    assert!(tasks.contains("command: cargo test --locked --package yydra-cli --all-targets"));
}
