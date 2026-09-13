// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn android_build_reports_generation_gradle_and_artifact_failures() {
    for (case, generation, gradle, expected) in [
        (
            "missing-host",
            "exit 0",
            "",
            "NATIVE_GENERATION_OUTPUT_MISSING",
        ),
        (
            "mutated-input",
            "echo changed >> ../Cargo.lock",
            "exit 0",
            "NATIVE_GENERATION_MUTATED_AUTHORED_INPUTS",
        ),
        (
            "gradle-failed",
            ":",
            "echo build-failed >&2; exit 7",
            "ANDROID_RELEASE_BUILD_FAILED",
        ),
        (
            "missing-apk",
            ":",
            "exit 0",
            "ANDROID_RELEASE_OUTPUT_MISSING",
        ),
    ] {
        let sandbox = tempfile::tempdir().unwrap();
        let root = sandbox.path().join(case);
        let created = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .arg("new")
            .arg(&root)
            .args([
                "--product-name",
                "Failure Product",
                "--product-id",
                "failure-product",
                "--product-source-license",
                "Apache-2.0",
            ])
            .output()
            .unwrap();
        assert!(created.status.success());
        let bin = sandbox.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let npm = bin.join("npm");
        fs::write(
            &npm,
            format!(
                r#"#!/bin/sh
if [ "$*" = 'run typecheck' ]; then exit 0; fi
test "$*" = 'run --ignore-scripts generate:android' || exit 99
{generation}
mkdir -p android/app
touch android/settings.gradle android/app/build.gradle
cat > android/gradlew <<'WRAPPER'
#!/bin/sh
{gradle}
WRAPPER
chmod +x android/gradlew
"#
            ),
        )
        .unwrap();
        fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let output = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args(["internal", "build"])
            .arg(&root)
            .args(["--target", "android"])
            .env("PATH", std::env::join_paths(paths).unwrap())
            .output()
            .unwrap();
        assert!(!output.status.success(), "{case} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn explicit_android_build_produces_an_apk_without_building_server_or_h5() {
    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path().join("product");
    let created = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            root.to_str().unwrap(),
            "--product-name",
            "Build Product",
            "--product-id",
            "build-product",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let npm = bin.join("npm");
    fs::write(
        &npm,
        r#"#!/bin/sh
if [ "$*" = "run typecheck" ]; then exit 0; fi
if [ "$*" != "run --ignore-scripts generate:android" ]; then exit 9; fi
mkdir -p android/app
touch android/settings.gradle android/app/build.gradle
cat > android/gradlew <<'WRAPPER'
#!/bin/sh
test "$CI" = 1 || exit 5
case "$GRADLE_USER_HOME" in *android-account-free*) ;; *) exit 6;; esac
mkdir -p app/build/outputs/apk/release
echo 'fixture APK' > app/build/outputs/apk/release/app-release.apk
WRAPPER
chmod +x android/gradlew
"#,
    )
    .unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    let cargo = bin.join("cargo");
    fs::write(&cargo, "#!/bin/sh\nexit 10\n").unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    for _ in 0..2 {
        let built = Command::new(env!("CARGO_BIN_EXE_yydra"))
            .args(["--message-format=json", "internal", "build"])
            .arg(&root)
            .args(["--target", "android"])
            .env("PATH", std::env::join_paths(&paths).unwrap())
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        assert!(
            root.join("frontend/android/app/build/outputs/apk/release/app-release.apk")
                .is_file()
        );
        assert!(!root.join("frontend/dist").exists());
        assert!(String::from_utf8_lossy(&built.stdout).contains("app-release.apk"));
    }
}

#[test]
fn default_build_delivers_server_and_h5_without_android_tools() {
    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path().join("product");
    let created = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            root.to_str().unwrap(),
            "--product-name",
            "Build Product",
            "--product-id",
            "build-product",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for (name, script) in [
        (
            "cargo",
            r#"#!/usr/bin/env python3
import json, os, pathlib, sys
assert sys.argv[1:] == ['build', '--locked', '--release', '--manifest-path', 'crates/server/Cargo.toml', '--bin', 'server', '--message-format=json']
root = pathlib.Path(os.getcwd())
artifact = root / 'custom-target/release/server'
artifact.parent.mkdir(parents=True, exist_ok=True)
artifact.write_text('built server')
print(json.dumps({'reason':'compiler-artifact','target':{'name':'server','kind':['bin']},'executable':str(artifact)}))
print(json.dumps({'reason':'build-finished','success':True}))
"#,
        ),
        (
            "npm",
            r#"#!/usr/bin/env python3
import pathlib, sys
if sys.argv[1:] == ['run', 'typecheck']:
    pathlib.Path('typecheck-passed').write_text('yes')
elif sys.argv[1:] == ['run', 'export:h5']:
    assert pathlib.Path('typecheck-passed').exists()
    pathlib.Path('dist').mkdir(exist_ok=True)
    pathlib.Path('dist/index.html').write_text('built H5')
else:
    sys.exit('unexpected frontend/native command: ' + repr(sys.argv))
"#,
        ),
    ] {
        let path = bin.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let selected = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["internal", "build"])
        .arg(&root)
        .args(["--target", "server"])
        .env("PATH", std::env::join_paths(&paths).unwrap())
        .output()
        .unwrap();
    assert!(selected.status.success());
    assert!(!root.join("frontend/typecheck-passed").exists());
    let built = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["--message-format=json", "internal", "build"])
        .arg(&root)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(root.join("custom-target/release/server").is_file());
    assert!(root.join("frontend/dist/index.html").is_file());
    let report = String::from_utf8_lossy(&built.stdout);
    assert!(report.contains("custom-target/release/server"), "{report}");
    assert!(report.contains("frontend/dist"), "{report}");
}

#[test]
fn frontend_failure_rejects_stale_artifacts_and_generate_is_not_a_public_command() {
    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path().join("product");
    let created = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args([
            "new",
            root.to_str().unwrap(),
            "--product-name",
            "Build Product",
            "--product-id",
            "build-product",
            "--product-source-license",
            "Apache-2.0",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    fs::create_dir_all(root.join("frontend/dist")).unwrap();
    fs::write(root.join("frontend/dist/index.html"), "old build").unwrap();
    let bin = sandbox.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let npm = bin.join("npm");
    fs::write(
        &npm,
        "#!/bin/sh\necho 'frontend prerequisite failed' >&2\nexit 7\n",
    )
    .unwrap();
    fs::set_permissions(npm, fs::Permissions::from_mode(0o755)).unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let built = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["--message-format=json", "internal", "build"])
        .arg(&root)
        .args(["--target", "h5"])
        .env("PATH", std::env::join_paths(paths).unwrap())
        .output()
        .unwrap();
    assert!(!built.status.success());
    let report = String::from_utf8_lossy(&built.stdout);
    assert!(report.contains("frontend prerequisite failed"));
    assert!(!report.contains("BUILD_ARTIFACT_READY"));
    let removed = Command::new(env!("CARGO_BIN_EXE_yydra"))
        .args(["generate", "api"])
        .arg(root)
        .output()
        .unwrap();
    assert_eq!(removed.status.code(), Some(2));
}
