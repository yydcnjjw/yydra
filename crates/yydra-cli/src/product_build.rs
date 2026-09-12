// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde_json::Value;

use crate::{Diagnostic, Reporter, find_workspace_root, npm_program};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum BuildTarget {
    Server,
    H5,
    Android,
}

pub(crate) fn build(
    workspace: &Path,
    target: Option<BuildTarget>,
    reporter: &Reporter,
) -> Result<()> {
    let root = find_workspace_root(workspace)
        .context("BUILD_WORKSPACE_INVALID: locate Product Workspace")?;
    let targets: &[BuildTarget] = match target {
        None => &[BuildTarget::Server, BuildTarget::H5],
        Some(BuildTarget::Server) => &[BuildTarget::Server],
        Some(BuildTarget::H5) => &[BuildTarget::H5],
        Some(BuildTarget::Android) => &[BuildTarget::Android],
    };
    for target in targets {
        let (phase, code) = match target {
            BuildTarget::Server => ("build.server", "BUILD_SERVER_FAILED"),
            BuildTarget::H5 => ("build.h5", "BUILD_H5_FAILED"),
            BuildTarget::Android => ("build.android", "BUILD_ANDROID_FAILED"),
        };
        let artifact = reporter.phase(
            phase,
            code,
            Some(&root),
            Some("fix the reported build input or tool failure and rerun `yydra build`"),
            || match target {
                BuildTarget::Server => build_server(&root),
                BuildTarget::H5 => build_h5(&root),
                BuildTarget::Android => {
                    run(
                        &root.join("frontend"),
                        npm_program(),
                        &["run", "typecheck"],
                        "BUILD_FRONTEND_TYPECHECK_FAILED",
                    )?;
                    crate::android_build::build_android_artifact(&root)
                }
            },
        )?;
        reporter.emit(Diagnostic {
            phase,
            code: "BUILD_ARTIFACT_READY",
            severity: "info",
            status: "pass",
            message: "application artifact ready",
            location: Some(&artifact),
            remediation: None,
        });
    }
    Ok(())
}

fn build_server(root: &Path) -> Result<PathBuf> {
    let output = run(
        root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "crates/server/Cargo.toml",
            "--bin",
            "server",
            "--message-format=json",
        ],
        "BUILD_SERVER_FAILED",
    )?;
    let executable = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == "server"
        })
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .filter(|path| path.is_absolute() && path.is_file())
        .context(
            "BUILD_SERVER_OUTPUT_MISSING: Cargo did not report an existing server executable",
        )?;
    Ok(executable)
}

fn build_h5(root: &Path) -> Result<PathBuf> {
    let frontend = root.join("frontend");
    run(
        &frontend,
        npm_program(),
        &["run", "typecheck"],
        "BUILD_FRONTEND_TYPECHECK_FAILED",
    )?;
    run(
        &frontend,
        npm_program(),
        &["run", "export:h5"],
        "BUILD_H5_FAILED",
    )?;
    let output = frontend.join("dist");
    if !output.join("index.html").is_file() {
        bail!("BUILD_H5_OUTPUT_MISSING: H5 export did not produce dist/index.html");
    }
    Ok(output)
}

fn run(root: &Path, program: &str, arguments: &[&str], code: &str) -> Result<Output> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .with_context(|| format!("{code}: start {program}"))?;
    if !output.status.success() {
        bail!(
            "{code}: {program} exited with {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output)
}
