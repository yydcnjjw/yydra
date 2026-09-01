use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    DISTRIBUTION_VERSION, check_baseline_skills, check_generated_ownership, compare_files,
    compare_trees, find_workspace_root, template_digest, tree_files,
};

const EVIDENCE_SCHEMA_VERSION: u32 = 1;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CheckProfile {
    Local,
    LinuxCi,
    MacosCi,
    Aggregate,
}

impl CheckProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::LinuxCi => "linux-ci",
            Self::MacosCi => "macos-ci",
            Self::Aggregate => "aggregate",
        }
    }
}

#[derive(Debug)]
pub(crate) struct CheckRequest {
    pub(crate) workspace: PathBuf,
    pub(crate) profile: CheckProfile,
    pub(crate) evidence_dir: Option<PathBuf>,
    pub(crate) shards: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CiOwner {
    Linux,
    Macos,
}

#[derive(Clone, Copy, Debug)]
struct NodeSpec {
    id: &'static str,
    owner: CiOwner,
}

const NODE_SPECS: &[NodeSpec] = &[
    NodeSpec {
        id: "origin.exact-distribution",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "ownership.baseline-skills",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "ownership.generated-boundaries",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "rust.format",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "rust.dependencies",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "rust.compile",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "rust.clippy",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "rust.test",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "api.openapi-drift",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "api.generated-client-drift",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "frontend.expo-dependencies",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "frontend.advisories",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "frontend.typecheck",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "frontend.test",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "h5.production-export",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "database.running-service",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "h5.e2e",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "android.release",
        owner: CiOwner::Linux,
    },
    NodeSpec {
        id: "ios.simulator-release",
        owner: CiOwner::Macos,
    },
    NodeSpec {
        id: "ownership.authored-inputs-unchanged",
        owner: CiOwner::Linux,
    },
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum NodeStatus {
    Pass,
    Fail,
    InfrastructureError,
    Skipped,
    NotRun,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceArtifact {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeEvidence {
    id: String,
    required: bool,
    ci_owner: String,
    platform_applicability: String,
    status: NodeStatus,
    commands: Vec<String>,
    duration_ms: u128,
    log: EvidenceArtifact,
    outputs: Vec<EvidenceArtifact>,
    failure_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceEvidence {
    product_id: String,
    distribution_version: String,
    template_sha256: String,
    input_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostEvidence {
    os: String,
    architecture: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckManifest {
    schema_version: u32,
    profile: CheckProfile,
    status: String,
    aggregate_complete: bool,
    distribution_version: String,
    workspace: WorkspaceEvidence,
    host: HostEvidence,
    nodes: Vec<NodeEvidence>,
    shard_manifests: Vec<EvidenceArtifact>,
    result_sha256: String,
}

pub(crate) fn check(request: CheckRequest) -> Result<()> {
    let root = find_workspace_root(&request.workspace)?;
    let workspace = workspace_evidence(&root, request.evidence_dir.as_deref())?;
    if request.profile == CheckProfile::Aggregate {
        aggregate(&root, workspace, request.evidence_dir, &request.shards)
    } else {
        if !request.shards.is_empty() {
            bail!("--shard is valid only with --profile aggregate");
        }
        execute_profile(&root, workspace, request.profile, request.evidence_dir)
    }
}

fn execute_profile(
    root: &Path,
    workspace: WorkspaceEvidence,
    profile: CheckProfile,
    evidence_dir: Option<PathBuf>,
) -> Result<()> {
    validate_profile_host(profile)?;
    let evidence_root = prepare_evidence_root(root, profile, evidence_dir)?;
    let initial_input = workspace.input_sha256.clone();
    let mut runner = GraphRunner::new(root, &evidence_root, profile, workspace);

    runner.node(spec("origin.exact-distribution"), |context| {
        context.internal("validate exact Distribution version and template digest");
        validate_origin(&runner_workspace(context.root, &context.evidence_root)?)
    });

    let linux_owned = profile != CheckProfile::MacosCi;
    if linux_owned {
        runner.node(spec("ownership.baseline-skills"), |context| {
            context.internal("compare Baseline Skill path and byte inventory with Distribution");
            check_baseline_skills(context.root)
        });
        runner.node(spec("ownership.generated-boundaries"), |context| {
            context.internal("enforce Generated Client ownership and Product Domain imports");
            check_generated_ownership(context.root)
        });
        runner.command_node(
            spec("rust.format"),
            root,
            "cargo",
            &["fmt", "--all", "--check"],
            &[],
        );
        runner.command_node(
            spec("rust.dependencies"),
            root,
            "cargo",
            &["metadata", "--locked", "--format-version", "1"],
            &[],
        );
        runner.command_node(
            spec("rust.compile"),
            root,
            "cargo",
            &[
                "check",
                "--locked",
                "--workspace",
                "--all-targets",
                "--all-features",
            ],
            &[("SQLX_OFFLINE", "true")],
        );
        runner.command_node(
            spec("rust.clippy"),
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
            &[("SQLX_OFFLINE", "true")],
        );
        runner.command_node(
            spec("rust.test"),
            root,
            "cargo",
            &["test", "--locked", "--workspace", "--all-features"],
            &[("SQLX_OFFLINE", "true")],
        );
        runner.node(spec("api.openapi-drift"), check_openapi_drift);
        runner.node(
            spec("api.generated-client-drift"),
            check_generated_client_drift,
        );

        let frontend = root.join("frontend");
        runner.command_node(
            spec("frontend.expo-dependencies"),
            &frontend,
            "npm",
            &["exec", "--", "expo", "install", "--check"],
            &[],
        );
        runner.command_node(
            spec("frontend.advisories"),
            &frontend,
            "npm",
            &["audit", "--audit-level=low"],
            &[],
        );
        runner.command_node(
            spec("frontend.typecheck"),
            &frontend,
            "npm",
            &["run", "typecheck"],
            &[],
        );
        runner.command_node(spec("frontend.test"), &frontend, "npm", &["test"], &[]);
        runner.node(spec("h5.production-export"), export_h5);
        let database_status = runner.node(spec("database.running-service"), |context| {
            with_live_stack(context, true, |_, _| Ok(()))
        });
        if database_status == NodeStatus::Pass {
            runner.node(spec("h5.e2e"), |context| {
                with_live_stack(context, false, run_h5_e2e)
            });
        } else {
            runner.skipped(
                spec("h5.e2e"),
                "database.running-service did not pass; H5 E2E prerequisite is unavailable",
            );
        }
        runner.node(spec("android.release"), build_android_release);
    } else {
        for node in NODE_SPECS.iter().filter(|node| {
            node.owner == CiOwner::Linux
                && !matches!(
                    node.id,
                    "origin.exact-distribution" | "ownership.authored-inputs-unchanged"
                )
        }) {
            runner.not_run(*node, "owned by the linux-ci shard");
        }
    }

    let run_ios = match profile {
        CheckProfile::MacosCi => true,
        CheckProfile::Local => std::env::consts::OS == "macos",
        CheckProfile::LinuxCi => false,
        CheckProfile::Aggregate => unreachable!(),
    };
    if run_ios {
        runner.node(spec("ios.simulator-release"), build_ios_simulator_release);
    } else {
        runner.not_run(
            spec("ios.simulator-release"),
            if profile == CheckProfile::LinuxCi {
                "owned by the macos-ci shard"
            } else {
                "current host is not macOS with Xcode"
            },
        );
    }

    runner.node(spec("ownership.authored-inputs-unchanged"), |context| {
        context.internal("compare authored input inventory before and after all check nodes");
        let actual = workspace_input_digest(context.root, Some(&context.evidence_root))?;
        if actual != initial_input {
            bail!("authored Product Workspace inputs changed during read-only check execution");
        }
        Ok(())
    });

    runner.finish()
}

fn validate_profile_host(profile: CheckProfile) -> Result<()> {
    match profile {
        CheckProfile::LinuxCi if std::env::consts::OS != "linux" => {
            bail!("linux-ci profile requires a Linux host")
        }
        CheckProfile::MacosCi if std::env::consts::OS != "macos" => {
            bail!("macos-ci profile requires a macOS host")
        }
        _ => Ok(()),
    }
}

fn spec(id: &str) -> NodeSpec {
    *NODE_SPECS
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("missing check node spec for {id}"))
}

struct GraphRunner<'a> {
    root: &'a Path,
    evidence_root: PathBuf,
    profile: CheckProfile,
    workspace: WorkspaceEvidence,
    nodes: Vec<NodeEvidence>,
}

impl<'a> GraphRunner<'a> {
    fn new(
        root: &'a Path,
        evidence_root: &Path,
        profile: CheckProfile,
        workspace: WorkspaceEvidence,
    ) -> Self {
        Self {
            root,
            evidence_root: evidence_root.to_path_buf(),
            profile,
            workspace,
            nodes: Vec::new(),
        }
    }

    fn node<F>(&mut self, node: NodeSpec, operation: F) -> NodeStatus
    where
        F: FnOnce(&mut NodeContext<'_>) -> Result<()>,
    {
        let started = Instant::now();
        let log_path = self
            .evidence_root
            .join("logs")
            .join(format!("{}.log", node.id));
        let mut context = NodeContext::new(self.root, &self.evidence_root, &log_path)
            .expect("evidence directory was prepared before node execution");
        let result = operation(&mut context);
        let (status, failure_reason) = match result {
            Ok(()) => (NodeStatus::Pass, None),
            Err(error) if context.infrastructure_error => {
                let message = format!("{error:#}");
                let _ = writeln!(context.log, "infrastructure-error: {message}");
                (NodeStatus::InfrastructureError, Some(message))
            }
            Err(error) => {
                let message = format!("{error:#}");
                let _ = writeln!(context.log, "failure: {message}");
                (NodeStatus::Fail, Some(message))
            }
        };
        context.log.flush().expect("flush node evidence log");
        let log = artifact(&log_path, &self.evidence_root).expect("hash node evidence log");
        let outputs = context
            .outputs
            .iter()
            .map(|output| artifact(output, &self.evidence_root))
            .collect::<Result<Vec<_>>>()
            .expect("hash node output evidence");
        let evidence = NodeEvidence {
            id: node.id.to_owned(),
            required: true,
            ci_owner: owner_name(node.owner).to_owned(),
            platform_applicability: applicability(node.owner).to_owned(),
            status: status.clone(),
            commands: context.commands,
            duration_ms: started.elapsed().as_millis(),
            log,
            outputs,
            failure_reason,
        };
        print_node(&evidence);
        self.nodes.push(evidence);
        status
    }

    fn command_node(
        &mut self,
        node: NodeSpec,
        current_dir: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> NodeStatus {
        self.node(node, |context| {
            context.command(current_dir, program, arguments, environment)
        })
    }

    fn not_run(&mut self, node: NodeSpec, reason: &str) {
        self.record_without_execution(node, NodeStatus::NotRun, reason);
    }

    fn skipped(&mut self, node: NodeSpec, reason: &str) {
        self.record_without_execution(node, NodeStatus::Skipped, reason);
    }

    fn record_without_execution(&mut self, node: NodeSpec, status: NodeStatus, reason: &str) {
        let log_path = self
            .evidence_root
            .join("logs")
            .join(format!("{}.log", node.id));
        fs::write(&log_path, format!("{}: {reason}\n", status_name(&status)))
            .expect("write explicit non-execution evidence");
        let evidence = NodeEvidence {
            id: node.id.to_owned(),
            required: true,
            ci_owner: owner_name(node.owner).to_owned(),
            platform_applicability: applicability(node.owner).to_owned(),
            status,
            commands: Vec::new(),
            duration_ms: 0,
            log: artifact(&log_path, &self.evidence_root).expect("hash non-execution evidence"),
            outputs: Vec::new(),
            failure_reason: Some(reason.to_owned()),
        };
        print_node(&evidence);
        self.nodes.push(evidence);
    }

    fn finish(self) -> Result<()> {
        let failed = self.nodes.iter().any(|node| {
            matches!(
                node.status,
                NodeStatus::Fail | NodeStatus::InfrastructureError | NodeStatus::Skipped
            )
        });
        let status = if failed { "fail" } else { "pass-applicable" };
        let mut manifest = CheckManifest {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            profile: self.profile,
            status: status.to_owned(),
            aggregate_complete: false,
            distribution_version: DISTRIBUTION_VERSION.to_owned(),
            workspace: self.workspace,
            host: host_evidence(),
            nodes: self.nodes,
            shard_manifests: Vec::new(),
            result_sha256: String::new(),
        };
        manifest.result_sha256 = semantic_digest(&manifest)?;
        write_manifest(&self.evidence_root, &manifest)?;
        println!(
            "evidence={}",
            self.evidence_root.join("manifest.json").display()
        );
        println!("status={status}");
        if failed {
            bail!("Mechanical Quality Contract failed; inspect the evidence manifest")
        }
        Ok(())
    }
}

struct NodeContext<'a> {
    root: &'a Path,
    evidence_root: PathBuf,
    log: File,
    commands: Vec<String>,
    outputs: Vec<PathBuf>,
    infrastructure_error: bool,
}

impl<'a> NodeContext<'a> {
    fn new(root: &'a Path, evidence_root: &Path, log_path: &Path) -> Result<Self> {
        let log = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(log_path)
            .with_context(|| format!("create node log '{}'", log_path.display()))?;
        Ok(Self {
            root,
            evidence_root: evidence_root.to_path_buf(),
            log,
            commands: Vec::new(),
            outputs: Vec::new(),
            infrastructure_error: false,
        })
    }

    fn internal(&mut self, description: &str) {
        self.commands.push(format!("internal:{description}"));
        let _ = writeln!(self.log, "internal: {description}");
    }

    fn command(
        &mut self,
        current_dir: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> Result<()> {
        let display = display_command(
            self.root,
            &self.evidence_root,
            current_dir,
            program,
            arguments,
            environment,
        );
        self.commands.push(display.clone());
        writeln!(self.log, "$ {display}")?;
        self.log.flush()?;
        let stdout = self.log.try_clone()?;
        let stderr = self.log.try_clone()?;
        let status = Command::new(program)
            .args(arguments)
            .envs(environment.iter().copied())
            .current_dir(current_dir)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .status();
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                self.infrastructure_error = true;
                return Err(error)
                    .with_context(|| format!("start {program} {}", arguments.join(" ")));
            }
        };
        if !status.success() {
            bail!(
                "{program} {} exited with {}",
                arguments.join(" "),
                status
                    .code()
                    .map_or_else(|| "signal".to_owned(), |code| code.to_string())
            );
        }
        Ok(())
    }

    fn spawn(
        &mut self,
        current_dir: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> Result<Child> {
        let display = display_command(
            self.root,
            &self.evidence_root,
            current_dir,
            program,
            arguments,
            environment,
        );
        self.commands.push(display.clone());
        writeln!(self.log, "$ {display}")?;
        self.log.flush()?;
        let stdout = self.log.try_clone()?;
        let stderr = self.log.try_clone()?;
        Command::new(program)
            .args(arguments)
            .envs(environment.iter().copied())
            .current_dir(current_dir)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .inspect_err(|_| {
                self.infrastructure_error = true;
            })
            .with_context(|| format!("start background {program} {}", arguments.join(" ")))
    }

    fn output(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            bail!(
                "expected evidence artifact '{}' was not produced",
                path.display()
            );
        }
        self.outputs.push(path.to_path_buf());
        Ok(())
    }
}

fn check_openapi_drift(context: &mut NodeContext<'_>) -> Result<()> {
    let generated = context
        .evidence_root
        .join("artifacts/api.openapi-drift/openapi.json");
    fs::create_dir_all(generated.parent().expect("OpenAPI artifact has parent"))?;
    let generated_arg = generated.to_string_lossy().into_owned();
    context.command(
        context.root,
        "cargo",
        &[
            "run",
            "--locked",
            "--bin",
            "generate-openapi",
            "--",
            &generated_arg,
        ],
        &[("SQLX_OFFLINE", "true")],
    )?;
    compare_files(&context.root.join("contracts/openapi.json"), &generated)
        .context("OpenAPI drift detected; run the exact Distribution's `yydra generate api`")?;
    context.output(&generated)
}

fn check_generated_client_drift(context: &mut NodeContext<'_>) -> Result<()> {
    let frontend = context.root.join("frontend");
    let staged = frontend.join("src/.yydra-check/public-api");
    let generated = context
        .evidence_root
        .join("artifacts/api.generated-client-drift/public-api");
    if staged.exists() {
        bail!(
            "temporary Generated Client path already exists: {}; remove it and retry",
            staged.display()
        );
    }
    fs::create_dir_all(
        generated
            .parent()
            .expect("Generated Client artifact has parent"),
    )?;
    let run_result = context.command(
        &frontend,
        "npm",
        &["run", "generate:api"],
        &[("YYDRA_GENERATED_API_ROOT", "./src/.yydra-check/public-api")],
    );
    let result = run_result.and_then(|()| {
        compare_trees(&frontend.join("src/generated/public-api"), &staged).context(
            "Generated Client drift detected; run the exact Distribution's `yydra generate api`",
        )?;
        copy_tree(&staged, &generated)?;
        context.output(&generated)
    });
    if let Some(parent) = staged.parent()
        && parent.exists()
    {
        fs::remove_dir_all(parent).with_context(|| {
            format!(
                "remove temporary Generated Client root '{}'",
                parent.display()
            )
        })?;
    }
    result
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let source_path = entry?.path();
        let destination_path = destination.join(
            source_path
                .file_name()
                .context("generated artifact path has no file name")?,
        );
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn export_h5(context: &mut NodeContext<'_>) -> Result<()> {
    let frontend = context.root.join("frontend");
    let output = context
        .evidence_root
        .join("artifacts/h5.production-export/dist");
    fs::create_dir_all(output.parent().expect("H5 artifact has parent"))?;
    let output_arg = output.to_string_lossy().into_owned();
    context.command(
        &frontend,
        "npm",
        &[
            "exec",
            "--",
            "expo",
            "export",
            "--platform",
            "web",
            "--output-dir",
            &output_arg,
        ],
        &[("CI", "1")],
    )?;
    context.output(&output)
}

fn with_live_stack<F>(
    context: &mut NodeContext<'_>,
    run_live_smoke: bool,
    operation: F,
) -> Result<()>
where
    F: FnOnce(&mut NodeContext<'_>, &str) -> Result<()>,
{
    let compose = context.root.join("compose.yaml");
    if !compose.is_file() {
        bail!("Product Workspace has no Distribution-owned compose.yaml");
    }
    let project = format!(
        "yydra-check-{}-{}",
        std::process::id(),
        origin_value(context.root, "product_id")?.replace('_', "-")
    );
    let postgres_port = available_port()?;
    let server_port = available_port()?;
    let postgres_port_arg = postgres_port.to_string();
    let database_url =
        format!("postgres://postgres:postgres@127.0.0.1:{postgres_port}/yydra_reading_queue");
    let server_address = format!("127.0.0.1:{server_port}");
    let api_base = format!("http://{server_address}");
    let compose_arg = compose.to_string_lossy().into_owned();
    let up_result = context.command(
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
        ],
        &[("YYDRA_POSTGRES_PORT", &postgres_port_arg)],
    );
    if up_result.is_err() {
        let _ = context.command(
            context.root,
            "docker",
            &[
                "compose",
                "--project-name",
                &project,
                "-f",
                &compose_arg,
                "down",
                "--volumes",
                "--remove-orphans",
            ],
            &[("YYDRA_POSTGRES_PORT", &postgres_port_arg)],
        );
        return up_result;
    }

    let execution = (|| {
        context.command(
            context.root,
            "cargo",
            &["run", "--locked", "--bin", "migrate"],
            &[("DATABASE_URL", &database_url)],
        )?;
        let mut server = context.spawn(
            context.root,
            "cargo",
            &["run", "--locked", "--bin", "server"],
            &[
                ("DATABASE_URL", &database_url),
                ("YYDRA_BIND_ADDRESS", &server_address),
                ("RUST_LOG", "info"),
            ],
        )?;
        let server_result = (|| {
            wait_for_server(&mut server, server_address.parse()?)?;
            if run_live_smoke {
                let script = context.root.join("scripts/verify-live-api.sh");
                let output = context
                    .evidence_root
                    .join("artifacts/database.running-service/live-api-smoke.json");
                fs::create_dir_all(output.parent().expect("database artifact has parent"))?;
                let script_arg = script.to_string_lossy().into_owned();
                let output_arg = output.to_string_lossy().into_owned();
                context.command(
                    context.root,
                    "bash",
                    &[&script_arg, &output_arg],
                    &[("API_BASE_URL", &api_base)],
                )?;
                context.output(&output)?;
            }
            operation(context, &api_base)
        })();
        let _ = server.kill();
        let _ = server.wait();
        server_result
    })();

    let down_result = context.command(
        context.root,
        "docker",
        &[
            "compose",
            "--project-name",
            &project,
            "-f",
            &compose_arg,
            "down",
            "--volumes",
            "--remove-orphans",
        ],
        &[("YYDRA_POSTGRES_PORT", &postgres_port_arg)],
    );
    execution.and(down_result)
}

fn wait_for_server(server: &mut Child, address: SocketAddr) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if let Some(status) = server.try_wait()? {
            bail!("Product Workspace server exited before readiness with {status}");
        }
        if TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    bail!("Product Workspace server did not listen on {address} within 60 seconds")
}

fn run_h5_e2e(context: &mut NodeContext<'_>, api_base: &str) -> Result<()> {
    let frontend = context.root.join("frontend");
    let output = context.evidence_root.join("artifacts/h5.e2e/playwright");
    fs::create_dir_all(&output)?;
    let output_arg = output.to_string_lossy().into_owned();
    context.command(
        &frontend,
        "npm",
        &["run", "test:e2e"],
        &[
            ("CI", "1"),
            ("EXPO_PUBLIC_API_URL", api_base),
            ("YYDRA_PLAYWRIGHT_OUTPUT", &output_arg),
        ],
    )?;
    context.output(&output)
}

fn available_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn build_android_release(context: &mut NodeContext<'_>) -> Result<()> {
    let frontend = context.root.join("frontend");
    let native = frontend.join("android");
    if native.exists() {
        bail!("generated Android host already exists; remove it before read-only check execution");
    }
    let execution = (|| {
        context.command(
            &frontend,
            "npm",
            &[
                "exec",
                "--",
                "expo",
                "prebuild",
                "--platform",
                "android",
                "--clean",
                "--no-install",
            ],
            &[("CI", "1")],
        )?;
        context.command(
            &native,
            "./gradlew",
            &["--no-daemon", "assembleRelease"],
            &[("CI", "1")],
        )?;
        let source = native.join("app/build/outputs/apk/release/app-release.apk");
        let output = context
            .evidence_root
            .join("artifacts/android.release/app-release.apk");
        fs::create_dir_all(output.parent().expect("Android artifact has parent"))?;
        fs::copy(&source, &output).with_context(|| {
            format!("copy Android release artifact from '{}'", source.display())
        })?;
        context.output(&output)
    })();
    if native.exists() {
        fs::remove_dir_all(&native)
            .with_context(|| format!("remove generated Android host '{}'", native.display()))?;
    }
    execution
}

fn build_ios_simulator_release(context: &mut NodeContext<'_>) -> Result<()> {
    let frontend = context.root.join("frontend");
    let native = frontend.join("ios");
    let scratch = context.root.join(format!(
        ".yydra-check/ios.simulator-release-{}",
        std::process::id()
    ));
    if native.exists() {
        bail!("generated iOS host already exists; remove it before read-only check execution");
    }
    if scratch.exists() {
        bail!(
            "iOS build scratch directory already exists: '{}'",
            scratch.display()
        );
    }
    let execution = (|| {
        context.command(
            &frontend,
            "npm",
            &[
                "exec",
                "--",
                "expo",
                "prebuild",
                "--platform",
                "ios",
                "--clean",
                "--no-install",
            ],
            &[("CI", "1")],
        )?;
        context.command(&native, "pod", &["install"], &[("CI", "1")])?;
        let workspace = find_extension(&native, "xcworkspace")?;
        let scheme = workspace
            .file_stem()
            .and_then(|value| value.to_str())
            .context("generated iOS workspace name is not UTF-8")?
            .to_owned();
        let derived = scratch.join("derived-data");
        fs::create_dir_all(&scratch)?;
        let workspace_arg = workspace.to_string_lossy().into_owned();
        let derived_arg = derived.to_string_lossy().into_owned();
        context.command(
            &native,
            "xcodebuild",
            &[
                "-workspace",
                &workspace_arg,
                "-scheme",
                &scheme,
                "-configuration",
                "Release",
                "-sdk",
                "iphonesimulator",
                "-derivedDataPath",
                &derived_arg,
                "CODE_SIGNING_ALLOWED=NO",
                "build",
            ],
            &[("CI", "1")],
        )?;
        let application = find_extension(&derived, "app")?;
        let application_name = application
            .file_name()
            .context("built iOS application has no file name")?;
        let output = context
            .evidence_root
            .join("artifacts/ios.simulator-release")
            .join(application_name);
        fs::create_dir_all(output.parent().expect("iOS artifact has parent"))?;
        let application_arg = application.to_string_lossy().into_owned();
        let output_arg = output.to_string_lossy().into_owned();
        context.command(
            &native,
            "ditto",
            &[&application_arg, &output_arg],
            &[("CI", "1")],
        )?;
        context.output(&output)?;
        let archive = output.with_extension("app.zip");
        let archive_arg = archive.to_string_lossy().into_owned();
        context.command(
            &native,
            "ditto",
            &[
                "-c",
                "-k",
                "--sequesterRsrc",
                "--keepParent",
                &output_arg,
                &archive_arg,
            ],
            &[("CI", "1")],
        )?;
        context.output(&archive)
    })();
    if native.exists() {
        fs::remove_dir_all(&native)
            .with_context(|| format!("remove generated iOS host '{}'", native.display()))?;
    }
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .with_context(|| format!("remove iOS build scratch '{}'", scratch.display()))?;
    }
    execution
}

fn find_extension(root: &Path, extension: &str) -> Result<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read generated directory '{}'", directory.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                return Ok(path);
            }
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    bail!("no .{extension} artifact found under '{}'", root.display())
}

fn aggregate(
    root: &Path,
    workspace: WorkspaceEvidence,
    evidence_dir: Option<PathBuf>,
    shard_paths: &[PathBuf],
) -> Result<()> {
    if shard_paths.is_empty() {
        bail!("aggregate profile requires at least one --shard manifest");
    }
    let evidence_root = prepare_evidence_root(root, CheckProfile::Aggregate, evidence_dir)?;
    let mut shards = Vec::new();
    let mut shard_artifacts = Vec::new();
    for shard_path in shard_paths {
        let bytes = fs::read(shard_path)
            .with_context(|| format!("read shard manifest '{}'", shard_path.display()))?;
        let shard: CheckManifest = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse shard manifest '{}'", shard_path.display()))?;
        validate_shard(&workspace, &shard, shard_path)?;
        shard_artifacts.push(EvidenceArtifact {
            path: shard_path.display().to_string(),
            sha256: hex::encode(Sha256::digest(&bytes)),
        });
        shards.push(shard);
    }

    let nodes = merge_nodes(&evidence_root, &shards)?;
    let complete = nodes.iter().all(|node| node.status == NodeStatus::Pass);
    let status = if complete { "pass" } else { "fail" };
    let mut manifest = CheckManifest {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        profile: CheckProfile::Aggregate,
        status: status.to_owned(),
        aggregate_complete: complete,
        distribution_version: DISTRIBUTION_VERSION.to_owned(),
        workspace,
        host: host_evidence(),
        nodes,
        shard_manifests: shard_artifacts,
        result_sha256: String::new(),
    };
    manifest.result_sha256 = semantic_digest(&manifest)?;
    write_manifest(&evidence_root, &manifest)?;
    println!("evidence={}", evidence_root.join("manifest.json").display());
    println!("status={status}");
    if !complete {
        bail!("aggregate Mechanical Quality Contract is incomplete or failed")
    }
    Ok(())
}

fn validate_shard(
    workspace: &WorkspaceEvidence,
    shard: &CheckManifest,
    manifest_path: &Path,
) -> Result<()> {
    if shard.schema_version != EVIDENCE_SCHEMA_VERSION {
        bail!("unsupported shard evidence schema {}", shard.schema_version);
    }
    if shard.profile == CheckProfile::Aggregate {
        bail!("an aggregate manifest cannot be used as a host shard");
    }
    if shard.distribution_version != DISTRIBUTION_VERSION
        || shard.workspace.product_id != workspace.product_id
        || shard.workspace.distribution_version != workspace.distribution_version
        || shard.workspace.template_sha256 != workspace.template_sha256
        || shard.workspace.input_sha256 != workspace.input_sha256
    {
        bail!(
            "shard manifest does not describe the exact same Distribution and Product Workspace inputs"
        );
    }
    if shard.result_sha256 != semantic_digest(shard)? {
        bail!("shard manifest semantic result digest is invalid");
    }
    if shard.nodes.len() != NODE_SPECS.len()
        || NODE_SPECS.iter().any(|expected| {
            shard
                .nodes
                .iter()
                .filter(|node| node.id == expected.id)
                .count()
                != 1
        })
    {
        bail!("shard manifest does not contain the exact required node catalog");
    }
    let shard_root = manifest_path
        .parent()
        .context("shard manifest path has no parent directory")?;
    for node in &shard.nodes {
        validate_artifact(shard_root, &node.log)
            .with_context(|| format!("validate {} log artifact", node.id))?;
        for output in &node.outputs {
            validate_artifact(shard_root, output)
                .with_context(|| format!("validate {} output artifact", node.id))?;
        }
    }
    Ok(())
}

fn validate_artifact(root: &Path, evidence: &EvidenceArtifact) -> Result<()> {
    let recorded = Path::new(&evidence.path);
    let path = if recorded.is_absolute() {
        recorded.to_path_buf()
    } else {
        root.join(recorded)
    };
    let actual = if path.is_dir() {
        directory_digest(&path)?
    } else {
        file_digest(&path)?
    };
    if actual != evidence.sha256 {
        bail!("artifact digest mismatch for '{}'", path.display());
    }
    Ok(())
}

fn merge_nodes(evidence_root: &Path, shards: &[CheckManifest]) -> Result<Vec<NodeEvidence>> {
    let mut merged = Vec::new();
    fs::create_dir_all(evidence_root.join("logs"))?;
    for node in NODE_SPECS {
        let candidates = shards
            .iter()
            .flat_map(|shard| shard.nodes.iter())
            .filter(|evidence| evidence.id == node.id)
            .collect::<Vec<_>>();
        let passing = candidates
            .iter()
            .find(|evidence| evidence.status == NodeStatus::Pass);
        let failure = candidates.iter().find(|evidence| {
            matches!(
                evidence.status,
                NodeStatus::Fail | NodeStatus::InfrastructureError | NodeStatus::Skipped
            )
        });
        let selected = if let Some(failure) = failure {
            Some(*failure)
        } else {
            passing.copied()
        };
        let (status, commands, outputs, reason) = match selected {
            Some(evidence) => (
                evidence.status.clone(),
                evidence.commands.clone(),
                evidence.outputs.clone(),
                evidence.failure_reason.clone(),
            ),
            None => (
                NodeStatus::NotRun,
                Vec::new(),
                Vec::new(),
                Some(
                    "no supplied host shard produced a pass result for this required node"
                        .to_owned(),
                ),
            ),
        };
        let log_path = evidence_root.join("logs").join(format!("{}.log", node.id));
        fs::write(
            &log_path,
            format!(
                "aggregate status={}\nreason={}\n",
                status_name(&status),
                reason.as_deref().unwrap_or("none")
            ),
        )?;
        let evidence = NodeEvidence {
            id: node.id.to_owned(),
            required: true,
            ci_owner: owner_name(node.owner).to_owned(),
            platform_applicability: applicability(node.owner).to_owned(),
            status,
            commands,
            duration_ms: 0,
            log: artifact(&log_path, evidence_root)?,
            outputs,
            failure_reason: reason,
        };
        print_node(&evidence);
        merged.push(evidence);
    }
    Ok(merged)
}

fn prepare_evidence_root(
    root: &Path,
    profile: CheckProfile,
    requested: Option<PathBuf>,
) -> Result<PathBuf> {
    let evidence_root = match requested {
        Some(path) if path.is_absolute() => path,
        Some(path) => root.join(path),
        None => root.join("target/yydra-check").join(format!(
            "{}-{}",
            profile.as_str(),
            std::process::id()
        )),
    };
    if evidence_root.exists() {
        bail!(
            "evidence directory '{}' already exists; choose a new directory",
            evidence_root.display()
        );
    }
    fs::create_dir_all(evidence_root.join("logs"))?;
    fs::create_dir_all(evidence_root.join("artifacts"))?;
    Ok(evidence_root)
}

fn write_manifest(root: &Path, manifest: &CheckManifest) -> Result<()> {
    let manifest_path = root.join("manifest.json");
    let diagnostics_path = root.join("diagnostics.jsonl");
    fs::write(&manifest_path, serde_json::to_vec_pretty(manifest)?)?;
    let mut diagnostics = File::create(&diagnostics_path)?;
    for node in &manifest.nodes {
        serde_json::to_writer(&mut diagnostics, node)?;
        diagnostics.write_all(b"\n")?;
    }
    Ok(())
}

fn workspace_evidence(root: &Path, evidence_dir: Option<&Path>) -> Result<WorkspaceEvidence> {
    let excluded = evidence_dir.map(|path| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        }
    });
    Ok(WorkspaceEvidence {
        product_id: origin_value(root, "product_id")?,
        distribution_version: origin_value(root, "distribution_version")?,
        template_sha256: origin_value(root, "template_sha256")?,
        input_sha256: workspace_input_digest(root, excluded.as_deref())?,
    })
}

fn runner_workspace(root: &Path, evidence_root: &Path) -> Result<WorkspaceEvidence> {
    workspace_evidence(root, Some(evidence_root))
}

fn validate_origin(workspace: &WorkspaceEvidence) -> Result<()> {
    if workspace.distribution_version != DISTRIBUTION_VERSION {
        bail!(
            "distribution mismatch; install exactly with: cargo install yydra-cli --version {} --locked",
            workspace.distribution_version
        );
    }
    let expected = template_digest();
    if workspace.template_sha256 != expected {
        bail!(
            "Workspace Origin Record template digest does not match Distribution {DISTRIBUTION_VERSION}"
        );
    }
    Ok(())
}

fn origin_value(root: &Path, field: &str) -> Result<String> {
    let origin_path = root.join(".yydra/origin.toml");
    let origin = fs::read_to_string(&origin_path)
        .with_context(|| format!("read Workspace Origin Record '{}'", origin_path.display()))?;
    origin
        .lines()
        .find_map(|line| {
            line.strip_prefix(&format!("{field} = \""))
                .and_then(|line| line.strip_suffix('"'))
        })
        .map(str::to_owned)
        .with_context(|| format!("Workspace Origin Record has no {field}"))
}

fn workspace_input_digest(root: &Path, evidence_root: Option<&Path>) -> Result<String> {
    let mut inventory = Vec::new();
    collect_workspace_inputs(root, root, evidence_root, &mut inventory)?;
    inventory.sort();
    let mut hasher = Sha256::new();
    for relative in inventory {
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        hasher.update(fs::read(root.join(&relative))?);
        hasher.update([0]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_workspace_inputs(
    root: &Path,
    directory: &Path,
    evidence_root: Option<&Path>,
    inventory: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("read Product Workspace directory '{}'", directory.display()))?
    {
        let path = entry?.path();
        if evidence_root.is_some_and(|excluded| path.starts_with(excluded)) {
            continue;
        }
        let relative = path.strip_prefix(root)?;
        if excluded_input(relative) {
            continue;
        }
        if path.is_dir() {
            collect_workspace_inputs(root, &path, evidence_root, inventory)?;
        } else if path.is_file() {
            inventory.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn excluded_input(relative: &Path) -> bool {
    relative.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some(
                ".git"
                    | "target"
                    | "node_modules"
                    | ".expo"
                    | "dist"
                    | "test-results"
                    | "android"
                    | "ios"
                    | ".yydra-check"
            )
        )
    })
}

fn artifact(path: &Path, evidence_root: &Path) -> Result<EvidenceArtifact> {
    let digest = if path.is_dir() {
        directory_digest(path)?
    } else {
        file_digest(path)?
    };
    let display = path.strip_prefix(evidence_root).map_or_else(
        |_| path.display().to_string(),
        |relative| relative.display().to_string(),
    );
    Ok(EvidenceArtifact {
        path: display,
        sha256: digest,
    })
}

fn file_digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn directory_digest(root: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    for file in tree_files(root)? {
        let relative = file.strip_prefix(root)?;
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        hasher.update(fs::read(file)?);
        hasher.update([0]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn semantic_digest(manifest: &CheckManifest) -> Result<String> {
    let semantic = (
        manifest.profile,
        manifest.aggregate_complete,
        &manifest.distribution_version,
        &manifest.workspace.product_id,
        &manifest.workspace.template_sha256,
        &manifest.workspace.input_sha256,
        manifest
            .nodes
            .iter()
            .map(|node| (&node.id, &node.status))
            .collect::<Vec<_>>(),
    );
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&semantic)?)))
}

fn host_evidence() -> HostEvidence {
    HostEvidence {
        os: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
    }
}

fn display_command(
    root: &Path,
    evidence_root: &Path,
    current_dir: &Path,
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
        normalize(&current_dir.display().to_string())
    )];
    parts.extend(
        environment
            .iter()
            .map(|(key, value)| format!("{key}={}", normalize(value))),
    );
    parts.push(program.to_owned());
    parts.extend(arguments.iter().map(|argument| normalize(argument)));
    parts.join(" ")
}

fn owner_name(owner: CiOwner) -> &'static str {
    match owner {
        CiOwner::Linux => "linux-ci",
        CiOwner::Macos => "macos-ci",
    }
}

fn applicability(owner: CiOwner) -> &'static str {
    match owner {
        CiOwner::Linux => "linux-or-local-supported-host",
        CiOwner::Macos => "macos-xcode",
    }
}

fn status_name(status: &NodeStatus) -> &'static str {
    match status {
        NodeStatus::Pass => "pass",
        NodeStatus::Fail => "fail",
        NodeStatus::InfrastructureError => "infrastructure-error",
        NodeStatus::Skipped => "skipped",
        NodeStatus::NotRun => "not-run",
    }
}

fn print_node(node: &NodeEvidence) {
    println!("{} {}", status_name(&node.status).to_uppercase(), node.id);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn semantic_digest_ignores_duration_and_log_bytes() {
        let workspace = WorkspaceEvidence {
            product_id: "test".to_owned(),
            distribution_version: DISTRIBUTION_VERSION.to_owned(),
            template_sha256: "template".to_owned(),
            input_sha256: "input".to_owned(),
        };
        let node = NodeEvidence {
            id: "rust.test".to_owned(),
            required: true,
            ci_owner: "linux-ci".to_owned(),
            platform_applicability: "linux".to_owned(),
            status: NodeStatus::Pass,
            commands: vec!["cargo test".to_owned()],
            duration_ms: 1,
            log: EvidenceArtifact {
                path: "one".to_owned(),
                sha256: "one".to_owned(),
            },
            outputs: Vec::new(),
            failure_reason: None,
        };
        let mut first = CheckManifest {
            schema_version: 1,
            profile: CheckProfile::LinuxCi,
            status: "pass-applicable".to_owned(),
            aggregate_complete: false,
            distribution_version: DISTRIBUTION_VERSION.to_owned(),
            workspace,
            host: host_evidence(),
            nodes: vec![node],
            shard_manifests: Vec::new(),
            result_sha256: String::new(),
        };
        let first_digest = semantic_digest(&first).unwrap();
        first.nodes[0].duration_ms = 99;
        first.nodes[0].log.sha256 = "different".to_owned();
        assert_eq!(first_digest, semantic_digest(&first).unwrap());
    }

    #[test]
    fn catalog_has_unique_stable_identifiers() {
        let counts = NODE_SPECS.iter().fold(BTreeMap::new(), |mut counts, node| {
            *counts.entry(node.id).or_insert(0) += 1;
            counts
        });
        assert!(counts.values().all(|count| *count == 1));
        assert!(counts.contains_key("database.running-service"));
        assert!(counts.contains_key("h5.e2e"));
        assert!(counts.contains_key("android.release"));
        assert!(counts.contains_key("ios.simulator-release"));
    }
}
