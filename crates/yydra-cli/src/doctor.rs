// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only Workspace and development-tool diagnostics.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::product_build::BuildTarget;
use crate::{
    Diagnostic, Reporter, find_workspace_root, install_shutdown_handler, npm_program,
    verify_workspace,
};

pub(crate) fn diagnose(
    workspace: &Path,
    target: Option<BuildTarget>,
    reporter: &Reporter,
) -> Result<()> {
    let root = find_workspace_root(workspace)?;
    let shutdown = install_shutdown_handler().context("install doctor shutdown handler")?;
    let mut doctor = Doctor {
        root: &root,
        reporter,
        shutdown: &shutdown,
        failures: 0,
    };
    doctor.report("doctor.verify", "DOCTOR_WORKSPACE_VERIFY", true,
        "restore the reported authority or install the exact CLI version named by the Workspace Origin Record",
        verify_workspace(&root).map(|_| "Workspace origin and Distribution snapshots match".to_owned()));

    for (phase, program, arguments, nightly) in [
        ("doctor.rustc", "rustc", &["--version"][..], true),
        ("doctor.cargo", "cargo", &["--version"][..], true),
        ("doctor.rustfmt", "rustfmt", &["--version"][..], true),
        (
            "doctor.clippy",
            "cargo",
            &["clippy", "--version"][..],
            false,
        ),
    ] {
        let result = doctor.probe(program, arguments).and_then(|version| {
            if nightly
                && !version
                    .split_whitespace()
                    .nth(1)
                    .and_then(|value| semver::Version::parse(value).ok())
                    .is_some_and(|version| version.pre.as_str() == "nightly")
            {
                bail!("expected the Workspace's nightly toolchain; observed {version}");
            }
            Ok(version)
        });
        doctor.report(phase, "DOCTOR_RUST_TOOL", true,
            "install the Workspace's nightly toolchain with rustfmt and clippy; remove an incompatible toolchain override", result);
    }

    if target != Some(BuildTarget::Server) {
        for (phase, program) in [("doctor.node", "node"), ("doctor.npm", npm_program())] {
            let result = doctor.probe(program, &["--version"]).and_then(|version| {
                semver::Version::parse(version.trim_start_matches('v'))
                    .context("tool did not report a valid version")?;
                Ok(version)
            });
            doctor.report(phase, "DOCTOR_FRONTEND_TOOL", true,
                "install Node.js and npm compatible with the frontend dependencies, and make them available on PATH", result);
        }
        let installed = root.join("frontend/node_modules").is_dir();
        doctor.report(
            "doctor.frontend-dependencies",
            "DOCTOR_FRONTEND_DEPENDENCIES",
            false,
            "run `yydra setup` before building; doctor can run before dependency installation",
            if installed {
                Ok(
                    "frontend/node_modules exists; dependency installation is owned by setup"
                        .to_owned(),
                )
            } else {
                Err(anyhow::anyhow!("frontend dependencies are not installed"))
            },
        );
    }

    if target.is_none() || target == Some(BuildTarget::Server) {
        for (phase, args) in [
            (
                "doctor.docker",
                &["version", "--format", "{{.Server.Version}}"][..],
            ),
            ("doctor.compose", &["compose", "version", "--short"][..]),
        ] {
            let result = doctor.probe("docker", args);
            doctor.report(phase, "DOCTOR_OPTIONAL_DOCKER", false,
                "install/start Docker with Compose for local containers, or use an external PostgreSQL service", result);
        }
    }
    if target == Some(BuildTarget::Android) {
        doctor.android();
    }
    if shutdown.load(Ordering::SeqCst) {
        reporter.emit(Diagnostic {
            phase: "doctor.summary",
            code: "DOCTOR_CANCELLED",
            severity: "error",
            status: "fail",
            message: "doctor was cancelled",
            location: Some(&root),
            remediation: None,
        });
        bail!("doctor was cancelled");
    }
    let failures = doctor.failures;
    reporter.emit(Diagnostic {
        phase: "doctor.summary", code: "DOCTOR_SUMMARY",
        severity: if failures == 0 { "info" } else { "error" },
        status: if failures == 0 { "pass" } else { "fail" },
        message: &format!("{failures} required diagnostics failed; this checks environment readiness, not application correctness or build success"),
        location: Some(&root), remediation: None,
    });
    if failures != 0 {
        bail!("doctor found {failures} required environment or Workspace problems");
    }
    Ok(())
}

struct Doctor<'a> {
    root: &'a Path,
    reporter: &'a Reporter,
    shutdown: &'a AtomicBool,
    failures: usize,
}

impl Doctor<'_> {
    fn probe(&self, program: impl AsRef<OsStr>, arguments: &[&str]) -> Result<String> {
        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(self.root)
            // Rustup otherwise may install the selected toolchain while probing it.
            .env("RUSTUP_AUTO_INSTALL", "0")
            .env("NO_UPDATE_NOTIFIER", "1")
            .env("EXPO_NO_TELEMETRY", "1");
        let output =
            crate::process::capture(command, self.shutdown, Some(Duration::from_secs(10)))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let text = text.trim();
        if !output.status.success() {
            bail!("tool exited with {}: {text}", output.status);
        }
        if text.is_empty() {
            bail!("tool returned no version information");
        }
        Ok(text.to_owned())
    }

    fn report(
        &mut self,
        phase: &str,
        code: &str,
        required: bool,
        remediation: &str,
        result: Result<String>,
    ) {
        let (status, severity, message, remediation) = match result {
            Ok(message) => ("pass", "info", message, None),
            Err(error) => {
                if required {
                    self.failures += 1;
                }
                (
                    if required { "fail" } else { "warning" },
                    if required { "error" } else { "warning" },
                    format!("{error:#}"),
                    Some(remediation),
                )
            }
        };
        self.reporter.emit(Diagnostic {
            phase,
            code,
            severity,
            status,
            message: &message,
            location: Some(self.root),
            remediation,
        });
    }

    fn android(&mut self) {
        for (phase, executable) in [("doctor.java", "java"), ("doctor.javac", "javac")] {
            let program = std::env::var_os("JAVA_HOME")
                .filter(|value| !value.is_empty())
                .map(|home| {
                    PathBuf::from(home)
                        .join("bin")
                        .join(executable_name(executable))
                })
                .unwrap_or_else(|| PathBuf::from(executable_name(executable)));
            let result = self.probe(&program, &["-version"]);
            self.report(phase, "DOCTOR_ANDROID_JDK", true,
                "install a JDK compatible with the Expo/React Native Android build and correct JAVA_HOME or PATH", result);
        }
        let sdk = android_sdk_root();
        let result = sdk
            .as_ref()
            .map(|path| format!("Android SDK: {}", path.display()))
            .map_err(|error| anyhow::anyhow!("{error:#}"));
        self.report("doctor.android-sdk", "DOCTOR_ANDROID_SDK", true,
            "set ANDROID_HOME to the installed Android SDK; ANDROID_SDK_ROOT, when set, must identify the same directory", result);
        let Ok(sdk) = sdk else {
            return;
        };
        let adb = self.probe(
            sdk.join("platform-tools").join(executable_name("adb")),
            &["version"],
        );
        self.report(
            "doctor.android-platform-tools",
            "DOCTOR_ANDROID_PLATFORM_TOOLS",
            true,
            "install Android SDK Platform-Tools in the selected SDK",
            adb,
        );
        for (phase, directory, marker) in [
            ("doctor.android-platform", "platforms", "android.jar"),
            (
                "doctor.android-build-tools",
                "build-tools",
                executable_name("aapt2"),
            ),
            ("doctor.android-ndk", "ndk", "source.properties"),
            (
                "doctor.android-cmake",
                "cmake",
                if cfg!(windows) {
                    "bin/cmake.exe"
                } else {
                    "bin/cmake"
                },
            ),
        ] {
            self.report(phase, "DOCTOR_ANDROID_COMPONENT", true,
                "install the Android SDK component required by the locked Expo/React Native build; exact package compatibility is established by `yydra build --target android`",
                installed_components(&sdk.join(directory), marker));
        }
    }
}

fn executable_name(name: &str) -> &str {
    if cfg!(windows) {
        match name {
            "java" => "java.exe",
            "javac" => "javac.exe",
            "adb" => "adb.exe",
            "aapt2" => "aapt2.exe",
            _ => name,
        }
    } else {
        name
    }
}

fn android_sdk_root() -> Result<PathBuf> {
    let home = std::env::var_os("ANDROID_HOME").filter(|value| !value.is_empty());
    let legacy = std::env::var_os("ANDROID_SDK_ROOT").filter(|value| !value.is_empty());
    if let (Some(home), Some(legacy)) = (&home, &legacy)
        && fs::canonicalize(home)
            .ok()
            .zip(fs::canonicalize(legacy).ok())
            .is_none_or(|(a, b)| a != b)
    {
        bail!("ANDROID_HOME and ANDROID_SDK_ROOT must resolve to the same installed SDK");
    }
    // The account-free build replaces HOME, so the SDK must be explicit.
    let path = home
        .or(legacy)
        .map(PathBuf::from)
        .context("set ANDROID_HOME explicitly so the isolated Android build can locate the SDK")?;
    if !path.is_dir() {
        bail!("Android SDK directory does not exist: {}", path.display());
    }
    Ok(path)
}

fn installed_components(directory: &Path, marker: &str) -> Result<String> {
    let mut versions = fs::read_dir(directory)
        .with_context(|| format!("read SDK components in {}", directory.display()))?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|entry| entry.path().join(marker).is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    versions.sort();
    if versions.is_empty() {
        bail!(
            "no installed component containing {marker} in {}",
            directory.display()
        );
    }
    Ok(format!(
        "installed: {}; build selects the required version",
        versions.join(", ")
    ))
}
