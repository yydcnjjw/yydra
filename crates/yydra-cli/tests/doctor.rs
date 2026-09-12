// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn executable(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path().join("product");
    let created = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .arg("new")
        .arg(&root)
        .args([
            "--product-name",
            "Doctor Product",
            "--product-id",
            "doctor-product",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for (name, body) in [
        (
            "rustc",
            "echo 'rustc 1.100.0-nightly (aaaaaaaaa 2026-09-10)'",
        ),
        (
            "rustfmt",
            "echo 'rustfmt 1.9.0-nightly (aaaaaaaaa 2026-09-10)'",
        ),
        (
            "cargo",
            "if [ \"$*\" = 'clippy --version' ]; then echo 'clippy 0.1.100 (aaaaaaaaa 2026-09-10)'; elif [ \"$*\" = '--version' ]; then echo 'cargo 1.100.0-nightly (aaaaaaaaa 2026-09-10)'; else exit 99; fi",
        ),
        ("node", "test \"$*\" = --version || exit 99; echo v26.8.2"),
        ("npm", "test \"$*\" = --version || exit 99; echo 12.0.2"),
    ] {
        executable(
            &bin.join(name),
            &format!("test \"$RUSTUP_AUTO_INSTALL\" = 0 || exit 98\n{body}"),
        );
    }
    (sandbox, root, bin)
}

fn doctor(root: &Path, bin: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["--message-format=json", "doctor"])
        .arg(root)
        .args(args)
        .env("PATH", bin)
        .env_remove("JAVA_HOME")
        .env_remove("ANDROID_HOME")
        .env_remove("ANDROID_SDK_ROOT")
        .output()
        .unwrap()
}

fn events(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(inventory(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}

#[test]
fn default_doctor_is_read_only_and_optional_dependencies_do_not_block_setup() {
    let (_sandbox, root, bin) = fixture();
    let before = inventory(&root);
    let output = doctor(&root, &bin, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = events(&output);
    assert!(
        events
            .iter()
            .any(|e| e["phase"] == "doctor.docker" && e["status"] == "warning")
    );
    assert!(
        events
            .iter()
            .any(|e| e["phase"] == "doctor.frontend-dependencies" && e["status"] == "warning")
    );
    assert!(!events.iter().any(|e| e["phase"] == "doctor.java"));
    assert_eq!(events.last().unwrap()["status"], "pass");
    assert_eq!(before, inventory(&root));
    assert!(!root.join("frontend/node_modules").exists());
    assert!(!root.join("target").exists());
}

#[test]
fn doctor_reports_all_missing_tools_and_rejects_an_effective_stable_compiler() {
    let (_sandbox, root, bin) = fixture();
    executable(
        &bin.join("rustc"),
        "echo 'rustc 1.99.0 (aaaaaaaaa 2026-09-10)'",
    );
    fs::remove_file(bin.join("node")).unwrap();
    fs::remove_file(bin.join("npm")).unwrap();
    let output = doctor(&root, &bin, &[]);
    assert!(!output.status.success());
    let events = events(&output);
    for phase in ["doctor.rustc", "doctor.node", "doctor.npm"] {
        assert!(
            events.iter().any(|e| e["phase"] == phase
                && e["status"] == "fail"
                && e["remediation"].is_string())
        );
    }
    assert_eq!(events.last().unwrap()["status"], "fail");
}

#[test]
fn server_doctor_does_not_require_frontend_or_android_tools() {
    let (_sandbox, root, bin) = fixture();
    fs::remove_file(bin.join("node")).unwrap();
    fs::remove_file(bin.join("npm")).unwrap();
    let output = doctor(&root, &bin, &["--target", "server"]);
    assert!(output.status.success());
    assert!(!events(&output).iter().any(|e| e["phase"] == "doctor.node"));
}

#[test]
fn android_doctor_inspects_existing_sdk_without_generating_a_host() {
    let (sandbox, root, bin) = fixture();
    executable(&bin.join("java"), "echo 'openjdk version 17.0.20' >&2");
    executable(&bin.join("javac"), "echo 'javac 17.0.20'");
    let sdk = sandbox.path().join("sdk");
    fs::create_dir_all(sdk.join("platform-tools")).unwrap();
    executable(
        &sdk.join("platform-tools/adb"),
        "echo 'Android Debug Bridge version 1.0.41'",
    );
    for marker in [
        "platforms/android-36/android.jar",
        "build-tools/36.0.0/aapt2",
        "ndk/27.1/source.properties",
        "cmake/3.22.1/bin/cmake",
    ] {
        let path = sdk.join(marker);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "fixture").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["--message-format=json", "doctor"])
        .arg(&root)
        .args(["--target", "android"])
        .env("PATH", &bin)
        .env("ANDROID_HOME", &sdk)
        .env_remove("ANDROID_SDK_ROOT")
        .env_remove("JAVA_HOME")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.join("frontend/android").exists());
    assert!(!root.join("frontend/node_modules").exists());
    let failed = doctor(&root, &bin, &["--target", "android"]);
    assert!(!failed.status.success());
    assert!(
        events(&failed)
            .iter()
            .any(|e| e["phase"] == "doctor.android-sdk" && e["status"] == "fail")
    );
}

#[test]
fn cancellation_during_optional_probe_cannot_report_success() {
    let (sandbox, root, bin) = fixture();
    let marker = sandbox.path().join("docker-started");
    executable(
        &bin.join("docker"),
        &format!("echo started > '{}'\nexec /bin/sleep 30", marker.display()),
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["--message-format=json", "doctor"])
        .arg(&root)
        .env("PATH", &bin)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() && std::time::Instant::now() < deadline {
        assert!(
            child.try_wait().unwrap().is_none(),
            "doctor exited before Docker probe"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(marker.exists());
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert_eq!(events(&output).last().unwrap()["code"], "DOCTOR_CANCELLED");
}

#[test]
fn removed_check_command_is_not_advertised_or_accepted() {
    let help = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&help.stdout).contains("  check "));
    let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .arg("check")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
}
