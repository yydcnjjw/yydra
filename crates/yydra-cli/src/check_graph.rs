// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    DISTRIBUTION_VERSION, MessageFormat, find_workspace_root, install_shutdown_handler,
    npm_program, template_source_files, verify_origin_authority, verify_snapshot_authorities,
};

#[cfg(windows)]
use crate::{WindowsJob, create_kill_on_close_job};

const RESULT_SCHEMA_VERSION: u64 = 1;
const INPUTS_UNCHANGED_NODE: &str = "ownership.authored-inputs-unchanged";

pub(crate) struct CheckRequest {
    pub(crate) workspace: PathBuf,
    pub(crate) evidence_dir: Option<PathBuf>,
    pub(crate) selected_nodes: Vec<String>,
}

#[derive(Clone, Copy)]
struct NodeSpec {
    id: &'static str,
    prerequisites: &'static [&'static str],
    remediation: &'static str,
    proves: &'static str,
    does_not_prove: &'static str,
}

const NODE_SPECS: &[NodeSpec] = &[
    NodeSpec {
        id: "origin.exact-distribution",
        prerequisites: &[],
        remediation: "restore the reviewed Workspace Origin Record and exact Distribution snapshots, or install the exact CLI version named by the record",
        proves: "the Workspace identity and normalized creation inputs name this exact packaged CLI Distribution",
        does_not_prove: "that product-owned source still matches the create-once template or that the Workspace passes other quality nodes",
    },
    NodeSpec {
        id: "ownership.baseline-skills",
        prerequisites: &["origin.exact-distribution"],
        remediation: "restore the exact .agents/skills path and byte inventory from this Distribution; do not hand-edit Baseline Skill snapshots",
        proves: "the local Baseline Skill path and byte inventory exactly matches the packaged Distribution snapshot",
        does_not_prove: "Skill activation, Agent behavior, Agent performance, or Baseline Skill effectiveness",
    },
    NodeSpec {
        id: "ownership.generated-snapshots",
        prerequisites: &["origin.exact-distribution"],
        remediation: "restore committed generated and exact snapshot authorities from reviewed version control; do not regenerate them inside check mode",
        proves: "the current Distribution-owned origin, inventory, provenance, and license snapshot authorities retain their exact bytes",
        does_not_prove: "future generated Public API or native-host drift owned by later graph nodes",
    },
    NodeSpec {
        id: "rust.architecture",
        prerequisites: &[],
        remediation: "restore Cargo.lock and remove the reported forbidden normal, development, build, or target-specific dependency edge",
        proves: "Cargo resolves from the committed lock and every declared dependency kind and target edge obeys the V0 workspace role graph without cycles",
        does_not_prove: "that business logic was not copied into an allowed transport or persistence module",
    },
    NodeSpec {
        id: "rust.format",
        prerequisites: &[],
        remediation: "run cargo fmt --all, review the diff, and rerun yydra check",
        proves: "Rust source is in the pinned formatter's canonical form",
        does_not_prove: "correctness, architecture, or runtime behavior",
    },
    NodeSpec {
        id: "rust.compile",
        prerequisites: &["rust.architecture"],
        remediation: "fix the reported locked all-target/all-feature Rust compilation error",
        proves: "the complete selected Rust workspace targets and features compile from Cargo.lock",
        does_not_prove: "test behavior, database availability, or H5 behavior",
    },
    NodeSpec {
        id: "rust.clippy",
        prerequisites: &["rust.compile"],
        remediation: "fix every reported Clippy warning without weakening the Distribution command",
        proves: "the selected all-target/all-feature Clippy baseline has zero warnings",
        does_not_prove: "absence of defects or compliance with rules outside the selected baseline",
    },
    NodeSpec {
        id: "rust.test",
        prerequisites: &["rust.compile"],
        remediation: "restore non-empty canonical Rust tests and fix the reported test failure",
        proves: "at least one canonical Rust test was discovered and all workspace all-feature/all-target tests passed",
        does_not_prove: "unrepresented product behavior or external-service conformance",
    },
    NodeSpec {
        id: "rust.doctest",
        prerequisites: &["rust.compile"],
        remediation: "fix the reported Rust documentation example failure",
        proves: "all applicable Rust documentation tests pass",
        does_not_prove: "that every public item has a documentation example",
    },
    NodeSpec {
        id: "frontend.lock",
        prerequisites: &[],
        remediation: "restore frontend/package.json and package-lock.json, then run yydra setup from the exact Distribution",
        proves: "npm can install the exact committed frontend resolution without rewriting its lock",
        does_not_prove: "advisory, provenance, or artifact license policy",
    },
    NodeSpec {
        id: "api.generated-contract",
        prerequisites: &["rust.compile", "frontend.lock"],
        remediation: "run `yydra generate api`, review every contract and Generated Client change, and commit the complete atomic output set",
        proves: "Rust route collection reproduces the normalized OpenAPI, Orval Fetch/TypeScript/Zod outputs, wire profile, and generation record without modifying the Workspace",
        does_not_prove: "that the running service returns every documented response or that Product Domain behavior is correct",
    },
    NodeSpec {
        id: "api.runtime-conformance",
        prerequisites: &["api.generated-contract"],
        remediation: "align the public-route handler status, content type, headers, and response body with the committed Public API Contract",
        proves: "the Framework-owned public router matches its collected contract and discriminating fixtures reject undocumented status, content type, and malformed bodies",
        does_not_prove: "exhaustive generated-input coverage, authentication policy, or database-backed Product Domain behavior",
    },
    NodeSpec {
        id: "frontend.format",
        prerequisites: &["frontend.lock"],
        remediation: "run the pinned frontend formatter, review the diff, and rerun yydra check",
        proves: "frontend authored source is in the pinned formatter's canonical form",
        does_not_prove: "type safety, lint correctness, or runtime behavior",
    },
    NodeSpec {
        id: "frontend.lint",
        prerequisites: &["frontend.lock"],
        remediation: "fix every reported frontend lint warning without weakening the Distribution command",
        proves: "the selected frontend lint baseline completes with zero warnings",
        does_not_prove: "absence of defects or accessibility conformance",
    },
    NodeSpec {
        id: "frontend.typecheck",
        prerequisites: &["frontend.lock"],
        remediation: "fix the strict no-emit TypeScript errors",
        proves: "the complete frontend TypeScript project passes strict type checking without emit",
        does_not_prove: "browser behavior or server contract conformance",
    },
    NodeSpec {
        id: "frontend.test",
        prerequisites: &["frontend.lock"],
        remediation: "restore non-empty canonical Vitest coverage and fix the reported failure",
        proves: "the canonical frontend test runner discovered tests and all selected tests passed",
        does_not_prove: "production H5 behavior against the real service",
    },
    NodeSpec {
        id: "api.client-contract",
        prerequisites: &["api.generated-contract", "frontend.typecheck"],
        remediation: "restore the generated runtime schemas and fix the handwritten Framework facade without importing Generated Client internals from Product code",
        proves: "the handwritten facade injects transport concerns and classifies declared Problems, transport, caller cancellation, timeout, and malformed or undocumented responses",
        does_not_prove: "browser or native runtime behavior against a deployed service",
    },
    NodeSpec {
        id: "infrastructure.docker",
        prerequisites: &[],
        remediation: "install Docker with Compose support and make its daemon available, then rerun yydra check",
        proves: "the required container engine and Compose client are reachable for this invocation",
        does_not_prove: "that PostgreSQL, the backend, or H5 semantics will pass",
    },
    NodeSpec {
        id: "infrastructure.playwright-chromium",
        prerequisites: &["frontend.lock"],
        remediation: "install the pinned Playwright Chromium browser for this host, then rerun yydra check",
        proves: "the pinned Playwright package resolves to an installed Chromium executable on this host",
        does_not_prove: "that the production H5 export or its browser assertions will pass",
    },
    NodeSpec {
        id: "h5.real-runtime",
        prerequisites: &[
            "rust.compile",
            "frontend.typecheck",
            "infrastructure.docker",
            "infrastructure.playwright-chromium",
        ],
        remediation: "inspect the node log, run yydra db migrate and the focused production H5 test, then fix the first semantic failure",
        proves: "the production H5 export reaches a real Axum service and PostgreSQL after a browser refresh",
        does_not_prove: "Android runtime, physical-device behavior, native accessibility, or complete WCAG conformance",
    },
    NodeSpec {
        id: INPUTS_UNCHANGED_NODE,
        prerequisites: &[],
        remediation: "restore every changed authored, snapshot, generated, lock, migration, and configuration input; check mode must remain read-only",
        proves: "all in-scope Workspace input paths and bytes are identical before and after this check invocation",
        does_not_prove: "bit-for-bit build-output reproducibility or absence of external side effects",
    },
];

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Pass,
    Fail,
    InfrastructureError,
    Skipped,
    NotRun,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckCause {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependency_node_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeResult {
    schema_version: u64,
    event: &'static str,
    node_id: String,
    prerequisites: Vec<String>,
    outcome: Outcome,
    duration_ms: u64,
    cause: Option<CheckCause>,
    remediation: Option<String>,
    proves: String,
    does_not_prove: String,
    commands: Vec<String>,
    tool_versions: BTreeMap<String, String>,
    log: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckManifest {
    schema_version: u64,
    distribution_version: &'static str,
    cli_version: &'static str,
    rule_schema_version: u64,
    workspace: String,
    profile: &'static str,
    scope: &'static str,
    catalog_nodes: Vec<String>,
    selected_nodes: Vec<String>,
    complete: bool,
    aggregate_conformance: bool,
    status: String,
    input_digest: String,
    required_tool_versions: BTreeMap<String, String>,
    observed_tool_versions: BTreeMap<String, String>,
    seeds: Vec<String>,
    acknowledgements: Vec<String>,
    exceptions: Vec<String>,
    artifacts: Vec<ArtifactEvidence>,
    nodes: Vec<NodeResult>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactEvidence {
    path: String,
    sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SummaryEvent<'a> {
    schema_version: u64,
    event: &'static str,
    status: &'a str,
    scope: &'static str,
    complete: bool,
    aggregate_conformance: bool,
    evidence: String,
}

#[derive(Debug)]
struct NodeFailure {
    outcome: Outcome,
    code: &'static str,
    message: String,
}

impl NodeFailure {
    fn fail(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::Fail,
            code,
            message: message.into(),
        }
    }

    fn infrastructure(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::InfrastructureError,
            code,
            message: message.into(),
        }
    }
}

struct NodeContext<'a> {
    root: &'a Path,
    evidence_root: &'a Path,
    shutdown: &'a AtomicBool,
    log: File,
    commands: Vec<String>,
    tool_versions: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InputKind {
    Directory,
    File,
    Symlink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InputEntry {
    kind: InputKind,
    bytes: Vec<u8>,
}

type InputInventory = BTreeMap<PathBuf, InputEntry>;

struct InputBaselines<'a> {
    original_root: &'a Path,
    original: &'a InputInventory,
    execution: &'a InputInventory,
    scratch_root: &'a Path,
}

pub(crate) fn check(request: CheckRequest, format: MessageFormat) -> Result<()> {
    let root = find_workspace_root(&request.workspace)?;
    let evidence_root = evidence_root(&root, request.evidence_dir)?;
    create_private_dir_all(&evidence_root).with_context(|| {
        format!(
            "create private check evidence directory '{}'",
            evidence_root.display()
        )
    })?;
    create_private_dir_all(&evidence_root.join("logs"))?;
    create_private_dir_all(&evidence_root.join("artifacts"))?;

    let selected = selected_specs(&request.selected_nodes)?;
    let complete = request.selected_nodes.is_empty();
    let diagnostics_path = evidence_root.join("diagnostics.jsonl");
    let mut diagnostics = create_private_file(&diagnostics_path)?;
    let original_inputs = match workspace_inputs(&root) {
        Ok(inputs) => inputs,
        Err(failure) => {
            return finish_preflight_failure(
                PreflightReport {
                    root: &root,
                    evidence_root: &evidence_root,
                    diagnostics: &mut diagnostics,
                    requested_nodes: &request.selected_nodes,
                    selected: &selected,
                    complete,
                    format,
                },
                failure,
                None,
            );
        }
    };
    let scratch_root = evidence_root.join("scratch");
    let execution_root = scratch_root.join("workspace");
    create_private_dir_all(&execution_root)?;
    if let Err(failure) = copy_workspace_inputs(&root, &execution_root, &original_inputs) {
        let failure = preflight_cleanup_failure(&scratch_root, failure);
        return finish_preflight_failure(
            PreflightReport {
                root: &root,
                evidence_root: &evidence_root,
                diagnostics: &mut diagnostics,
                requested_nodes: &request.selected_nodes,
                selected: &selected,
                complete,
                format,
            },
            failure,
            Some(&original_inputs),
        );
    }
    let execution_inputs = match workspace_inputs(&execution_root) {
        Ok(inputs) => inputs,
        Err(failure) => {
            let failure = preflight_cleanup_failure(&scratch_root, failure);
            return finish_preflight_failure(
                PreflightReport {
                    root: &root,
                    evidence_root: &evidence_root,
                    diagnostics: &mut diagnostics,
                    requested_nodes: &request.selected_nodes,
                    selected: &selected,
                    complete,
                    format,
                },
                failure,
                Some(&original_inputs),
            );
        }
    };
    let baselines = InputBaselines {
        original_root: &root,
        original: &original_inputs,
        execution: &execution_inputs,
        scratch_root: &scratch_root,
    };
    let shutdown = install_shutdown_handler().context("install check shutdown handler")?;
    let mut results = Vec::new();

    for spec in NODE_SPECS {
        if !selected.contains(spec.id) {
            let result = not_run_result(*spec, &evidence_root)?;
            emit_node(&result, format);
            serde_json::to_writer(&mut diagnostics, &result)?;
            diagnostics.write_all(b"\n")?;
            diagnostics.flush()?;
            results.push(result);
            continue;
        }
        let result = if let Some(dependency) = spec.prerequisites.iter().find(|dependency| {
            results.iter().any(|result: &NodeResult| {
                result.node_id == **dependency && result.outcome != Outcome::Pass
            })
        }) {
            skipped_result(*spec, dependency, &evidence_root)?
        } else {
            execute_node(
                *spec,
                &execution_root,
                &evidence_root,
                &shutdown,
                &baselines,
            )?
        };
        emit_node(&result, format);
        serde_json::to_writer(&mut diagnostics, &result)?;
        diagnostics.write_all(b"\n")?;
        diagnostics.flush()?;
        results.push(result);
    }

    let failed = results.iter().any(|result| {
        selected.contains(result.node_id.as_str()) && result.outcome != Outcome::Pass
    });
    let status = if failed {
        "fail"
    } else if complete {
        "pass-core"
    } else {
        "pass-selected"
    };
    diagnostics.flush()?;
    let artifacts = vec![
        artifact_evidence(&evidence_root, &evidence_root.join("logs"))?,
        artifact_evidence(&evidence_root, &evidence_root.join("artifacts"))?,
        artifact_evidence(&evidence_root, &diagnostics_path)?,
    ];
    let manifest = CheckManifest {
        schema_version: RESULT_SCHEMA_VERSION,
        distribution_version: DISTRIBUTION_VERSION,
        cli_version: DISTRIBUTION_VERSION,
        rule_schema_version: RESULT_SCHEMA_VERSION,
        workspace: root.display().to_string(),
        profile: "clean-core-local",
        scope: "clean-core-local",
        catalog_nodes: NODE_SPECS.iter().map(|spec| spec.id.to_owned()).collect(),
        selected_nodes: request.selected_nodes,
        complete,
        aggregate_conformance: false,
        status: status.to_owned(),
        input_digest: input_inventory_digest(&original_inputs),
        required_tool_versions: required_tool_versions(),
        observed_tool_versions: observed_tool_versions(&results),
        seeds: Vec::new(),
        acknowledgements: Vec::new(),
        exceptions: Vec::new(),
        artifacts,
        nodes: results,
    };
    let manifest_path = evidence_root.join("manifest.json");
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    manifest_bytes.push(b'\n');
    let mut manifest_file = create_private_file(&manifest_path)?;
    manifest_file.write_all(&manifest_bytes)?;
    manifest_file.flush()?;
    emit_summary(status, complete, &manifest_path, format);
    if failed {
        bail!(
            "Mechanical Quality Contract failed; inspect '{}'",
            manifest_path.display()
        );
    }
    Ok(())
}

fn selected_specs(requested: &[String]) -> Result<BTreeSet<&'static str>> {
    if requested.is_empty() {
        return Ok(NODE_SPECS.iter().map(|spec| spec.id).collect());
    }
    let mut selected = BTreeSet::new();
    for id in requested {
        if !NODE_SPECS.iter().any(|spec| spec.id == id) {
            bail!("unknown Mechanical Quality Contract node '{id}'");
        }
        include_with_prerequisites(id, &mut selected);
    }
    selected.insert(INPUTS_UNCHANGED_NODE);
    Ok(selected)
}

fn include_with_prerequisites(id: &str, selected: &mut BTreeSet<&'static str>) {
    let spec = NODE_SPECS
        .iter()
        .find(|spec| spec.id == id)
        .expect("selected node was validated");
    if !selected.insert(spec.id) {
        return;
    }
    for prerequisite in spec.prerequisites {
        include_with_prerequisites(prerequisite, selected);
    }
}

fn evidence_root(root: &Path, requested: Option<PathBuf>) -> Result<PathBuf> {
    let path = if let Some(requested) = requested {
        if requested.is_absolute() {
            requested
        } else {
            root.join(requested)
        }
    } else {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_nanos();
        std::env::temp_dir().join(format!("yydra-check-{}-{nonce}", std::process::id()))
    };
    let path = lexical_normalize(&path)?;
    if path.exists() {
        bail!(
            "evidence directory '{}' already exists; choose a new path",
            path.display()
        );
    }
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("canonicalize Product Workspace '{}'", root.display()))?;
    if path.starts_with(&canonical_root) {
        bail!(
            "evidence directory '{}' must be outside the Product Workspace",
            path.display()
        );
    }
    validate_external_path(&path, &canonical_root)?;
    Ok(path)
}

fn lexical_normalize(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    bail!("path '{}' escapes its filesystem root", path.display());
                }
            }
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::Normal(_) => normalized.push(component.as_os_str()),
        }
    }
    if !normalized.is_absolute() {
        bail!(
            "path '{}' did not resolve to an absolute path",
            path.display()
        );
    }
    Ok(normalized)
}

fn validate_external_path(path: &Path, workspace_root: &Path) -> Result<()> {
    for ancestor in path.ancestors().skip(1) {
        if !ancestor.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor)
            .with_context(|| format!("inspect evidence ancestor '{}'", ancestor.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "evidence directory '{}' has symlink ancestor '{}'",
                path.display(),
                ancestor.display()
            );
        }
        let canonical = ancestor
            .canonicalize()
            .with_context(|| format!("canonicalize evidence ancestor '{}'", ancestor.display()))?;
        if canonical.starts_with(workspace_root) {
            bail!(
                "evidence directory '{}' resolves inside the Product Workspace",
                path.display()
            );
        }
    }
    Ok(())
}

fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn create_private_file(path: &Path) -> std::io::Result<File> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

fn skipped_result(spec: NodeSpec, dependency: &str, evidence_root: &Path) -> Result<NodeResult> {
    let message = format!("prerequisite {dependency} did not pass");
    let log_path = evidence_root.join("logs").join(format!("{}.log", spec.id));
    let mut log = create_private_file(&log_path)?;
    writeln!(log, "skipped: {message}")?;
    log.flush()?;
    Ok(NodeResult {
        schema_version: RESULT_SCHEMA_VERSION,
        event: "check-node",
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome: Outcome::Skipped,
        duration_ms: 0,
        cause: Some(CheckCause {
            code: "CHECK_PREREQUISITE_FAILED".to_owned(),
            message,
            dependency_node_id: Some(dependency.to_string()),
        }),
        remediation: Some(spec.remediation.to_owned()),
        proves: spec.proves.to_owned(),
        does_not_prove: spec.does_not_prove.to_owned(),
        commands: Vec::new(),
        tool_versions: BTreeMap::new(),
        log: relative_log(&log_path, evidence_root),
    })
}

fn not_run_result(spec: NodeSpec, evidence_root: &Path) -> Result<NodeResult> {
    let message = "node was not selected by this diagnostic plan";
    let log_path = evidence_root.join("logs").join(format!("{}.log", spec.id));
    let mut log = create_private_file(&log_path)?;
    writeln!(log, "not-run: {message}")?;
    log.flush()?;
    Ok(NodeResult {
        schema_version: RESULT_SCHEMA_VERSION,
        event: "check-node",
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome: Outcome::NotRun,
        duration_ms: 0,
        cause: Some(CheckCause {
            code: "CHECK_NOT_SELECTED".to_owned(),
            message: message.to_owned(),
            dependency_node_id: None,
        }),
        remediation: None,
        proves: spec.proves.to_owned(),
        does_not_prove: spec.does_not_prove.to_owned(),
        commands: Vec::new(),
        tool_versions: BTreeMap::new(),
        log: relative_log(&log_path, evidence_root),
    })
}

struct PreflightReport<'a> {
    root: &'a Path,
    evidence_root: &'a Path,
    diagnostics: &'a mut File,
    requested_nodes: &'a [String],
    selected: &'a BTreeSet<&'static str>,
    complete: bool,
    format: MessageFormat,
}

fn finish_preflight_failure(
    report: PreflightReport<'_>,
    failure: NodeFailure,
    original_inputs: Option<&InputInventory>,
) -> Result<()> {
    let PreflightReport {
        root,
        evidence_root,
        diagnostics,
        requested_nodes,
        selected,
        complete,
        format,
    } = report;
    let mut failure = Some(failure);
    let mut results = Vec::new();
    for spec in NODE_SPECS {
        let result = if spec.id == INPUTS_UNCHANGED_NODE {
            preflight_failure_result(
                *spec,
                failure.take().expect("preflight failure is emitted once"),
                evidence_root,
            )?
        } else if selected.contains(spec.id) {
            preflight_not_run_result(*spec, evidence_root)?
        } else {
            not_run_result(*spec, evidence_root)?
        };
        emit_node(&result, format);
        serde_json::to_writer(&mut *diagnostics, &result)?;
        diagnostics.write_all(b"\n")?;
        diagnostics.flush()?;
        results.push(result);
    }

    let diagnostics_path = evidence_root.join("diagnostics.jsonl");
    let artifacts = vec![
        artifact_evidence(evidence_root, &evidence_root.join("logs"))?,
        artifact_evidence(evidence_root, &evidence_root.join("artifacts"))?,
        artifact_evidence(evidence_root, &diagnostics_path)?,
    ];
    let manifest = CheckManifest {
        schema_version: RESULT_SCHEMA_VERSION,
        distribution_version: DISTRIBUTION_VERSION,
        cli_version: DISTRIBUTION_VERSION,
        rule_schema_version: RESULT_SCHEMA_VERSION,
        workspace: root.display().to_string(),
        profile: "clean-core-local",
        scope: "clean-core-local",
        catalog_nodes: NODE_SPECS.iter().map(|spec| spec.id.to_owned()).collect(),
        selected_nodes: requested_nodes.to_vec(),
        complete,
        aggregate_conformance: false,
        status: "fail".to_owned(),
        input_digest: original_inputs
            .map(input_inventory_digest)
            .unwrap_or_else(|| "unavailable".to_owned()),
        required_tool_versions: required_tool_versions(),
        observed_tool_versions: BTreeMap::new(),
        seeds: Vec::new(),
        acknowledgements: Vec::new(),
        exceptions: Vec::new(),
        artifacts,
        nodes: results,
    };
    let manifest_path = evidence_root.join("manifest.json");
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    let mut manifest_file = create_private_file(&manifest_path)?;
    manifest_file.write_all(&bytes)?;
    manifest_file.flush()?;
    emit_summary("fail", complete, &manifest_path, format);
    bail!(
        "Mechanical Quality Contract preflight failed; inspect '{}'",
        manifest_path.display()
    )
}

fn preflight_failure_result(
    spec: NodeSpec,
    failure: NodeFailure,
    evidence_root: &Path,
) -> Result<NodeResult> {
    let log_path = evidence_root.join("logs").join(format!("{}.log", spec.id));
    let mut log = create_private_file(&log_path)?;
    writeln!(log, "{}: {}", failure.code, failure.message)?;
    log.flush()?;
    Ok(NodeResult {
        schema_version: RESULT_SCHEMA_VERSION,
        event: "check-node",
        node_id: spec.id.to_owned(),
        prerequisites: Vec::new(),
        outcome: failure.outcome,
        duration_ms: 0,
        cause: Some(CheckCause {
            code: failure.code.to_owned(),
            message: failure.message,
            dependency_node_id: None,
        }),
        remediation: Some(spec.remediation.to_owned()),
        proves: spec.proves.to_owned(),
        does_not_prove: spec.does_not_prove.to_owned(),
        commands: Vec::new(),
        tool_versions: BTreeMap::new(),
        log: relative_log(&log_path, evidence_root),
    })
}

fn preflight_not_run_result(spec: NodeSpec, evidence_root: &Path) -> Result<NodeResult> {
    let message = "node was not run because the isolated execution preflight failed";
    let log_path = evidence_root.join("logs").join(format!("{}.log", spec.id));
    let mut log = create_private_file(&log_path)?;
    writeln!(log, "not-run: {message}")?;
    log.flush()?;
    Ok(NodeResult {
        schema_version: RESULT_SCHEMA_VERSION,
        event: "check-node",
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome: Outcome::NotRun,
        duration_ms: 0,
        cause: Some(CheckCause {
            code: "CHECK_PREFLIGHT_FAILED".to_owned(),
            message: message.to_owned(),
            dependency_node_id: Some(INPUTS_UNCHANGED_NODE.to_owned()),
        }),
        remediation: Some(
            "resolve the isolated execution preflight failure, then rerun this node".to_owned(),
        ),
        proves: spec.proves.to_owned(),
        does_not_prove: spec.does_not_prove.to_owned(),
        commands: Vec::new(),
        tool_versions: BTreeMap::new(),
        log: relative_log(&log_path, evidence_root),
    })
}

fn preflight_cleanup_failure(scratch_root: &Path, failure: NodeFailure) -> NodeFailure {
    if !scratch_root.exists() {
        return failure;
    }
    match fs::remove_dir_all(scratch_root) {
        Ok(()) => failure,
        Err(error) => NodeFailure::infrastructure(
            "CHECK_SCRATCH_CLEANUP_FAILED",
            format!(
                "{}: {}; additionally could not remove '{}': {error}",
                failure.code,
                failure.message,
                scratch_root.display()
            ),
        ),
    }
}

fn execute_node(
    spec: NodeSpec,
    root: &Path,
    evidence_root: &Path,
    shutdown: &AtomicBool,
    baselines: &InputBaselines<'_>,
) -> Result<NodeResult> {
    let log_path = evidence_root.join("logs").join(format!("{}.log", spec.id));
    let log = create_private_file(&log_path)?;
    let mut context = NodeContext {
        root,
        evidence_root,
        shutdown,
        log,
        commands: Vec::new(),
        tool_versions: BTreeMap::new(),
    };
    let started = Instant::now();
    let execution = match spec.id {
        "origin.exact-distribution" => verify_origin_authority(root)
            .map_err(|error| NodeFailure::fail("ORIGIN_AUTHORITY_DRIFT", format!("{error:#}"))),
        "ownership.baseline-skills" => check_baseline_skills(root),
        "ownership.generated-snapshots" => verify_snapshot_authorities(root).map_err(|error| {
            NodeFailure::fail("GENERATED_SNAPSHOT_DRIFT", format!("{error:#}"))
        }),
        "rust.architecture" => check_rust_architecture(&mut context),
        "rust.format" => context.command(
            root,
            "cargo",
            &["fmt", "--all", "--", "--check"],
            &[],
            "RUST_FORMAT_FAILED",
        ),
        "rust.compile" => context.command(
            root,
            "cargo",
            &[
                "check",
                "--locked",
                "--workspace",
                "--all-targets",
                "--all-features",
            ],
            &[],
            "RUST_COMPILE_FAILED",
        ),
        "rust.clippy" => context.command(
            root,
            "cargo",
            &[
                "clippy",
                "--locked",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
            &[],
            "RUST_CLIPPY_FAILED",
        ),
        "rust.test" => check_rust_tests(&mut context),
        "rust.doctest" => context.command(
            root,
            "cargo",
            &["test", "--locked", "--workspace", "--all-features", "--doc"],
            &[],
            "RUST_DOCTEST_FAILED",
        ),
        "frontend.lock" => check_frontend_lock(&mut context),
        "api.generated-contract" => check_api_generated_contract(&mut context),
        "api.runtime-conformance" => check_api_runtime_conformance(&mut context),
        "frontend.format" => check_frontend_format(&mut context),
        "frontend.lint" => check_frontend_lint(&mut context),
        "frontend.typecheck" => check_frontend_typecheck(&mut context),
        "frontend.test" => check_frontend_tests(&mut context),
        "api.client-contract" => check_api_client_contract(&mut context),
        "infrastructure.docker" => check_docker(&mut context),
        "infrastructure.playwright-chromium" => context.infrastructure_command(
            &root.join("frontend"),
            "node",
            &[
                "-e",
                "const fs=require('node:fs');const {chromium}=require('playwright');const path=chromium.executablePath();if(!fs.existsSync(path)){console.error(`missing Chromium at ${path}`);process.exit(2)}",
            ],
            &[],
            "PLAYWRIGHT_CHROMIUM_UNAVAILABLE",
        ),
        "h5.real-runtime" => check_h5_runtime(&mut context),
        INPUTS_UNCHANGED_NODE => check_and_remove_scratch(root, baselines),
        _ => unreachable!("all node specs have an implementation"),
    };
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let execution = match execution {
        Err(failure) => match writeln!(context.log, "{}: {}", failure.code, failure.message) {
            Ok(()) => Err(failure),
            Err(error) => Err(NodeFailure::infrastructure(
                "CHECK_EVIDENCE_WRITE_FAILED",
                error.to_string(),
            )),
        },
        success => success,
    };
    let execution = match context.log.flush() {
        Ok(()) => execution,
        Err(error) => Err(NodeFailure::infrastructure(
            "CHECK_EVIDENCE_WRITE_FAILED",
            error.to_string(),
        )),
    };
    let (outcome, cause, remediation) = match execution {
        Ok(()) => (Outcome::Pass, None, None),
        Err(failure) => (
            failure.outcome,
            Some(CheckCause {
                code: failure.code.to_owned(),
                message: failure.message,
                dependency_node_id: None,
            }),
            Some(spec.remediation.to_owned()),
        ),
    };
    Ok(NodeResult {
        schema_version: RESULT_SCHEMA_VERSION,
        event: "check-node",
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome,
        duration_ms,
        cause,
        remediation,
        proves: spec.proves.to_owned(),
        does_not_prove: spec.does_not_prove.to_owned(),
        commands: context.commands,
        tool_versions: context.tool_versions,
        log: relative_log(&log_path, evidence_root),
    })
}

impl NodeContext<'_> {
    fn observe_tool_version(
        &mut self,
        directory: &Path,
        name: &str,
        program: &str,
        arguments: &[&str],
    ) -> std::result::Result<String, NodeFailure> {
        let output = self.capture(directory, program, arguments, &[])?;
        if !output.status.success() {
            return Err(NodeFailure::infrastructure(
                "CHECK_TOOL_VERSION_UNAVAILABLE",
                format!(
                    "{program} {} could not report its version",
                    arguments.join(" ")
                ),
            ));
        }
        let version = String::from_utf8(output.stdout)
            .map_err(|error| {
                NodeFailure::infrastructure("CHECK_TOOL_VERSION_INVALID", error.to_string())
            })?
            .trim()
            .to_owned();
        if version.is_empty() {
            return Err(NodeFailure::infrastructure(
                "CHECK_TOOL_VERSION_INVALID",
                format!(
                    "{program} {} reported an empty version",
                    arguments.join(" ")
                ),
            ));
        }
        self.tool_versions.insert(name.to_owned(), version.clone());
        Ok(version)
    }

    fn observe_exact_tool_version(
        &mut self,
        directory: &Path,
        name: &str,
        program: &str,
        arguments: &[&str],
        expected: &str,
    ) -> std::result::Result<(), NodeFailure> {
        let actual = self.observe_tool_version(directory, name, program, arguments)?;
        if actual != expected {
            return Err(NodeFailure::fail(
                "CHECK_TOOL_VERSION_MISMATCH",
                format!("{name} reported {actual:?}; exact Distribution authority is {expected}"),
            ));
        }
        Ok(())
    }

    fn command(
        &mut self,
        directory: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
        failure_code: &'static str,
    ) -> std::result::Result<(), NodeFailure> {
        let output = self.capture(directory, program, arguments, environment)?;
        if !output.status.success() {
            return Err(NodeFailure::fail(
                failure_code,
                format!(
                    "{program} {} exited with {}",
                    arguments.join(" "),
                    output
                        .status
                        .code()
                        .map_or_else(|| "signal".to_owned(), |code| code.to_string())
                ),
            ));
        }
        Ok(())
    }

    fn infrastructure_command(
        &mut self,
        directory: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
        failure_code: &'static str,
    ) -> std::result::Result<(), NodeFailure> {
        let output = self.capture(directory, program, arguments, environment)?;
        if !output.status.success() {
            return Err(NodeFailure::infrastructure(
                failure_code,
                format!(
                    "{program} {} exited with {}",
                    arguments.join(" "),
                    output
                        .status
                        .code()
                        .map_or_else(|| "signal".to_owned(), |code| code.to_string())
                ),
            ));
        }
        Ok(())
    }

    fn capture(
        &mut self,
        directory: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> std::result::Result<std::process::Output, NodeFailure> {
        self.capture_with_policy(directory, program, arguments, environment, true, None)
    }

    fn cleanup_command(
        &mut self,
        directory: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
        failure_code: &'static str,
    ) -> std::result::Result<(), NodeFailure> {
        let output = self.capture_with_policy(
            directory,
            program,
            arguments,
            environment,
            false,
            Some(Duration::from_secs(30)),
        )?;
        if !output.status.success() {
            return Err(NodeFailure::infrastructure(
                failure_code,
                format!(
                    "{program} {} exited with {}",
                    arguments.join(" "),
                    output
                        .status
                        .code()
                        .map_or_else(|| "signal".to_owned(), |code| code.to_string())
                ),
            ));
        }
        Ok(())
    }

    fn capture_with_policy(
        &mut self,
        directory: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
        respect_shutdown: bool,
        timeout: Option<Duration>,
    ) -> std::result::Result<std::process::Output, NodeFailure> {
        let display = display_command(
            self.root,
            self.evidence_root,
            directory,
            program,
            arguments,
            environment,
        );
        self.commands.push(display.clone());
        writeln!(self.log, "$ {display}").map_err(evidence_write_failure)?;
        self.log.flush().map_err(evidence_write_failure)?;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                NodeFailure::infrastructure("CHECK_CLOCK_UNAVAILABLE", error.to_string())
            })?
            .as_nanos();
        let output_root = self
            .evidence_root
            .join("artifacts")
            .join(format!(".command-{}-{nonce}", std::process::id()));
        create_private_dir_all(&output_root).map_err(evidence_write_failure)?;
        let stdout_path = output_root.join("stdout");
        let stderr_path = output_root.join("stderr");
        let stdout = create_private_file(&stdout_path).map_err(evidence_write_failure)?;
        let stderr = create_private_file(&stderr_path).map_err(evidence_write_failure)?;

        let mut command = sanitized_command(program);
        command
            .args(arguments)
            .envs(environment.iter().copied())
            .current_dir(directory)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        #[cfg(unix)]
        command.process_group(0);
        let mut child = CapturedChild::spawn(command).map_err(|error| {
            NodeFailure::infrastructure(
                "CHECK_TOOL_UNAVAILABLE",
                format!("could not start {program} {}: {error}", arguments.join(" ")),
            )
        })?;
        let started = Instant::now();
        let status = loop {
            if respect_shutdown && self.shutdown.load(Ordering::SeqCst) {
                child.terminate();
                return Err(NodeFailure::infrastructure(
                    "CHECK_CANCELLED",
                    format!("cancelled while running {program} {}", arguments.join(" ")),
                ));
            }
            if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
                child.terminate();
                return Err(NodeFailure::infrastructure(
                    "CHECK_CLEANUP_TIMEOUT",
                    format!("timed out cleaning up {program} {}", arguments.join(" ")),
                ));
            }
            match child.child.try_wait() {
                Ok(Some(status)) => {
                    child.disarm();
                    break status;
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    child.terminate();
                    return Err(NodeFailure::infrastructure(
                        "CHECK_TOOL_POLL_FAILED",
                        format!("poll {program}: {error}"),
                    ));
                }
            }
        };
        let stdout = fs::read(&stdout_path).map_err(evidence_write_failure)?;
        let stderr = fs::read(&stderr_path).map_err(evidence_write_failure)?;
        self.log
            .write_all(&stdout)
            .map_err(evidence_write_failure)?;
        self.log
            .write_all(&stderr)
            .map_err(evidence_write_failure)?;
        self.log.flush().map_err(evidence_write_failure)?;
        fs::remove_dir_all(&output_root).map_err(evidence_write_failure)?;
        Ok(std::process::Output {
            status,
            stdout,
            stderr,
        })
    }
}

fn evidence_write_failure(error: impl std::fmt::Display) -> NodeFailure {
    NodeFailure::infrastructure("CHECK_EVIDENCE_WRITE_FAILED", error.to_string())
}

struct CapturedChild {
    child: Child,
    armed: bool,
    #[cfg(windows)]
    job: Option<WindowsJob>,
}

impl CapturedChild {
    fn spawn(mut command: Command) -> std::io::Result<Self> {
        #[allow(unused_mut)]
        let mut child = command.spawn()?;
        #[cfg(windows)]
        let job = match create_kill_on_close_job(&child) {
            Ok(job) => Some(job),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::other(format!(
                    "place child in a kill-on-close Windows Job Object: {error:#}"
                )));
            }
        };
        Ok(Self {
            child,
            armed: true,
            #[cfg(windows)]
            job,
        })
    }

    fn disarm(&mut self) {
        self.armed = false;
        #[cfg(windows)]
        drop(self.job.take());
    }

    fn terminate(&mut self) {
        if !self.armed {
            return;
        }
        #[cfg(windows)]
        {
            drop(self.job.take());
            let _ = self.child.wait();
        }
        #[cfg(not(windows))]
        terminate_server(&mut self.child);
        self.armed = false;
    }
}

impl Drop for CapturedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn check_baseline_skills(root: &Path) -> std::result::Result<(), NodeFailure> {
    let prefix = ".agents/skills/";
    let expected = template_source_files()
        .into_iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix(prefix)
                .map(|path| (PathBuf::from(path), bytes))
        })
        .map(|(path, bytes)| {
            let source = String::from_utf8_lossy(bytes)
                .replace("__YYDRA_DISTRIBUTION_VERSION__", DISTRIBUTION_VERSION)
                .into_bytes();
            (path, source)
        })
        .collect::<BTreeMap<_, _>>();
    let skills_root = root.join(prefix);
    let actual = if skills_root.exists() {
        plain_tree(&skills_root)?
    } else {
        BTreeMap::new()
    };
    if actual != expected {
        return Err(NodeFailure::fail(
            "BASELINE_SKILL_INVENTORY_DRIFT",
            describe_input_drift(&expected, &actual),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: String,
    dependencies: Vec<MetadataDependency>,
}

#[derive(Deserialize)]
struct MetadataDependency {
    name: String,
    kind: Option<String>,
    target: Option<String>,
}

fn check_rust_architecture(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    check_rust_toolchain_authority(context.root)?;
    let rustc = context.observe_tool_version(context.root, "rustc", "rustc", &["--version"])?;
    let cargo = context.observe_tool_version(context.root, "cargo", "cargo", &["--version"])?;
    let rustfmt =
        context.observe_tool_version(context.root, "rustfmt", "rustfmt", &["--version"])?;
    let clippy =
        context.observe_tool_version(context.root, "clippy", "cargo", &["clippy", "--version"])?;
    if !rustc.starts_with("rustc 1.97.1 ")
        || !cargo.starts_with("cargo 1.97.1 ")
        || !rustfmt.starts_with("rustfmt 1.9.0-stable ")
        || !clippy.starts_with("clippy 0.1.97 ")
    {
        return Err(NodeFailure::fail(
            "RUST_TOOLCHAIN_AUTHORITY_DRIFT",
            format!(
                "expected rustc/cargo 1.97.1, rustfmt 1.9.0-stable, and clippy 0.1.97; observed {rustc:?}, {cargo:?}, {rustfmt:?}, and {clippy:?}"
            ),
        ));
    }
    for (name, version) in [
        ("rust-toolchain", "1.97.1"),
        ("rustc", "1.97.1"),
        ("cargo", "1.97.1"),
        ("rustfmt", "1.9.0-stable"),
        ("clippy", "0.1.97"),
    ] {
        context
            .tool_versions
            .insert(name.to_owned(), version.to_owned());
    }
    let lock_path = context.root.join("Cargo.lock");
    let before_lock = fs::read(&lock_path).map_err(|error| {
        NodeFailure::fail(
            "CARGO_LOCK_DRIFT",
            format!("read '{}': {error}", lock_path.display()),
        )
    })?;
    let metadata_output = context.capture(
        context.root,
        "cargo",
        &["metadata", "--no-deps", "--format-version", "1"],
        &[],
    )?;
    if !metadata_output.status.success() {
        let message = String::from_utf8_lossy(&metadata_output.stderr)
            .trim()
            .to_owned();
        if message.contains("cyclic package dependency") {
            return Err(NodeFailure::fail("ARCH_DEPENDENCY_CYCLE", message));
        }
        return Err(NodeFailure::fail("ARCH_METADATA_INVALID", message));
    }
    let after_metadata_lock = fs::read(&lock_path).unwrap_or_default();
    if after_metadata_lock != before_lock {
        return Err(NodeFailure::fail(
            "CARGO_LOCK_DRIFT",
            "cargo metadata would rewrite the committed Cargo.lock",
        ));
    }
    let metadata: CargoMetadata =
        serde_json::from_slice(&metadata_output.stdout).map_err(|error| {
            NodeFailure::fail(
                "ARCH_METADATA_INVALID",
                format!("parse cargo metadata: {error}"),
            )
        })?;
    validate_architecture(&metadata, context.root)?;
    let locked = context.capture(
        context.root,
        "cargo",
        &["metadata", "--locked", "--format-version", "1"],
        &[],
    )?;
    if !locked.status.success() {
        return Err(NodeFailure::fail(
            "CARGO_LOCK_DRIFT",
            String::from_utf8_lossy(&locked.stderr).trim().to_owned(),
        ));
    }
    Ok(())
}

fn check_rust_toolchain_authority(root: &Path) -> std::result::Result<(), NodeFailure> {
    let expected = template_source_files()
        .into_iter()
        .find_map(|(path, bytes)| (path == "rust-toolchain.toml").then_some(bytes))
        .expect("packaged Rust toolchain authority is embedded");
    let path = root.join("rust-toolchain.toml");
    let actual = fs::read(&path).map_err(|error| {
        NodeFailure::fail(
            "RUST_TOOLCHAIN_AUTHORITY_DRIFT",
            format!("read '{}': {error}", path.display()),
        )
    })?;
    if actual != expected {
        return Err(NodeFailure::fail(
            "RUST_TOOLCHAIN_AUTHORITY_DRIFT",
            "rust-toolchain.toml does not match the exact Distribution-owned toolchain",
        ));
    }
    Ok(())
}

fn validate_architecture(
    metadata: &CargoMetadata,
    root: &Path,
) -> std::result::Result<(), NodeFailure> {
    let workspace_ids = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let packages = metadata
        .packages
        .iter()
        .filter(|package| workspace_ids.contains(&package.id))
        .collect::<Vec<_>>();
    let workspace_names = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut graph = BTreeMap::<&str, BTreeSet<&str>>::new();

    for package in &packages {
        package_role(package, root)?;
        graph.entry(&package.name).or_default();
        for dependency in &package.dependencies {
            if workspace_names.contains(dependency.name.as_str()) {
                graph
                    .entry(&package.name)
                    .or_default()
                    .insert(&dependency.name);
            }
        }
    }
    detect_cycle(&graph)?;

    for package in &packages {
        let role = package_role(package, root)?;
        for dependency in &package.dependencies {
            let edge_kind = dependency.kind.as_deref().unwrap_or("normal");
            let target = dependency.target.as_deref().unwrap_or("all targets");
            if forbidden_ecosystem(role, &dependency.name) {
                return Err(NodeFailure::fail(
                    "ARCH_FORBIDDEN_DEPENDENCY",
                    format!(
                        "{role} package '{}' has forbidden {edge_kind} dependency '{}' for {target}",
                        package.name, dependency.name
                    ),
                ));
            }
            if dependency.name.starts_with("yydra-")
                && !workspace_names.contains(dependency.name.as_str())
            {
                return Err(NodeFailure::fail(
                    "ARCH_FRAMEWORK_INTERNAL_DEPENDENCY",
                    format!(
                        "{role} package '{}' reaches Framework-internal crate '{}' through a {edge_kind} edge for {target}",
                        package.name, dependency.name
                    ),
                ));
            }
            if workspace_names.contains(dependency.name.as_str()) {
                let dependency_package = packages
                    .iter()
                    .find(|candidate| candidate.name == dependency.name)
                    .expect("workspace dependency name came from packages");
                let dependency_role = package_role(dependency_package, root)?;
                if !allowed_workspace_edge(role, dependency_role) {
                    return Err(NodeFailure::fail(
                        "ARCH_FORBIDDEN_LAYER_EDGE",
                        format!(
                            "{role} package '{}' has forbidden {edge_kind} edge to {dependency_role} package '{}' for {target}",
                            package.name, dependency.name
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn package_role<'a>(
    package: &'a MetadataPackage,
    root: &Path,
) -> std::result::Result<&'a str, NodeFailure> {
    let manifest = Path::new(&package.manifest_path);
    let relative = manifest.strip_prefix(root).map_err(|_| {
        NodeFailure::fail(
            "ARCH_WORKSPACE_PATH_ESCAPE",
            format!(
                "workspace package manifest '{}' escapes the Product Workspace",
                manifest.display()
            ),
        )
    })?;
    let role = relative
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    match role {
        "domain" | "application" | "persistence-postgres" | "transport-http" | "server" => Ok(role),
        _ => Err(NodeFailure::fail(
            "ARCH_UNKNOWN_WORKSPACE_ROLE",
            format!(
                "package '{}' has undeclared workspace role at '{}'",
                package.name,
                relative.display()
            ),
        )),
    }
}

fn forbidden_ecosystem(role: &str, dependency: &str) -> bool {
    match role {
        "domain" => matches!(
            dependency,
            "axum" | "sqlx" | "tokio" | "tower" | "tower-http"
        ),
        "application" => matches!(dependency, "axum" | "tower" | "tower-http"),
        "persistence-postgres" => matches!(dependency, "axum" | "tower" | "tower-http"),
        "transport-http" => dependency == "sqlx",
        "server" => false,
        _ => true,
    }
}

fn allowed_workspace_edge(from: &str, to: &str) -> bool {
    match from {
        "domain" => false,
        "application" => matches!(to, "domain" | "persistence-postgres"),
        "persistence-postgres" => to == "domain",
        "transport-http" => to == "application",
        "server" => matches!(
            to,
            "application" | "persistence-postgres" | "transport-http"
        ),
        _ => false,
    }
}

fn detect_cycle(graph: &BTreeMap<&str, BTreeSet<&str>>) -> std::result::Result<(), NodeFailure> {
    fn visit<'a>(
        node: &'a str,
        graph: &BTreeMap<&'a str, BTreeSet<&'a str>>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> std::result::Result<(), NodeFailure> {
        if visiting.contains(node) {
            return Err(NodeFailure::fail(
                "ARCH_DEPENDENCY_CYCLE",
                format!("workspace dependency cycle reaches package '{node}'"),
            ));
        }
        if !visited.insert(node) {
            return Ok(());
        }
        visiting.insert(node);
        if let Some(dependencies) = graph.get(node) {
            for dependency in dependencies {
                visit(dependency, graph, visiting, visited)?;
            }
        }
        visiting.remove(node);
        Ok(())
    }

    let mut visited = BTreeSet::new();
    for node in graph.keys() {
        visit(node, graph, &mut BTreeSet::new(), &mut visited)?;
    }
    Ok(())
}

fn check_rust_tests(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    let arguments = [
        "test",
        "--locked",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "--list",
        "--format",
        "terse",
    ];
    let listed = context.capture(context.root, "cargo", &arguments, &[])?;
    if !listed.status.success() {
        return Err(NodeFailure::fail(
            "RUST_TEST_DISCOVERY_FAILED",
            "cargo test discovery exited nonzero",
        ));
    }
    let discovered = has_discovered_rust_tests(&listed.stdout);
    if !discovered {
        return Err(NodeFailure::fail(
            "RUST_TESTS_EMPTY",
            "the canonical Rust test command discovered zero tests",
        ));
    }
    context.command(
        context.root,
        "cargo",
        &[
            "test",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
        ],
        &[],
        "RUST_TEST_FAILED",
    )
}

fn has_discovered_rust_tests(output: &[u8]) -> bool {
    String::from_utf8_lossy(output)
        .lines()
        .any(|line| line.ends_with(": test"))
}

fn check_frontend_lock(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    let package = context.root.join("frontend/package.json");
    let lock = context.root.join("frontend/package-lock.json");
    let before_package = fs::read(&package).map_err(|error| {
        NodeFailure::fail(
            "FRONTEND_LOCK_MISSING",
            format!("read '{}': {error}", package.display()),
        )
    })?;
    let before_lock = fs::read(&lock).map_err(|error| {
        NodeFailure::fail(
            "FRONTEND_LOCK_MISSING",
            format!("read '{}': {error}", lock.display()),
        )
    })?;
    validate_frontend_tool_authority(&before_package, &before_lock)?;
    let result = context.command(
        &context.root.join("frontend"),
        npm_program(),
        &["ci", "--ignore-scripts"],
        &[],
        "FRONTEND_LOCK_INSTALL_FAILED",
    );
    let after_package = fs::read(&package).unwrap_or_default();
    let after_lock = fs::read(&lock).unwrap_or_default();
    if after_package != before_package || after_lock != before_lock {
        return Err(NodeFailure::fail(
            "FRONTEND_LOCK_MUTATED",
            "npm changed package.json or package-lock.json during locked installation",
        ));
    }
    result?;
    let frontend = context.root.join("frontend");
    context.observe_tool_version(&frontend, "node", "node", &["--version"])?;
    context.observe_tool_version(&frontend, "npm", npm_program(), &["--version"])?;
    for (name, expected) in [
        ("expo", "57.0.19"),
        ("@playwright/test", "1.62.1"),
        ("@eslint/js", "10.0.1"),
        ("eslint", "10.9.1"),
        ("orval", "8.27.0"),
        ("prettier", "3.9.6"),
        ("typescript", "6.0.3"),
        ("typescript-eslint", "8.69.0"),
        ("vitest", "4.1.11"),
    ] {
        let expression = format!("require('./node_modules/{name}/package.json').version");
        context.observe_exact_tool_version(
            &frontend,
            name,
            "node",
            &["-p", &expression],
            expected,
        )?;
    }
    Ok(())
}

fn check_api_generated_contract(
    context: &mut NodeContext<'_>,
) -> std::result::Result<(), NodeFailure> {
    let executable = std::env::current_exe().map_err(|error| {
        NodeFailure::infrastructure("API_CHECK_EXECUTABLE_UNAVAILABLE", error.to_string())
    })?;
    let executable = executable.to_str().ok_or_else(|| {
        NodeFailure::infrastructure(
            "API_CHECK_EXECUTABLE_UNAVAILABLE",
            "the exact yydra executable path is not valid UTF-8",
        )
    })?;
    let output = context.capture(
        context.root,
        executable,
        &["generate", "api", ".", "--check"],
        &[],
    )?;
    if output.status.success() {
        return Ok(());
    }
    let detail = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let code = [
        "API_BREAKING_CHANGE_UNACKNOWLEDGED",
        "API_GENERATED_DRIFT",
        "API_CLIENT_DRIFT",
        "API_GENERATION_RECORD_DRIFT",
        "API_OPENAPI_PROFILE_INVALID",
        "API_OPENAPI_OPERATION_ID_INVALID",
        "API_OPENAPI_CONTENT_TYPE_INVALID",
        "API_OPENAPI_FIELD_NAME_INVALID",
        "API_OPENAPI_UNKNOWN_FIELD_POLICY_INVALID",
        "API_OPENAPI_REQUIREDNESS_INVALID",
        "API_OPENAPI_DECIMAL_INVALID",
        "API_OPENAPI_TIMESTAMP_INVALID",
        "API_OPENAPI_SAFE_INTEGER_INVALID",
        "API_OPENAPI_WIRE_TYPE_INVALID",
        "API_OPENAPI_NULLABILITY_INVALID",
        "API_OPENAPI_SHAPE_REUSE_INVALID",
        "API_CLIENT_STAGE_INVALID",
        "API_CLIENT_TYPECHECK_FAILED",
        "API_CLIENT_GENERATION_FAILED",
        "API_CLIENT_TOOL_VERSION_INVALID",
        "API_OPENAPI_EXPORT_FAILED",
        "API_GENERATION_BASELINE_INVALID",
        "API_GENERATION_LOCK_INVALID",
        "API_GENERATION_BUSY",
        "API_GENERATION_RECOVERY_REQUIRED",
        "API_GENERATION_RECOVERY_FAILED",
    ]
    .into_iter()
    .find(|code| detail.contains(code))
    .unwrap_or("API_GENERATED_CONTRACT_FAILED");
    Err(NodeFailure::fail(
        code,
        "isolated `yydra generate api --check` rejected the committed authority chain",
    ))
}

fn check_api_runtime_conformance(
    context: &mut NodeContext<'_>,
) -> std::result::Result<(), NodeFailure> {
    context.command(
        context.root,
        "cargo",
        &[
            "test",
            "--locked",
            "--test",
            "public_api_contract",
            "--",
            "--nocapture",
        ],
        &[],
        "API_RUNTIME_CONFORMANCE_FAILED",
    )
}

fn check_api_client_contract(
    context: &mut NodeContext<'_>,
) -> std::result::Result<(), NodeFailure> {
    check_generated_client_import_boundary(context.root)?;
    context.command(
        &context.root.join("frontend"),
        npm_program(),
        &[
            "exec",
            "--offline",
            "--",
            "vitest",
            "run",
            "src/framework/api/client.test.ts",
            "--passWithNoTests=false",
        ],
        &[],
        "API_CLIENT_CONTRACT_FAILED",
    )
}

fn check_generated_client_import_boundary(root: &Path) -> std::result::Result<(), NodeFailure> {
    let frontend = root.join("frontend");
    let mut pending = vec![frontend.join("app"), frontend.join("src")];
    while let Some(path) = pending.pop() {
        let relative = path.strip_prefix(&frontend).map_err(|error| {
            NodeFailure::infrastructure("API_CLIENT_IMPORT_SCAN_FAILED", error.to_string())
        })?;
        if relative.starts_with("src/generated/public-api")
            || relative.starts_with("src/framework/api")
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            NodeFailure::fail(
                "API_CLIENT_IMPORT_SCAN_FAILED",
                format!("inspect '{}': {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(NodeFailure::fail(
                "API_CLIENT_IMPORT_BOUNDARY_VIOLATION",
                format!(
                    "handwritten frontend path '{}' is a symlink",
                    path.display()
                ),
            ));
        }
        if metadata.is_dir() {
            let mut entries = fs::read_dir(&path)
                .map_err(|error| {
                    NodeFailure::fail(
                        "API_CLIENT_IMPORT_SCAN_FAILED",
                        format!("read '{}': {error}", path.display()),
                    )
                })?
                .collect::<std::io::Result<Vec<_>>>()
                .map_err(|error| {
                    NodeFailure::fail("API_CLIENT_IMPORT_SCAN_FAILED", error.to_string())
                })?;
            entries.sort_by_key(fs::DirEntry::file_name);
            pending.extend(entries.into_iter().rev().map(|entry| entry.path()));
            continue;
        }
        if !metadata.is_file()
            || !matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts")
            )
        {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|error| {
            NodeFailure::fail(
                "API_CLIENT_IMPORT_SCAN_FAILED",
                format!("read '{}': {error}", path.display()),
            )
        })?;
        if imports_generated_public_api(&source).map_err(|error| {
            NodeFailure::fail(
                "API_CLIENT_IMPORT_SCAN_FAILED",
                format!("parse module imports in '{}': {error}", path.display()),
            )
        })? {
            return Err(NodeFailure::fail(
                "API_CLIENT_IMPORT_BOUNDARY_VIOLATION",
                format!(
                    "Product code '{}' imports the Generated Client directly; import the handwritten Framework facade instead",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
enum JavaScriptToken {
    Identifier(String),
    String(String),
    Punctuation(char),
}

fn imports_generated_public_api(source: &str) -> std::result::Result<bool, &'static str> {
    let tokens = javascript_tokens(source)?;
    for (index, token) in tokens.iter().enumerate() {
        let JavaScriptToken::String(specifier) = token else {
            continue;
        };
        if !specifier.contains("generated/public-api") {
            continue;
        }
        let previous = index.checked_sub(1).and_then(|index| tokens.get(index));
        let before_previous = index.checked_sub(2).and_then(|index| tokens.get(index));
        let direct_module_load = matches!(
            (before_previous, previous),
            (
                Some(JavaScriptToken::Identifier(keyword)),
                Some(JavaScriptToken::Punctuation('('))
            ) if keyword == "import" || keyword == "require"
        );
        let bare_import = matches!(
            previous,
            Some(JavaScriptToken::Identifier(keyword)) if keyword == "import"
        );
        let from_clause = matches!(
            previous,
            Some(JavaScriptToken::Identifier(keyword)) if keyword == "from"
        ) && tokens[..index.saturating_sub(1)]
            .iter()
            .rev()
            .take_while(|token| !matches!(token, JavaScriptToken::Punctuation(';')))
            .any(|token| {
                matches!(
                    token,
                    JavaScriptToken::Identifier(keyword)
                        if keyword == "import" || keyword == "export"
                )
            });
        if direct_module_load || bare_import || from_clause {
            return Ok(true);
        }
    }
    Ok(false)
}

fn javascript_tokens(source: &str) -> std::result::Result<Vec<JavaScriptToken>, &'static str> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                if index + 1 == bytes.len() {
                    return Err("unterminated block comment");
                }
                index += 2;
            }
            quote @ (b'\'' | b'"' | b'`') => {
                index += 1;
                let mut value = String::new();
                let mut terminated = false;
                while index < bytes.len() {
                    match bytes[index] {
                        byte if byte == quote => {
                            index += 1;
                            terminated = true;
                            break;
                        }
                        b'\\' => {
                            let Some(escaped) = bytes.get(index + 1) else {
                                return Err("unterminated string escape");
                            };
                            value.push(char::from(*escaped));
                            index += 2;
                        }
                        byte => {
                            value.push(char::from(byte));
                            index += 1;
                        }
                    }
                }
                if !terminated {
                    return Err("unterminated string literal");
                }
                tokens.push(JavaScriptToken::String(value));
            }
            byte if byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$') => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
                {
                    index += 1;
                }
                tokens.push(JavaScriptToken::Identifier(source[start..index].to_owned()));
            }
            byte if byte.is_ascii() => {
                tokens.push(JavaScriptToken::Punctuation(char::from(byte)));
                index += 1;
            }
            _ => index += 1,
        }
    }
    Ok(tokens)
}

fn check_docker(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    let output = context.capture(
        context.root,
        "docker",
        &["version", "--format", "{{.Server.Version}}"],
        &[],
    )?;
    if !output.status.success() {
        return Err(NodeFailure::infrastructure(
            "DOCKER_UNAVAILABLE",
            "Docker daemon did not report a server version",
        ));
    }
    let version = String::from_utf8(output.stdout)
        .map_err(|error| NodeFailure::infrastructure("DOCKER_UNAVAILABLE", error.to_string()))?
        .trim()
        .to_owned();
    if version.is_empty() {
        return Err(NodeFailure::infrastructure(
            "DOCKER_UNAVAILABLE",
            "Docker daemon reported an empty server version",
        ));
    }
    context.tool_versions.insert("docker".to_owned(), version);
    Ok(())
}

fn validate_frontend_tool_authority(
    package: &[u8],
    lock: &[u8],
) -> std::result::Result<(), NodeFailure> {
    let package: serde_json::Value = serde_json::from_slice(package).map_err(|error| {
        NodeFailure::fail(
            "FRONTEND_TOOLCHAIN_DRIFT",
            format!("parse frontend/package.json: {error}"),
        )
    })?;
    let required = [
        ("dependencies", "expo", "57.0.19"),
        ("devDependencies", "@playwright/test", "1.62.1"),
        ("devDependencies", "@eslint/js", "10.0.1"),
        ("devDependencies", "eslint", "10.9.1"),
        ("devDependencies", "orval", "8.27.0"),
        ("devDependencies", "prettier", "3.9.6"),
        ("devDependencies", "typescript", "6.0.3"),
        ("devDependencies", "typescript-eslint", "8.69.0"),
        ("devDependencies", "vitest", "4.1.11"),
    ];
    for (section, name, expected) in required {
        let actual = package
            .get(section)
            .and_then(|values| values.get(name))
            .and_then(serde_json::Value::as_str);
        if actual != Some(expected) {
            return Err(NodeFailure::fail(
                "FRONTEND_TOOLCHAIN_DRIFT",
                format!(
                    "frontend/package.json requires {section}.{name}={actual:?}; exact Distribution authority is {expected}"
                ),
            ));
        }
        let lock: serde_json::Value = serde_json::from_slice(lock).map_err(|error| {
            NodeFailure::fail(
                "FRONTEND_TOOLCHAIN_DRIFT",
                format!("parse frontend/package-lock.json: {error}"),
            )
        })?;
        let lock_key = format!("node_modules/{name}");
        let locked = lock
            .get("packages")
            .and_then(|packages| packages.get(&lock_key))
            .and_then(|value| value.get("version"))
            .and_then(serde_json::Value::as_str);
        if locked != Some(expected) {
            return Err(NodeFailure::fail(
                "FRONTEND_TOOLCHAIN_DRIFT",
                format!(
                    "frontend/package-lock.json resolves {name}={locked:?}; exact Distribution authority is {expected}"
                ),
            ));
        }
    }
    Ok(())
}

#[derive(Default)]
struct DerivedFiles {
    paths: Vec<PathBuf>,
}

impl DerivedFiles {
    fn write(
        &mut self,
        root: &Path,
        relative: &str,
        contents: &[u8],
    ) -> std::result::Result<PathBuf, NodeFailure> {
        let path = root.join(relative);
        let parent = path.parent().expect("derived file has a parent");
        create_private_dir_all(parent).map_err(evidence_write_failure)?;
        let mut file = create_private_file(&path).map_err(evidence_write_failure)?;
        self.paths.push(path.clone());
        file.write_all(contents).map_err(evidence_write_failure)?;
        file.flush().map_err(evidence_write_failure)?;
        Ok(path)
    }
}

impl Drop for DerivedFiles {
    fn drop(&mut self) {
        for path in self.paths.iter().rev() {
            let _ = fs::remove_file(path);
        }
    }
}

const FRONTEND_SOURCES: &[&str] = &[
    "app",
    "e2e",
    "src",
    "scripts",
    "app.json",
    "eslint.config.mjs",
    "orval.config.mjs",
    "package.json",
    "playwright.config.mts",
    "tsconfig.json",
    "vitest.config.mts",
];

const FRONTEND_LINT_SOURCES: &[&str] = &[
    "app",
    "e2e",
    "src",
    "scripts",
    "eslint.config.mjs",
    "orval.config.mjs",
    "playwright.config.mts",
    "vitest.config.mts",
];

fn check_frontend_format(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    let mut derived = DerivedFiles::default();
    derived.write(
        context.root,
        "frontend/.expo/yydra-check/prettier.config.mjs",
        b"export default {};\n",
    )?;
    derived.write(
        context.root,
        "frontend/.expo/yydra-check/prettier.ignore",
        b"",
    )?;
    let mut arguments = vec![
        "exec",
        "--offline",
        "--",
        "prettier",
        "--config",
        ".expo/yydra-check/prettier.config.mjs",
        "--ignore-path",
        ".expo/yydra-check/prettier.ignore",
        "--check",
    ];
    arguments.extend_from_slice(FRONTEND_SOURCES);
    context.command(
        &context.root.join("frontend"),
        npm_program(),
        &arguments,
        &[],
        "FRONTEND_FORMAT_FAILED",
    )
}

fn check_frontend_lint(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    const CONFIG: &str = r#"import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{js,mjs,ts,tsx,mts}"],
    languageOptions: {
      globals: {
        __dirname: "readonly",
        console: "readonly",
        process: "readonly",
        setTimeout: "readonly",
        URL: "readonly",
      },
    },
  },
);
"#;
    let mut derived = DerivedFiles::default();
    derived.write(
        context.root,
        "frontend/.expo/yydra-check/eslint.config.mjs",
        CONFIG.as_bytes(),
    )?;
    let mut arguments = vec![
        "exec",
        "--offline",
        "--",
        "eslint",
        "--config",
        ".expo/yydra-check/eslint.config.mjs",
        "--no-ignore",
        "--max-warnings=0",
    ];
    arguments.extend_from_slice(FRONTEND_LINT_SOURCES);
    context.command(
        &context.root.join("frontend"),
        npm_program(),
        &arguments,
        &[],
        "FRONTEND_LINT_FAILED",
    )
}

fn check_frontend_typecheck(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    const CONFIG: &str = r#"{
  "extends": "../../tsconfig.json",
  "compilerOptions": { "strict": true, "noEmit": true, "noCheck": false },
  "include": ["../../**/*.ts", "../../**/*.tsx", "../../**/*.mts"],
  "exclude": ["../../node_modules", "../../.expo", "../../dist", "../../test-results"]
}
"#;
    let mut derived = DerivedFiles::default();
    derived.write(
        context.root,
        "frontend/.expo/yydra-check/tsconfig.json",
        CONFIG.as_bytes(),
    )?;
    context.command(
        &context.root.join("frontend"),
        npm_program(),
        &[
            "exec",
            "--offline",
            "--",
            "tsc",
            "--project",
            ".expo/yydra-check/tsconfig.json",
        ],
        &[],
        "FRONTEND_TYPECHECK_FAILED",
    )
}

fn check_frontend_tests(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    const CONFIG: &str = r#"import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

const root = fileURLToPath(new URL("../..", import.meta.url));
export default defineConfig({
  root,
  resolve: { alias: { "@": fileURLToPath(new URL("../../src", import.meta.url)) } },
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx,mjs}"],
    passWithNoTests: false,
  },
});
"#;
    if !has_canonical_frontend_test(&context.root.join("frontend/src"))? {
        return Err(NodeFailure::fail(
            "FRONTEND_TESTS_EMPTY",
            "the canonical frontend test roots contain zero test files",
        ));
    }
    let mut derived = DerivedFiles::default();
    derived.write(
        context.root,
        "frontend/.expo/yydra-check/vitest.config.mts",
        CONFIG.as_bytes(),
    )?;
    context.command(
        &context.root.join("frontend"),
        npm_program(),
        &[
            "exec",
            "--offline",
            "--",
            "vitest",
            "run",
            "--config",
            ".expo/yydra-check/vitest.config.mts",
            "--passWithNoTests=false",
        ],
        &[],
        "FRONTEND_TEST_FAILED",
    )
}

fn has_canonical_frontend_test(root: &Path) -> std::result::Result<bool, NodeFailure> {
    if !root.is_dir() {
        return Ok(false);
    }
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| {
            NodeFailure::fail(
                "FRONTEND_TEST_DISCOVERY_FAILED",
                format!("read '{}': {error}", directory.display()),
            )
        })? {
            let path = entry
                .map_err(|error| {
                    NodeFailure::fail("FRONTEND_TEST_DISCOVERY_FAILED", error.to_string())
                })?
                .path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                NodeFailure::fail("FRONTEND_TEST_DISCOVERY_FAILED", error.to_string())
            })?;
            if metadata.is_dir() {
                directories.push(path);
            } else if metadata.is_file()
                && path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| {
                        name.ends_with(".test.ts")
                            || name.ends_with(".test.tsx")
                            || name.ends_with(".test.mjs")
                    })
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn check_h5_runtime(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    const PLAYWRIGHT_CONFIG: &str = r#"import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  outputDir: process.env.YYDRA_PLAYWRIGHT_OUTPUT,
  fullyParallel: false,
  retries: 0,
  reporter: "line",
  use: {
    baseURL: `http://127.0.0.1:${process.env.YYDRA_H5_PORT}`,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
});
"#;
    const PLAYWRIGHT_SPEC: &str = r#"import { expect, test } from "@playwright/test";

test("production H5 reaches Axum and PostgreSQL after refresh", async ({ page }) => {
  const healthResponse = page.waitForResponse((response) => response.url().endsWith("/health"));
  await page.goto("/");
  const observed = await healthResponse;
  expect(observed.status()).toBe(200);
  await expect(observed.json()).resolves.toEqual({ status: "ready", database: "baseline" });
  await expect(page.getByText("Backend ready.")).toBeVisible();
  await expect(page.getByText("PostgreSQL schema: baseline")).toBeVisible();
  const apiUrl = process.env.EXPO_PUBLIC_API_URL;
  expect(apiUrl).toBeTruthy();
  const direct = await page.evaluate(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/health`);
    return { body: await response.json(), status: response.status };
  }, apiUrl);
  expect(direct).toEqual({
    body: { status: "ready", database: "baseline" },
    status: 200,
  });
  await page.reload();
  await expect(page.getByText("Backend ready.")).toBeVisible();
  await expect(page.getByText("PostgreSQL schema: baseline")).toBeVisible();
});
"#;
    context.tool_versions.insert(
        "postgres-image".to_owned(),
        "postgres:18.6-alpine3.24@sha256:d3e1620b530c944afa6e887d22eb899824da68e19c52024bf98f5220c88a65b2".to_owned(),
    );
    let mut derived = DerivedFiles::default();
    let compose_source = template_source_files()
        .into_iter()
        .find_map(|(path, bytes)| (path == "compose.yaml").then_some(bytes))
        .expect("packaged PostgreSQL Compose authority is embedded");
    let compose = derived.write(
        context.root,
        "target/yydra-check-runtime/compose.yaml",
        compose_source,
    )?;
    let postgres_port = available_port()?;
    let server_port = available_port()?;
    let h5_port = available_port()?;
    let project = format!("yydra-check-{}-{postgres_port}", std::process::id());
    let compose_arg = compose.to_string_lossy().into_owned();
    let postgres_port_arg = postgres_port.to_string();
    let server_address = format!("127.0.0.1:{server_port}");
    let database_url =
        format!("postgres://postgres:postgres@127.0.0.1:{postgres_port}/yydra_product");
    let api_url = format!("http://{server_address}");
    let h5_port_arg = h5_port.to_string();
    let runner = template_source_files()
        .into_iter()
        .find_map(|(path, bytes)| (path == "frontend/scripts/run-h5-e2e.mjs").then_some(bytes))
        .expect("packaged H5 runner is embedded");
    derived.write(
        context.root,
        "frontend/scripts/.yydra-check-run-h5-e2e.mjs",
        runner,
    )?;
    derived.write(
        context.root,
        "frontend/e2e/.yydra-check-playwright.config.mts",
        PLAYWRIGHT_CONFIG.as_bytes(),
    )?;
    derived.write(
        context.root,
        "frontend/e2e/.yydra-check-clean-workspace.spec.ts",
        PLAYWRIGHT_SPEC.as_bytes(),
    )?;
    let h5_dist = context.evidence_root.join("artifacts/h5.real-runtime/dist");
    let playwright = context
        .evidence_root
        .join("artifacts/h5.real-runtime/playwright");
    fs::create_dir_all(&playwright).map_err(|error| {
        NodeFailure::infrastructure("CHECK_EVIDENCE_WRITE_FAILED", error.to_string())
    })?;
    let h5_dist_arg = h5_dist.to_string_lossy().into_owned();
    let playwright_arg = playwright.to_string_lossy().into_owned();

    let mut compose_guard = ComposeGuard::new(
        context.root,
        project.clone(),
        compose_arg.clone(),
        postgres_port_arg.clone(),
    );
    let up = context.infrastructure_command(
        context.root,
        "docker",
        &[
            "compose",
            "--project-name",
            &project,
            "-f",
            &compose_arg,
            "up",
            "-d",
            "--wait",
            "postgres",
        ],
        &[("YYDRA_POSTGRES_PORT", &postgres_port_arg)],
        "H5_POSTGRES_UNAVAILABLE",
    );
    if let Err(failure) = up {
        return match compose_guard.cleanup(context) {
            Ok(()) => Err(failure),
            Err(cleanup) => Err(cleanup),
        };
    }

    let execution = (|| {
        context.command(
            context.root,
            "cargo",
            &["run", "--locked", "--bin", "migrate"],
            &[("DATABASE_URL", &database_url)],
            "H5_MIGRATION_FAILED",
        )?;
        let mut server = spawn_server(context, &database_url, &server_address)?;
        let readiness = wait_for_server(
            &mut server,
            server_address.parse().map_err(|error| {
                NodeFailure::fail("H5_SERVER_ADDRESS_INVALID", format!("{error}"))
            })?,
            context.shutdown,
        );
        if let Err(failure) = readiness {
            return match server.finish(&mut context.log) {
                Ok(()) => Err(failure),
                Err(log_failure) => Err(log_failure),
            };
        }
        let result = context.command(
            &context.root.join("frontend"),
            "node",
            &["scripts/.yydra-check-run-h5-e2e.mjs"],
            &[
                ("CI", "1"),
                ("EXPO_PUBLIC_API_URL", &api_url),
                ("YYDRA_H5_PORT", &h5_port_arg),
                ("YYDRA_H5_DIST", &h5_dist_arg),
                ("YYDRA_PLAYWRIGHT_OUTPUT", &playwright_arg),
                (
                    "YYDRA_PLAYWRIGHT_CONFIG",
                    "e2e/.yydra-check-playwright.config.mts",
                ),
                (
                    "YYDRA_PLAYWRIGHT_SPEC",
                    "e2e/.yydra-check-clean-workspace.spec.ts",
                ),
            ],
            "H5_E2E_FAILED",
        );
        let server_log = server.finish(&mut context.log);
        match (result, server_log) {
            (_, Err(failure)) => Err(failure),
            (Err(failure), Ok(())) => Err(failure),
            (Ok(()), Ok(())) => Ok(()),
        }
    })();
    let down = compose_guard.cleanup(context);
    match (execution, down) {
        (_, Err(cleanup)) => Err(cleanup),
        (Err(failure), Ok(())) => Err(failure),
        (Ok(()), Ok(())) => Ok(()),
    }
}

struct ComposeGuard {
    root: PathBuf,
    project: String,
    compose: String,
    port: String,
    active: bool,
}

impl ComposeGuard {
    fn new(root: &Path, project: String, compose: String, port: String) -> Self {
        Self {
            root: root.to_path_buf(),
            project,
            compose,
            port,
            active: true,
        }
    }

    fn cleanup(&mut self, context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
        let result = compose_down(context, &self.project, &self.compose, &self.port);
        if result.is_ok() {
            self.active = false;
        }
        result
    }
}

impl Drop for ComposeGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut command = sanitized_command("docker");
        command
            .args([
                "compose",
                "--project-name",
                &self.project,
                "-f",
                &self.compose,
                "down",
                "--volumes",
                "--remove-orphans",
            ])
            .env("YYDRA_POSTGRES_PORT", &self.port)
            .current_dir(&self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);
        let Ok(mut child) = CapturedChild::spawn(command) else {
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            match child.child.try_wait() {
                Ok(Some(_)) => {
                    child.disarm();
                    return;
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(_) => break,
            }
        }
        child.terminate();
    }
}

fn compose_down(
    context: &mut NodeContext<'_>,
    project: &str,
    compose: &str,
    port: &str,
) -> std::result::Result<(), NodeFailure> {
    context.cleanup_command(
        context.root,
        "docker",
        &[
            "compose",
            "--project-name",
            project,
            "-f",
            compose,
            "down",
            "--volumes",
            "--remove-orphans",
        ],
        &[("YYDRA_POSTGRES_PORT", port)],
        "H5_POSTGRES_CLEANUP_FAILED",
    )
}

struct ServerGuard {
    child: Child,
    output_root: PathBuf,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    #[cfg(windows)]
    job: Option<WindowsJob>,
}

impl ServerGuard {
    fn terminate(&mut self) {
        #[cfg(windows)]
        {
            drop(self.job.take());
            let _ = self.child.wait();
        }
        #[cfg(not(windows))]
        terminate_server(&mut self.child);
    }

    fn finish(&mut self, log: &mut File) -> std::result::Result<(), NodeFailure> {
        self.terminate();
        let stdout = fs::read(&self.stdout_path).map_err(evidence_write_failure)?;
        let stderr = fs::read(&self.stderr_path).map_err(evidence_write_failure)?;
        log.write_all(&stdout).map_err(evidence_write_failure)?;
        log.write_all(&stderr).map_err(evidence_write_failure)?;
        log.flush().map_err(evidence_write_failure)?;
        fs::remove_dir_all(&self.output_root).map_err(evidence_write_failure)
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn spawn_server(
    context: &mut NodeContext<'_>,
    database_url: &str,
    server_address: &str,
) -> std::result::Result<ServerGuard, NodeFailure> {
    let display = display_command(
        context.root,
        context.evidence_root,
        context.root,
        "cargo",
        &["run", "--locked", "--bin", "server"],
        &[
            ("DATABASE_URL", database_url),
            ("YYDRA_BIND_ADDRESS", server_address),
        ],
    );
    context.commands.push(display.clone());
    writeln!(context.log, "$ {display}").map_err(evidence_write_failure)?;
    context.log.flush().map_err(evidence_write_failure)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| NodeFailure::infrastructure("CHECK_CLOCK_UNAVAILABLE", error.to_string()))?
        .as_nanos();
    let output_root = context
        .evidence_root
        .join("artifacts")
        .join(format!(".server-{}-{nonce}", std::process::id()));
    create_private_dir_all(&output_root).map_err(evidence_write_failure)?;
    let stdout_path = output_root.join("stdout");
    let stderr_path = output_root.join("stderr");
    let stdout = create_private_file(&stdout_path).map_err(evidence_write_failure)?;
    let stderr = create_private_file(&stderr_path).map_err(evidence_write_failure)?;
    let mut command = sanitized_command("cargo");
    command
        .args(["run", "--locked", "--bin", "server"])
        .env("DATABASE_URL", database_url)
        .env("YYDRA_BIND_ADDRESS", server_address)
        .current_dir(context.root)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn().map_err(|error| {
        NodeFailure::infrastructure("H5_SERVER_UNAVAILABLE", format!("start server: {error}"))
    })?;
    #[cfg(windows)]
    let (child, job) = match create_kill_on_close_job(&child) {
        Ok(job) => (child, Some(job)),
        Err(error) => {
            let mut failed_child = child;
            let _ = failed_child.kill();
            let _ = failed_child.wait();
            return Err(NodeFailure::infrastructure(
                "H5_SERVER_SUPERVISION_UNAVAILABLE",
                format!("place server in a kill-on-close Windows Job Object: {error:#}"),
            ));
        }
    };
    Ok(ServerGuard {
        child,
        output_root,
        stdout_path,
        stderr_path,
        #[cfg(windows)]
        job,
    })
}

fn wait_for_server(
    server: &mut ServerGuard,
    address: SocketAddr,
    shutdown: &AtomicBool,
) -> std::result::Result<(), NodeFailure> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if shutdown.load(Ordering::SeqCst) {
            return Err(NodeFailure::infrastructure(
                "CHECK_CANCELLED",
                "cancelled while waiting for the Product Workspace server",
            ));
        }
        if let Some(status) = server.child.try_wait().map_err(|error| {
            NodeFailure::infrastructure("H5_SERVER_POLL_FAILED", error.to_string())
        })? {
            return Err(NodeFailure::fail(
                "H5_SERVER_EXITED",
                format!("Product Workspace server exited before readiness with {status}"),
            ));
        }
        if TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(NodeFailure::fail(
        "H5_SERVER_TIMEOUT",
        format!("Product Workspace server did not listen on {address} within 60 seconds"),
    ))
}

fn terminate_server(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    #[cfg(unix)]
    if let Ok(process_group) = i32::try_from(child.id()) {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;
        let _ = killpg(Pid::from_raw(process_group), Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = killpg(Pid::from_raw(process_group), Signal::SIGKILL);
    }
    #[cfg(not(any(unix, windows)))]
    let _ = child.kill();
    let _ = child.wait();
}

fn available_port() -> std::result::Result<u16, NodeFailure> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr().map(|address| address.port()))
        .map_err(|error| NodeFailure::infrastructure("CHECK_PORT_UNAVAILABLE", error.to_string()))
}

fn workspace_inputs(root: &Path) -> std::result::Result<InputInventory, NodeFailure> {
    let mut inputs = BTreeMap::new();
    collect_workspace_inputs(root, root, &mut inputs)?;
    Ok(inputs)
}

fn collect_workspace_inputs(
    root: &Path,
    directory: &Path,
    inputs: &mut InputInventory,
) -> std::result::Result<(), NodeFailure> {
    let entries = fs::read_dir(directory).map_err(|error| {
        NodeFailure::fail(
            "CHECK_INPUT_INVENTORY_FAILED",
            format!("read '{}': {error}", directory.display()),
        )
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| NodeFailure::fail("CHECK_INPUT_INVENTORY_FAILED", error.to_string()))?
            .path();
        let relative = path
            .strip_prefix(root)
            .expect("walk starts at workspace root");
        if excluded_input(relative) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            NodeFailure::fail(
                "CHECK_INPUT_INVENTORY_FAILED",
                format!("inspect '{}': {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(|error| {
                NodeFailure::fail("CHECK_INPUT_INVENTORY_FAILED", error.to_string())
            })?;
            inputs.insert(
                relative.to_path_buf(),
                InputEntry {
                    kind: InputKind::Symlink,
                    bytes: target.as_os_str().as_encoded_bytes().to_vec(),
                },
            );
        } else if metadata.is_dir() {
            inputs.insert(
                relative.to_path_buf(),
                InputEntry {
                    kind: InputKind::Directory,
                    bytes: Vec::new(),
                },
            );
            collect_workspace_inputs(root, &path, inputs)?;
        } else if metadata.is_file() {
            inputs.insert(
                relative.to_path_buf(),
                InputEntry {
                    kind: InputKind::File,
                    bytes: fs::read(&path).map_err(|error| {
                        NodeFailure::fail(
                            "CHECK_INPUT_INVENTORY_FAILED",
                            format!("read '{}': {error}", path.display()),
                        )
                    })?,
                },
            );
        }
    }
    Ok(())
}

fn excluded_input(relative: &Path) -> bool {
    relative.starts_with(".git")
        || relative.starts_with("target")
        || relative.starts_with("frontend/node_modules")
        || relative.starts_with("frontend/.expo")
        || relative.starts_with("frontend/dist")
        || relative.starts_with("frontend/test-results")
        || relative.starts_with("frontend/android")
        || relative.starts_with("frontend/ios")
}

fn copy_workspace_inputs(
    source_root: &Path,
    destination_root: &Path,
    inputs: &InputInventory,
) -> std::result::Result<(), NodeFailure> {
    for (relative, entry) in inputs {
        if entry.kind == InputKind::Directory {
            create_private_dir_all(&destination_root.join(relative))
                .map_err(evidence_write_failure)?;
        }
    }
    for (relative, entry) in inputs {
        let source = source_root.join(relative);
        let destination = destination_root.join(relative);
        match entry.kind {
            InputKind::Directory => {}
            InputKind::File => {
                if let Some(parent) = destination.parent() {
                    create_private_dir_all(parent).map_err(evidence_write_failure)?;
                }
                fs::copy(&source, &destination).map_err(|error| {
                    NodeFailure::infrastructure(
                        "CHECK_SCRATCH_COPY_FAILED",
                        format!(
                            "copy '{}' to '{}': {error}",
                            source.display(),
                            destination.display()
                        ),
                    )
                })?;
            }
            InputKind::Symlink => {
                copy_symlink(source_root, destination_root, &source, &destination)?
            }
        }
    }
    Ok(())
}

fn copy_symlink(
    source_root: &Path,
    destination_root: &Path,
    source: &Path,
    destination: &Path,
) -> std::result::Result<(), NodeFailure> {
    let canonical_root = source_root.canonicalize().map_err(|error| {
        NodeFailure::infrastructure("CHECK_SCRATCH_COPY_FAILED", error.to_string())
    })?;
    let canonical_target = source.canonicalize().map_err(|error| {
        NodeFailure::fail(
            "CHECK_SYMLINK_TARGET_INVALID",
            format!("resolve symlink '{}': {error}", source.display()),
        )
    })?;
    let target_relative = canonical_target
        .strip_prefix(&canonical_root)
        .map_err(|_| {
            NodeFailure::fail(
                "CHECK_SYMLINK_PATH_ESCAPE",
                format!(
                    "symlink '{}' resolves outside the Product Workspace",
                    source.display()
                ),
            )
        })?;
    if excluded_input(target_relative) {
        return Err(NodeFailure::fail(
            "CHECK_SYMLINK_TARGET_EXCLUDED",
            format!(
                "symlink '{}' targets excluded output '{}'",
                source.display(),
                target_relative.display()
            ),
        ));
    }
    create_relocated_symlink(
        &destination_root.join(target_relative),
        destination,
        canonical_target.is_dir(),
    )
}

#[cfg(unix)]
fn create_relocated_symlink(
    target: &Path,
    destination: &Path,
    _target_is_dir: bool,
) -> std::result::Result<(), NodeFailure> {
    std::os::unix::fs::symlink(target, destination).map_err(|error| {
        NodeFailure::infrastructure("CHECK_SCRATCH_COPY_FAILED", error.to_string())
    })
}

#[cfg(windows)]
fn create_relocated_symlink(
    target: &Path,
    destination: &Path,
    target_is_dir: bool,
) -> std::result::Result<(), NodeFailure> {
    use std::os::windows::fs::{symlink_dir, symlink_file};

    let result = if target_is_dir {
        symlink_dir(target, destination)
    } else {
        symlink_file(target, destination)
    };
    result.map_err(|error| {
        NodeFailure::infrastructure("CHECK_SCRATCH_COPY_FAILED", error.to_string())
    })
}

#[cfg(not(any(unix, windows)))]
fn create_relocated_symlink(
    _target: &Path,
    destination: &Path,
    _target_is_dir: bool,
) -> std::result::Result<(), NodeFailure> {
    Err(NodeFailure::infrastructure(
        "CHECK_SCRATCH_COPY_FAILED",
        format!(
            "copying symlink '{}' is unsupported on this host",
            destination.display()
        ),
    ))
}

fn check_and_remove_scratch(
    execution_root: &Path,
    baselines: &InputBaselines<'_>,
) -> std::result::Result<(), NodeFailure> {
    let actual_original = workspace_inputs(baselines.original_root)?;
    let actual_execution = workspace_inputs(execution_root)?;
    let drift = if actual_original != *baselines.original {
        Some((
            "CHECK_MUTATED_ORIGINAL_INPUTS",
            describe_input_drift(baselines.original, &actual_original),
        ))
    } else if actual_execution != *baselines.execution {
        Some((
            "CHECK_MUTATED_WORKSPACE_INPUTS",
            describe_input_drift(baselines.execution, &actual_execution),
        ))
    } else {
        None
    };
    fs::remove_dir_all(baselines.scratch_root).map_err(|error| {
        NodeFailure::infrastructure(
            "CHECK_SCRATCH_CLEANUP_FAILED",
            format!("remove '{}': {error}", baselines.scratch_root.display()),
        )
    })?;
    if let Some((code, message)) = drift {
        return Err(NodeFailure::fail(code, message));
    }
    Ok(())
}

fn plain_tree(root: &Path) -> std::result::Result<BTreeMap<PathBuf, Vec<u8>>, NodeFailure> {
    fn visit(
        root: &Path,
        directory: &Path,
        files: &mut BTreeMap<PathBuf, Vec<u8>>,
    ) -> std::result::Result<(), NodeFailure> {
        for entry in fs::read_dir(directory).map_err(|error| {
            NodeFailure::fail("BASELINE_SKILL_INVENTORY_DRIFT", error.to_string())
        })? {
            let path = entry
                .map_err(|error| {
                    NodeFailure::fail("BASELINE_SKILL_INVENTORY_DRIFT", error.to_string())
                })?
                .path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                NodeFailure::fail("BASELINE_SKILL_INVENTORY_DRIFT", error.to_string())
            })?;
            if metadata.file_type().is_symlink() {
                return Err(NodeFailure::fail(
                    "BASELINE_SKILL_INVENTORY_DRIFT",
                    format!(
                        "Baseline Skill snapshot path '{}' is a symlink",
                        path.display()
                    ),
                ));
            }
            if metadata.is_dir() {
                visit(root, &path, files)?;
            } else if metadata.is_file() {
                files.insert(
                    path.strip_prefix(root)
                        .expect("Skill path is below root")
                        .to_path_buf(),
                    fs::read(&path).map_err(|error| {
                        NodeFailure::fail("BASELINE_SKILL_INVENTORY_DRIFT", error.to_string())
                    })?,
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn describe_input_drift<Value: PartialEq>(
    expected: &BTreeMap<PathBuf, Value>,
    actual: &BTreeMap<PathBuf, Value>,
) -> String {
    let missing = expected
        .keys()
        .filter(|path| !actual.contains_key(*path))
        .collect::<Vec<_>>();
    let unexpected = actual
        .keys()
        .filter(|path| !expected.contains_key(*path))
        .collect::<Vec<_>>();
    let changed = expected
        .iter()
        .filter(|(path, bytes)| actual.get(*path).is_some_and(|actual| actual != *bytes))
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    format!("missing={missing:?}; unexpected={unexpected:?}; changed={changed:?}")
}

fn display_command(
    root: &Path,
    evidence_root: &Path,
    directory: &Path,
    program: &str,
    arguments: &[&str],
    environment: &[(&str, &str)],
) -> String {
    let normalize = |value: &str| {
        value
            .replace(&evidence_root.display().to_string(), "$EVIDENCE")
            .replace(&root.display().to_string(), "$WORKSPACE")
    };
    let mut parts = vec![format!(
        "cd {}",
        normalize(&directory.display().to_string())
    )];
    parts.extend(environment.iter().map(|(key, value)| {
        let displayed = if is_secret_environment_key(key) {
            "[REDACTED]".to_owned()
        } else {
            normalize(value)
        };
        format!("{key}={displayed}")
    }));
    parts.push(program.to_owned());
    parts.extend(arguments.iter().map(|argument| normalize(argument)));
    parts.join(" ")
}

fn sanitized_command(program: &str) -> Command {
    const ALLOWED_ENVIRONMENT: &[&str] = &[
        "CARGO_HOME",
        "COMSPEC",
        "DOCKER_CONFIG",
        "DOCKER_CONTEXT",
        "DOCKER_HOST",
        "DOCKER_TLS_VERIFY",
        "HOME",
        "HOMEDRIVE",
        "HOMEPATH",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "LANG",
        "LC_ALL",
        "NO_PROXY",
        "PATH",
        "PATHEXT",
        "PLAYWRIGHT_BROWSERS_PATH",
        "RUSTUP_HOME",
        "SYSTEMROOT",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USERPROFILE",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
    ];
    let mut command = Command::new(program);
    command.env_clear();
    for key in ALLOWED_ENVIRONMENT {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
}

fn is_secret_environment_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    key == "DATABASE_URL"
        || key.contains("PASSWORD")
        || key.contains("TOKEN")
        || key.contains("SECRET")
        || key.contains("CREDENTIAL")
        || key.ends_with("_KEY")
}

fn input_inventory_digest(inputs: &InputInventory) -> String {
    let mut digest = Sha256::new();
    for (path, entry) in inputs {
        digest.update(path.as_os_str().as_encoded_bytes());
        digest.update([0]);
        digest.update([match entry.kind {
            InputKind::Directory => 1,
            InputKind::File => 2,
            InputKind::Symlink => 3,
        }]);
        digest.update(
            u64::try_from(entry.bytes.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        digest.update(&entry.bytes);
    }
    format!("sha256:{}", hex::encode(digest.finalize()))
}

fn artifact_evidence(evidence_root: &Path, path: &Path) -> Result<ArtifactEvidence> {
    let mut entries = BTreeMap::<PathBuf, InputEntry>::new();
    collect_artifact_entries(path, path, &mut entries)?;
    let relative = path.strip_prefix(evidence_root).unwrap_or(path);
    Ok(ArtifactEvidence {
        path: relative.display().to_string(),
        sha256: input_inventory_digest(&entries),
    })
}

fn collect_artifact_entries(root: &Path, path: &Path, entries: &mut InputInventory) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect evidence artifact '{}'", path.display()))?;
    let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    if metadata.file_type().is_symlink() {
        entries.insert(
            relative,
            InputEntry {
                kind: InputKind::Symlink,
                bytes: fs::read_link(path)?.as_os_str().as_encoded_bytes().to_vec(),
            },
        );
    } else if metadata.is_file() {
        entries.insert(
            relative,
            InputEntry {
                kind: InputKind::File,
                bytes: fs::read(path)?,
            },
        );
    } else if metadata.is_dir() {
        if path != root {
            entries.insert(
                relative,
                InputEntry {
                    kind: InputKind::Directory,
                    bytes: Vec::new(),
                },
            );
        }
        let mut children = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            collect_artifact_entries(root, &child.path(), entries)?;
        }
    }
    Ok(())
}

fn required_tool_versions() -> BTreeMap<String, String> {
    [
        ("rust-toolchain", "1.97.1"),
        ("rustc", "1.97.1"),
        ("cargo", "1.97.1"),
        ("rustfmt", "1.9.0-stable"),
        ("clippy", "0.1.97"),
        ("expo", "57.0.19"),
        ("@playwright/test", "1.62.1"),
        ("@eslint/js", "10.0.1"),
        ("eslint", "10.9.1"),
        ("orval", "8.27.0"),
        ("prettier", "3.9.6"),
        ("typescript", "6.0.3"),
        ("typescript-eslint", "8.69.0"),
        ("vitest", "4.1.11"),
        (
            "postgres-image",
            "postgres:18.6-alpine3.24@sha256:d3e1620b530c944afa6e887d22eb899824da68e19c52024bf98f5220c88a65b2",
        ),
    ]
    .into_iter()
    .map(|(name, version)| (name.to_owned(), version.to_owned()))
    .collect()
}

fn observed_tool_versions(results: &[NodeResult]) -> BTreeMap<String, String> {
    let mut observed = BTreeMap::new();
    for result in results {
        for (name, version) in &result.tool_versions {
            observed.insert(name.clone(), version.clone());
        }
    }
    observed
}

fn relative_log(path: &Path, evidence_root: &Path) -> String {
    path.strip_prefix(evidence_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn emit_node(result: &NodeResult, format: MessageFormat) {
    match format {
        MessageFormat::Json => println!(
            "{}",
            serde_json::to_string(result).expect("serialize node result")
        ),
        MessageFormat::Human => {
            println!(
                "{} {} ({} ms)",
                outcome_name(&result.outcome).to_uppercase(),
                result.node_id,
                result.duration_ms
            );
            if !result.prerequisites.is_empty() {
                println!("  prerequisites: {}", result.prerequisites.join(", "));
            }
            if let Some(cause) = &result.cause {
                println!("  cause [{}]: {}", cause.code, cause.message);
            }
            if let Some(remediation) = &result.remediation {
                println!("  remediation: {remediation}");
            }
            println!("  proves: {}", result.proves);
            println!("  does-not-prove: {}", result.does_not_prove);
        }
    }
}

fn emit_summary(status: &str, complete: bool, manifest: &Path, format: MessageFormat) {
    match format {
        MessageFormat::Json => println!(
            "{}",
            serde_json::to_string(&SummaryEvent {
                schema_version: RESULT_SCHEMA_VERSION,
                event: "check-summary",
                status,
                scope: "clean-core-local",
                complete,
                aggregate_conformance: false,
                evidence: manifest.display().to_string(),
            })
            .expect("serialize check summary")
        ),
        MessageFormat::Human => println!(
            "CHECK {} scope=clean-core-local complete={} aggregate-conformance=false evidence={}",
            status.to_uppercase(),
            complete,
            manifest.display()
        ),
    }
}

fn outcome_name(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "pass",
        Outcome::Fail => "fail",
        Outcome::InfrastructureError => "infrastructure-error",
        Outcome::Skipped => "skipped",
        Outcome::NotRun => "not-run",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dependency(name: &str) -> MetadataDependency {
        MetadataDependency {
            name: name.to_owned(),
            kind: None,
            target: None,
        }
    }

    fn package(id: &str, role: &str, dependencies: &[&str]) -> MetadataPackage {
        MetadataPackage {
            id: id.to_owned(),
            name: id.to_owned(),
            manifest_path: format!("/workspace/crates/{role}/Cargo.toml"),
            dependencies: dependencies.iter().map(|name| dependency(name)).collect(),
        }
    }

    fn metadata(packages: Vec<MetadataPackage>) -> CargoMetadata {
        CargoMetadata {
            workspace_members: packages.iter().map(|package| package.id.clone()).collect(),
            packages,
        }
    }

    fn failure_code(metadata: &CargoMetadata) -> &'static str {
        validate_architecture(metadata, Path::new("/workspace"))
            .expect_err("negative architecture fixture")
            .code
    }

    #[test]
    fn allowed_architecture_fixture_passes() {
        let fixture = metadata(vec![
            package("reader-domain", "domain", &[]),
            package(
                "reader-application",
                "application",
                &["reader-domain", "reader-persistence-postgres"],
            ),
            package(
                "reader-persistence-postgres",
                "persistence-postgres",
                &["reader-domain", "sqlx"],
            ),
            package(
                "reader-transport-http",
                "transport-http",
                &["reader-application", "axum"],
            ),
            package(
                "reader-server",
                "server",
                &[
                    "reader-application",
                    "reader-persistence-postgres",
                    "reader-transport-http",
                    "axum",
                ],
            ),
        ]);
        validate_architecture(&fixture, Path::new("/workspace"))
            .expect("allowed architecture fixture");
    }

    #[test]
    fn architecture_negative_fixtures_have_discriminating_codes() {
        let forbidden_ecosystem = metadata(vec![package("reader-domain", "domain", &["sqlx"])]);
        assert_eq!(
            failure_code(&forbidden_ecosystem),
            "ARCH_FORBIDDEN_DEPENDENCY"
        );

        let forbidden_layer = metadata(vec![
            package("reader-application", "application", &[]),
            package(
                "reader-persistence-postgres",
                "persistence-postgres",
                &["reader-application"],
            ),
        ]);
        assert_eq!(failure_code(&forbidden_layer), "ARCH_FORBIDDEN_LAYER_EDGE");

        let cycle = metadata(vec![
            package("reader-application", "application", &["reader-domain"]),
            package("reader-domain", "domain", &["reader-application"]),
        ]);
        assert_eq!(failure_code(&cycle), "ARCH_DEPENDENCY_CYCLE");

        let framework_internal = metadata(vec![package("reader-domain", "domain", &["yydra-cli"])]);
        assert_eq!(
            failure_code(&framework_internal),
            "ARCH_FRAMEWORK_INTERNAL_DEPENDENCY"
        );

        let unknown_role = metadata(vec![package("reader-cache", "cache", &[])]);
        assert_eq!(failure_code(&unknown_role), "ARCH_UNKNOWN_WORKSPACE_ROLE");

        let mut escaped = package("reader-domain", "domain", &[]);
        escaped.manifest_path = "/outside/Cargo.toml".to_owned();
        assert_eq!(
            failure_code(&metadata(vec![escaped])),
            "ARCH_WORKSPACE_PATH_ESCAPE"
        );
    }

    #[test]
    fn canonical_rust_test_discovery_rejects_zero_tests() {
        assert!(!has_discovered_rust_tests(b"0 tests, 0 benchmarks\n"));
        assert!(!has_discovered_rust_tests(
            b"benches::throughput: benchmark\n"
        ));
        assert!(has_discovered_rust_tests(b"tests::transition: test\n"));
    }

    #[test]
    fn generated_client_boundary_recognizes_multiline_and_dynamic_module_loads() {
        for source in [
            "import {\n  profile,\n} from '../generated/public-api/fetch/client';",
            "export type { Profile }\nfrom '../generated/public-api/fetch/schemas';",
            "const client = import(\n  '../generated/public-api/fetch/client'\n);",
            "const client = require(\n  '../generated/public-api/fetch/client'\n);",
        ] {
            assert!(
                imports_generated_public_api(source).expect("scan valid fixture"),
                "fixture was missed: {source}"
            );
        }
        assert!(
            !imports_generated_public_api(
                "// import '../generated/public-api/fetch/client'\nconst note = \"generated/public-api\";"
            )
            .expect("scan comment/string fixture")
        );
    }
}
