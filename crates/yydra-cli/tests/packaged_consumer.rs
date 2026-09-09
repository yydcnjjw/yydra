// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn packaged_cli_preserves_its_lock_and_installs_through_the_exact_locked_path() {
    let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sandbox = tempdir().expect("create packaged-consumer sandbox");
    let package_target = sandbox.path().join("package-target");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());

    let package = Command::new(&cargo)
        .args([
            "package",
            "--manifest-path",
            package_root
                .join("Cargo.toml")
                .to_str()
                .expect("UTF-8 manifest path"),
            "--locked",
            "--offline",
            "--allow-dirty",
            "--target-dir",
            package_target.to_str().expect("UTF-8 package target"),
        ])
        .output()
        .expect("package yydra-cli");
    assert!(
        package.status.success(),
        "package stderr: {}",
        String::from_utf8_lossy(&package.stderr)
    );

    let extracted = package_target.join("package/yydra-cli-0.5.0");
    assert!(extracted.join("Cargo.lock").is_file());
    assert!(extracted.join("third-party/bolts-source.json").is_file());
    for removed in ["supply-chain", "src/supply_chain.rs", "build.rs"] {
        assert!(
            !extracted.join(removed).exists(),
            "packaged obsolete input {removed}"
        );
    }
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        assert_eq!(
            fs::read(extracted.join(license)).expect("read packaged license"),
            fs::read(package_root.join(license)).expect("read crate license")
        );
    }
    assert!(
        extracted
            .join("template/product-workspace/.yydra/origin.toml")
            .is_file()
    );
    for baseline_skill in [
        "yydra-product-change/SKILL.md",
        "yydra-product-change/references/product-change-path.md",
        "yydra-product-change/references/validation.md",
        "yydra-diagnose/SKILL.md",
        "yydra-diagnose/references/diagnostic-contract.md",
        "yydra-diagnose/references/repair-routes.md",
    ] {
        assert!(
            extracted
                .join("template/product-workspace/.agents/skills")
                .join(baseline_skill)
                .is_file(),
            "missing packaged Baseline Skill artifact {baseline_skill}"
        );
    }
    for relative in [
        "src/lib.rs",
        "Cargo.toml.tmpl",
        "README.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
    ] {
        let path = extracted
            .join("template/product-workspace/.yydra/build-support")
            .join(relative);
        assert!(
            fs::symlink_metadata(&path).unwrap().is_file(),
            "bundle must contain portable regular files: {}",
            path.display()
        );
    }
    let packaged_manifest = fs::read_to_string(extracted.join("Cargo.toml"))
        .expect("read normalized packaged manifest");
    assert!(!packaged_manifest.contains("path = \"../../"));

    let install_root = sandbox.path().join("install");
    let install = Command::new(&cargo)
        .args([
            "install",
            "yydra-cli@0.5.0",
            "--path",
            extracted.to_str().expect("UTF-8 extracted package"),
            "--locked",
            "--offline",
            "--root",
            install_root.to_str().expect("UTF-8 install root"),
            "--target-dir",
            sandbox
                .path()
                .join("install-target")
                .to_str()
                .expect("UTF-8 install target"),
        ])
        .output()
        .expect("install exact packaged CLI");
    assert!(
        install.status.success(),
        "install stderr: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let executable = install_root
        .join("bin")
        .join(format!("yydra{}", std::env::consts::EXE_SUFFIX));
    let workspace = sandbox.path().join("workspace");
    let create = Command::new(&executable)
        .args([
            "new",
            workspace.to_str().expect("UTF-8 workspace"),
            "--product-name",
            "Packaged Reader",
            "--product-id",
            "packaged-reader",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .expect("run installed packaged CLI");
    assert!(
        create.status.success(),
        "create stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    assert!(workspace.join(".yydra/origin.toml").is_file());
    for removed in [
        ".yydra/supply-chain-policy.json",
        ".yydra/supply-chain-exceptions.json",
    ] {
        assert!(
            !workspace.join(removed).exists(),
            "generated obsolete input {removed}"
        );
    }
    let doctor = Command::new(&executable)
        .arg("doctor")
        .arg(&workspace)
        .output()
        .expect("diagnose independently packaged Workspace");
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    for materialized in [
        "Cargo.toml",
        "Cargo.lock",
        "crates/application/Cargo.toml",
        "crates/application/src/post_commit.rs",
        "crates/application/tests/post_commit_executor.rs",
        "crates/domain/Cargo.toml",
        "crates/persistence-postgres/Cargo.toml",
        "crates/server/Cargo.toml",
        "crates/server/src/main.rs",
        "crates/transport-http/Cargo.toml",
        "frontend/app/index.tsx",
        "frontend/.npmrc",
        "frontend/package-lock.json",
        "migrations/0001_baseline.sql",
        "migrations/0005_reading_progress.sql",
    ] {
        assert!(
            workspace.join(materialized).is_file(),
            "missing packaged template artifact {materialized}"
        );
    }
    let mut pending = vec![workspace.clone()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("inspect materialized Workspace") {
            let path = entry.expect("read materialized entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                assert_ne!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("tmpl"),
                    "unmaterialized manifest template: {}",
                    path.display()
                );
            }
        }
    }
    let metadata = Command::new(&cargo)
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--no-deps",
            "--format-version=1",
        ])
        .current_dir(&workspace)
        .output()
        .expect("read freshly created Workspace metadata");
    assert!(
        metadata.status.success(),
        "metadata stderr: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let setup = Command::new(&executable)
        .arg("setup")
        .arg(&workspace)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .expect("prepare frontend tools before compiling the api-build Workspace member");
    assert!(
        setup.status.success(),
        "setup stderr: {}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let workspace_tests = Command::new(&cargo)
        .args([
            "test",
            "--locked",
            "--offline",
            "--workspace",
            "--all-targets",
        ])
        .current_dir(&workspace)
        .output()
        .expect("test freshly created Workspace");
    assert!(
        workspace_tests.status.success(),
        "Workspace test stderr: {}",
        String::from_utf8_lossy(&workspace_tests.stderr)
    );
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        assert_eq!(
            fs::read(workspace.join(license)).expect("read Workspace license snapshot"),
            fs::read(package_root.join(license)).expect("read crate license")
        );
    }
    let evidence = sandbox.path().join("skill-evidence");
    let skill_check = Command::new(&executable)
        .args([
            "--message-format=json",
            "check",
            workspace.to_str().expect("UTF-8 workspace"),
            "--evidence-dir",
            evidence.to_str().expect("UTF-8 evidence path"),
            "--node",
            "ownership.baseline-skills",
        ])
        .output()
        .expect("check exact packaged Baseline Skill inventory");
    assert!(
        skill_check.status.success(),
        "packaged Skill check stderr: {}",
        String::from_utf8_lossy(&skill_check.stderr)
    );
}
