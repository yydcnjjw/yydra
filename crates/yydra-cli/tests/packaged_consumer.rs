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

    let extracted = package_target.join("package/yydra-cli-0.1.0");
    assert!(extracted.join("Cargo.lock").is_file());
    assert!(
        extracted
            .join("template/product-workspace/.yydra/origin.toml")
            .is_file()
    );
    let packaged_manifest = fs::read_to_string(extracted.join("Cargo.toml"))
        .expect("read normalized packaged manifest");
    assert!(!packaged_manifest.contains("path = \"../../"));

    let install_root = sandbox.path().join("install");
    let install = Command::new(&cargo)
        .args([
            "install",
            "yydra-cli@0.1.0",
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
        ])
        .output()
        .expect("run installed packaged CLI");
    assert!(
        create.status.success(),
        "create stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    assert!(workspace.join(".yydra/origin.toml").is_file());
}
