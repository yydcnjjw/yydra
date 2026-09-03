// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use include_dir::{Dir, DirEntry, File, include_dir};
use sha2::{Digest, Sha256};

mod api_generation;
mod check_graph;

const DISTRIBUTION_VERSION: &str = env!("CARGO_PKG_VERSION");
const TEMPLATE_IDENTITY: &str = "yydra-v0-product-workspace";
const TEMPLATE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/template/product-workspace");
const SPDX_EXPRESSION: &str = "MIT OR Apache-2.0";
const LICENSE_MIT: &[u8] = include_bytes!("../LICENSE-MIT");
const LICENSE_APACHE: &[u8] = include_bytes!("../LICENSE-APACHE");

#[derive(Debug, Parser)]
#[command(name = "yydra", version, about = "Yydra V0 Distribution CLI")]
struct Cli {
    /// Select human-readable output or the versioned JSON Lines interface.
    #[arg(long, global = true, value_enum, default_value_t = MessageFormat::Human)]
    message_format: MessageFormat,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Materialize a Product Workspace offline and atomically.
    New {
        destination: PathBuf,
        #[arg(long)]
        product_name: Option<String>,
        #[arg(long)]
        product_id: Option<String>,
        /// SPDX expression chosen for source authored by the Product team after creation.
        #[arg(long)]
        product_source_license: Option<String>,
    },
    /// Diagnose an exact Product Workspace without mutating it.
    Doctor {
        #[arg(default_value = ".")]
        workspace: PathBuf,
    },
    /// Install Cargo and npm dependencies strictly from committed locks.
    Setup {
        #[arg(default_value = ".")]
        workspace: PathBuf,
    },
    /// Run explicit migration, backend, and H5 development phases.
    Dev {
        #[arg(default_value = ".")]
        workspace: PathBuf,
    },
    /// Evaluate the read-only Mechanical Quality Contract.
    Check {
        #[arg(default_value = ".")]
        workspace: PathBuf,
        /// Write evidence outside the Workspace at a path with no symlink ancestor.
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
        /// Git revision whose existing migrations must remain byte-for-byte append-only.
        #[arg(long)]
        comparison_base: Option<String>,
        /// Run only these diagnostic nodes and their prerequisites.
        #[arg(long = "node")]
        nodes: Vec<String>,
        /// Require an exact catalog-owned fixture identity for later aggregation.
        #[arg(long, value_enum, default_value_t = CheckFixture::Unclassified)]
        fixture: CheckFixture,
        /// Verify uploaded complete fixture manifests and produce aggregate conformance.
        #[arg(long = "aggregate-evidence", value_name = "MANIFEST")]
        aggregate_evidence: Vec<PathBuf>,
    },
    /// Regenerate committed derived outputs from Product Workspace authorities.
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Manage the Product Workspace database explicitly.
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
}

#[derive(Debug, Subcommand)]
enum GenerateCommand {
    /// Generate normalized OpenAPI and the Orval Fetch/TypeScript/Zod client atomically.
    Api {
        #[arg(default_value = ".")]
        workspace: PathBuf,
        /// Generate only into isolated roots and compare without modifying the Workspace.
        #[arg(long)]
        check: bool,
        /// Record a reviewed lockstep breaking-change reference; repeat for multiple references.
        #[arg(long = "acknowledge-breaking-change")]
        acknowledgements: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum DbCommand {
    /// Apply all committed migrations without starting the server.
    Migrate {
        #[arg(default_value = ".")]
        workspace: PathBuf,
    },
    /// Manage migration source files.
    Migration {
        #[command(subcommand)]
        command: MigrationCommand,
    },
}

#[derive(Debug, Subcommand)]
enum MigrationCommand {
    /// Create the next sequential SQL migration without applying it.
    Add {
        name: String,
        #[arg(default_value = ".")]
        workspace: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let reporter = Reporter::new(cli.message_format);
    match cli.command {
        Command::New {
            destination,
            product_name,
            product_id,
            product_source_license,
        } => {
            let input = reporter.phase(
                "new.inputs",
                "NEW_INPUT_VALIDATE",
                Some(&destination),
                Some("provide a non-empty product name, a lowercase kebab-case product id, and a valid SPDX expression"),
                || resolve_input(product_name, product_id, product_source_license),
            )?;
            reporter.phase(
                "new.materialize",
                "NEW_WORKSPACE_CREATE",
                Some(&destination),
                Some("choose an absent or empty destination and retain the exact packaged Distribution"),
                || create_workspace(&destination, &input),
            )
        }
        Command::Doctor { workspace } => doctor(&workspace, &reporter),
        Command::Setup { workspace } => setup(&workspace, &reporter),
        Command::Dev { workspace } => dev(&workspace, &reporter),
        Command::Check {
            workspace,
            evidence_dir,
            comparison_base,
            nodes,
            fixture,
            aggregate_evidence,
        } => {
            if aggregate_evidence.is_empty() {
                check_graph::check(
                    check_graph::CheckRequest {
                        workspace,
                        evidence_dir,
                        comparison_base,
                        selected_nodes: nodes,
                        fixture: fixture.as_str().to_owned(),
                    },
                    cli.message_format,
                )
            } else {
                if comparison_base.is_some()
                    || !nodes.is_empty()
                    || fixture != CheckFixture::Unclassified
                    || workspace.as_path() != Path::new(".")
                {
                    bail!(
                        "--aggregate-evidence cannot be combined with a Workspace argument, --comparison-base, --node, or --fixture"
                    );
                }
                check_graph::aggregate(
                    check_graph::AggregateRequest {
                        evidence_dir,
                        manifests: aggregate_evidence,
                    },
                    cli.message_format,
                )
            }
        }
        Command::Generate { command } => match command {
            GenerateCommand::Api {
                workspace,
                check,
                acknowledgements,
            } => api_generation::generate_api(
                api_generation::ApiGenerationRequest {
                    workspace: &workspace,
                    check,
                    acknowledgements: &acknowledgements,
                },
                &reporter,
            ),
        },
        Command::Db { command } => match command {
            DbCommand::Migrate { workspace } => db_migrate(&workspace, &reporter),
            DbCommand::Migration { command } => match command {
                MigrationCommand::Add { name, workspace } => {
                    db_migration_add(&workspace, &name, &reporter)
                }
            },
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum MessageFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum CheckFixture {
    #[default]
    Unclassified,
    Clean,
    ReadingQueue,
}

impl CheckFixture {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unclassified => "unclassified",
            Self::Clean => "clean",
            Self::ReadingQueue => "reading-queue",
        }
    }
}

struct Reporter {
    format: MessageFormat,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredDiagnosticEvent<'a> {
    schema_version: u64,
    phase: &'a str,
    code: &'a str,
    severity: &'a str,
    status: &'a str,
    message: &'a str,
    location: Option<String>,
    remediation: Option<&'a str>,
}

struct Diagnostic<'a> {
    phase: &'a str,
    code: &'a str,
    severity: &'a str,
    status: &'a str,
    message: &'a str,
    location: Option<&'a Path>,
    remediation: Option<&'a str>,
}

impl Reporter {
    fn new(format: MessageFormat) -> Self {
        Self { format }
    }

    fn phase<T>(
        &self,
        phase: &str,
        code: &str,
        location: Option<&Path>,
        remediation: Option<&str>,
        action: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.emit(Diagnostic {
            phase,
            code,
            severity: "info",
            status: "started",
            message: "phase started",
            location,
            remediation: None,
        });
        match action() {
            Ok(value) => {
                self.emit(Diagnostic {
                    phase,
                    code,
                    severity: "info",
                    status: "pass",
                    message: "phase completed",
                    location,
                    remediation: None,
                });
                Ok(value)
            }
            Err(error) => {
                let message = format!("{error:#}");
                self.emit(Diagnostic {
                    phase,
                    code,
                    severity: "error",
                    status: "fail",
                    message: &message,
                    location,
                    remediation,
                });
                Err(error)
            }
        }
    }

    fn emit(&self, diagnostic: Diagnostic<'_>) {
        if self.format == MessageFormat::Json {
            let event = StructuredDiagnosticEvent {
                schema_version: 1,
                phase: diagnostic.phase,
                code: diagnostic.code,
                severity: diagnostic.severity,
                status: diagnostic.status,
                message: diagnostic.message,
                location: diagnostic.location.map(|path| path.display().to_string()),
                remediation: diagnostic.remediation,
            };
            println!(
                "{}",
                serde_json::to_string(&event).expect("diagnostic event is JSON-encodable")
            );
        } else if diagnostic.severity == "error" {
            let location = diagnostic
                .location
                .map(|path| format!(" location={}", path.display()))
                .unwrap_or_default();
            eprintln!(
                "[{}] {} code={}: {}{}",
                diagnostic.phase, diagnostic.status, diagnostic.code, diagnostic.message, location
            );
            if let Some(remediation) = diagnostic.remediation {
                eprintln!("[{}] remediation: {remediation}", diagnostic.phase);
            }
        } else {
            let location = diagnostic
                .location
                .map(|path| format!(" location={}", path.display()))
                .unwrap_or_default();
            println!(
                "[{}] {} code={}: {}{}",
                diagnostic.phase, diagnostic.status, diagnostic.code, diagnostic.message, location
            );
        }
    }
}

fn resolve_input(
    product_name: Option<String>,
    product_id: Option<String>,
    product_source_license: Option<String>,
) -> Result<NormalizedInput> {
    let product_name = match product_name {
        Some(value) => value,
        None => prompt("Product name")?,
    };
    let product_id = match product_id {
        Some(value) => value,
        None => prompt("Product id")?,
    };
    let product_source_license = match product_source_license {
        Some(value) => value,
        None => prompt("Product source license (SPDX expression)")?,
    };
    NormalizedInput::new(&product_name, &product_id, &product_source_license)
}

fn prompt(label: &str) -> Result<String> {
    eprint!("{label}: ");
    io::stderr().flush().context("flush interactive prompt")?;
    let mut value = String::new();
    if io::stdin()
        .read_line(&mut value)
        .context("read interactive answer")?
        == 0
    {
        bail!("no interactive answer received for {label}");
    }
    Ok(value)
}

#[derive(Debug)]
struct NormalizedInput {
    product_name: String,
    product_id: String,
    product_source_license: String,
}

impl NormalizedInput {
    fn new(product_name: &str, product_id: &str, product_source_license: &str) -> Result<Self> {
        let product_name = product_name.trim();
        if product_name.is_empty() {
            bail!("product name must not be empty");
        }
        if product_name.chars().any(char::is_control) {
            bail!("product name must not contain control characters");
        }

        let product_id = product_id.trim();
        validate_product_id(product_id)?;
        let product_source_license = product_source_license.trim();
        if product_source_license.chars().any(char::is_control) {
            bail!("product source license must not contain control characters");
        }
        let product_source_license = product_source_license
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if product_source_license.is_empty() {
            bail!("product source license must not be empty");
        }
        if !product_source_license.is_ascii() {
            bail!("product source license must be an ASCII SPDX expression");
        }
        if product_source_license.len() > 256 {
            bail!("product source license must be at most 256 characters");
        }
        if let Err(error) = spdx::Expression::parse(&product_source_license) {
            bail!("product source license must be a valid SPDX expression: {error}");
        }
        Ok(Self {
            product_name: product_name.to_owned(),
            product_id: product_id.to_owned(),
            product_source_license,
        })
    }
}

fn validate_product_id(value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric();
    if !valid {
        bail!(
            "product id must start with a lowercase letter, end with a letter or digit, contain only lowercase ASCII letters, digits, or internal hyphens, and be at most 63 characters"
        );
    }
    Ok(())
}

fn create_workspace(destination: &Path, input: &NormalizedInput) -> Result<()> {
    inspect_destination(destination)?;

    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create destination parent '{}'", parent.display()))?;
    let destination_name = destination
        .file_name()
        .and_then(OsStr::to_str)
        .context("destination must end in a UTF-8 path component")?;
    let stage = reserve_stage(parent, destination_name)?;
    let template_digest = template_digest();
    let render = RenderContext {
        input,
        template_digest: &template_digest,
    };

    let result = materialize(&TEMPLATE, &stage, &render)
        .and_then(|()| write_distribution_authorities(&stage))
        .and_then(|()| {
            fs::rename(&stage, destination).with_context(|| {
                format!(
                    "atomically move staged workspace '{}' to '{}'",
                    stage.display(),
                    destination.display()
                )
            })
        });
    if result.is_err() && stage.exists() {
        fs::remove_dir_all(&stage)
            .with_context(|| format!("remove failed stage '{}'", stage.display()))?;
    }
    result?;

    Ok(())
}

fn inspect_destination(destination: &Path) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            let mut entries = fs::read_dir(destination)
                .with_context(|| format!("inspect destination '{}'", destination.display()))?;
            if entries.next().is_none() {
                Ok(())
            } else {
                bail!(
                    "destination '{}' is not empty; Distribution creation never merges or overwrites",
                    destination.display()
                )
            }
        }
        Ok(_) => bail!(
            "destination '{}' already exists and is not an empty directory; Distribution creation never merges or overwrites",
            destination.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("inspect destination '{}'", destination.display()))
        }
    }
}

fn reserve_stage(parent: &Path, destination_name: &str) -> Result<PathBuf> {
    for attempt in 0..1024 {
        let stage = parent.join(format!(
            ".yydra-stage-{destination_name}-{}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&stage) {
            Ok(()) => {
                if let Err(error) = set_mode(&stage, 0o755) {
                    let _ = fs::remove_dir(&stage);
                    return Err(error);
                }
                return Ok(stage);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("create staging directory '{}'", stage.display()));
            }
        }
    }
    bail!("could not reserve a unique staging directory")
}

struct RenderContext<'a> {
    input: &'a NormalizedInput,
    template_digest: &'a str,
}

fn materialize(directory: &Dir<'_>, destination: &Path, render: &RenderContext<'_>) -> Result<()> {
    for entry in directory.entries() {
        match entry {
            DirEntry::Dir(child) => {
                let output = destination.join(child.path());
                fs::create_dir_all(&output)
                    .with_context(|| format!("create template directory '{}'", output.display()))?;
                set_mode(&output, 0o755)?;
                materialize(child, destination, render)?;
            }
            DirEntry::File(file) => {
                let output = destination.join(materialized_template_path(file.path()));
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("create template parent directory '{}'", parent.display())
                    })?;
                }
                let source = std::str::from_utf8(file.contents()).with_context(|| {
                    format!(
                        "Distribution template '{}' must be UTF-8",
                        file.path().display()
                    )
                })?;
                let rendered = render_template(source, render)?;
                fs::write(&output, rendered)
                    .with_context(|| format!("write template file '{}'", output.display()))?;
                set_mode(&output, 0o644)?;
            }
        }
    }
    Ok(())
}

fn materialized_template_path(source: &Path) -> PathBuf {
    // Cargo excludes nested packages from a published crate. Store embedded
    // manifests with a packaging-only suffix and expose only canonical names.
    if source.file_name() == Some(OsStr::new("Cargo.toml.tmpl")) {
        source.with_file_name("Cargo.toml")
    } else {
        source.to_owned()
    }
}

fn write_distribution_authorities(destination: &Path) -> Result<()> {
    for (relative, contents) in [
        ("LICENSE-APACHE", LICENSE_APACHE),
        ("LICENSE-MIT", LICENSE_MIT),
    ] {
        let output = destination.join(relative);
        fs::write(&output, contents)
            .with_context(|| format!("write exact Distribution snapshot '{}'", output.display()))?;
        set_mode(&output, 0o644)?;
    }

    let inventory_path = destination.join(".yydra/distribution-inventory.json");
    let inventory = distribution_inventory_json()?;
    fs::write(&inventory_path, inventory).with_context(|| {
        format!(
            "write Distribution inventory '{}'",
            inventory_path.display()
        )
    })?;
    set_mode(&inventory_path, 0o644)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("set deterministic mode {mode:o} on '{}'", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn render_template(source: &str, render: &RenderContext<'_>) -> Result<String> {
    let replacements = [
        (
            "__YYDRA_DISTRIBUTION_VERSION__",
            DISTRIBUTION_VERSION.to_owned(),
        ),
        ("__YYDRA_TEMPLATE_IDENTITY__", TEMPLATE_IDENTITY.to_owned()),
        (
            "__YYDRA_TEMPLATE_SHA256__",
            render.template_digest.to_owned(),
        ),
        (
            "__YYDRA_CREATION_INPUTS_SHA256__",
            creation_inputs_digest(render.input, render.template_digest),
        ),
        (
            "__PRODUCT_NAME_TOML__",
            serde_json::to_string(&render.input.product_name)
                .context("encode normalized product name")?,
        ),
        (
            "__PRODUCT_NAME_JSON__",
            serde_json::to_string(&render.input.product_name)
                .context("encode product name as a JSON and JavaScript string literal")?,
        ),
        (
            "__PRODUCT_ID_TOML__",
            serde_json::to_string(&render.input.product_id)
                .context("encode normalized product id")?,
        ),
        (
            "__PRODUCT_SOURCE_LICENSE_TOML__",
            serde_json::to_string(&render.input.product_source_license)
                .context("encode normalized product source license")?,
        ),
        ("__PRODUCT_NAME__", render.input.product_name.clone()),
        ("__PRODUCT_ID__", render.input.product_id.clone()),
        (
            "__ANDROID_PACKAGE__",
            format!("dev.yydra.{}", render.input.product_id.replace('-', "_")),
        ),
    ];

    let mut output = String::with_capacity(source.len());
    let mut remaining = source;
    while let Some((offset, token, replacement)) = replacements
        .iter()
        .filter_map(|(token, replacement)| {
            remaining
                .find(token)
                .map(|offset| (offset, *token, replacement))
        })
        .min_by_key(|(offset, _, _)| *offset)
    {
        output.push_str(&remaining[..offset]);
        output.push_str(replacement);
        remaining = &remaining[offset + token.len()..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn template_digest() -> String {
    let mut files = template_source_files();
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = Sha256::new();
    for (path, contents) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(contents);
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

fn creation_inputs_digest(input: &NormalizedInput, template_digest: &str) -> String {
    let canonical = serde_json::to_vec(&(
        DISTRIBUTION_VERSION,
        TEMPLATE_IDENTITY,
        template_digest,
        &input.product_name,
        &input.product_id,
        &input.product_source_license,
    ))
    .expect("normalized creation inputs are JSON-encodable");
    hex::encode(Sha256::digest(canonical))
}

pub(crate) fn template_source_files() -> Vec<(String, &'static [u8])> {
    let mut embedded = Vec::new();
    collect_files(&TEMPLATE, &mut embedded);
    let mut files = embedded
        .into_iter()
        .map(|file| {
            (
                file.path()
                    .to_str()
                    .expect("embedded template paths are UTF-8")
                    .to_owned(),
                file.contents(),
            )
        })
        .collect::<Vec<_>>();
    files.push(("LICENSE-APACHE".to_owned(), LICENSE_APACHE));
    files.push(("LICENSE-MIT".to_owned(), LICENSE_MIT));
    files
}

#[derive(serde::Serialize)]
struct DistributionInventory {
    schema_version: u64,
    distribution_version: &'static str,
    template_identity: &'static str,
    manifest_output: &'static str,
    lifecycles: [&'static str; 5],
    path_rule_match_policy: &'static str,
    artifacts: Vec<InventoryArtifact>,
    path_rules: Vec<LifecyclePathRule>,
}

#[derive(serde::Serialize)]
struct InventoryArtifact {
    path: String,
    lifecycle: &'static str,
    mode: &'static str,
    source_sha256: String,
    source_authority: &'static str,
    spdx: &'static str,
    hand_editable_after_creation: bool,
}

#[derive(serde::Serialize)]
struct LifecyclePathRule {
    path_patterns: &'static [&'static str],
    lifecycle: &'static str,
    priority: u64,
    hand_editable: bool,
    workspace_source_authority: bool,
    provenance_authority: &'static str,
    license_notice_authority: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_bytes_license_authority: Option<&'static str>,
}

fn distribution_inventory_json() -> Result<Vec<u8>> {
    let mut artifacts = template_source_files()
        .into_iter()
        .map(|(path, contents)| {
            let path = materialized_template_path(Path::new(&path))
                .to_str()
                .expect("materialized template paths are UTF-8")
                .to_owned();
            let (lifecycle, hand_editable_after_creation) =
                if matches!(path.as_str(), "LICENSE-APACHE" | "LICENSE-MIT") {
                    ("exact-distribution-snapshot", false)
                } else if matches!(
                    path.as_str(),
                    ".yydra/origin.toml"
                        | ".yydra/product-source-license.toml"
                        | ".yydra/api-generation.json"
                        | ".yydra/api-generation-history.json"
                        | ".yydra/api-generation.lock"
                        | "contracts/openapi.json"
                ) || path.starts_with("frontend/src/generated/public-api/")
                {
                    ("committed-generated-output", false)
                } else {
                    ("product-owned-source", true)
                };
            InventoryArtifact {
                path,
                lifecycle,
                mode: "0644",
                source_sha256: hex::encode(Sha256::digest(contents)),
                source_authority: "yydra-distribution-template",
                spdx: SPDX_EXPRESSION,
                hand_editable_after_creation,
            }
        })
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));

    let inventory = DistributionInventory {
        schema_version: 1,
        distribution_version: DISTRIBUTION_VERSION,
        template_identity: TEMPLATE_IDENTITY,
        manifest_output: ".yydra/distribution-inventory.json",
        lifecycles: [
            "product-owned-source",
            "exact-distribution-snapshot",
            "committed-generated-output",
            "ephemeral-generated-output",
            "evidence-build-output",
        ],
        path_rule_match_policy: "highest-priority-match",
        artifacts,
        path_rules: vec![
            LifecyclePathRule {
                path_patterns: &[
                    "Cargo.toml",
                    "Cargo.lock",
                    "migrations/**",
                    "crates/**",
                    "frontend/package.json",
                    "frontend/package-lock.json",
                    "frontend/app/**",
                    "frontend/src/**",
                    "frontend/assets/**",
                    "frontend/app.config.*",
                    ".github/**",
                ],
                lifecycle: "product-owned-source",
                priority: 100,
                hand_editable: true,
                workspace_source_authority: true,
                provenance_authority: "product-team-after-creation",
                license_notice_authority: "workspace-origin-record.product_source_license",
                new_bytes_license_authority: Some("workspace-origin-record.product_source_license"),
            },
            LifecyclePathRule {
                path_patterns: &["LICENSE-*", ".agents/skills/yydra-*/**"],
                lifecycle: "exact-distribution-snapshot",
                priority: 300,
                hand_editable: false,
                workspace_source_authority: false,
                provenance_authority: "exact-yydra-distribution",
                license_notice_authority: "embedded-artifact-spdx-and-retained-notices",
                new_bytes_license_authority: None,
            },
            LifecyclePathRule {
                path_patterns: &[
                    ".yydra/**",
                    "contracts/openapi.json",
                    "frontend/src/generated/public-api/**",
                ],
                lifecycle: "committed-generated-output",
                priority: 300,
                hand_editable: false,
                workspace_source_authority: false,
                provenance_authority: "declared-generator-and-upstream-contract",
                license_notice_authority: "generator-input-provenance-and-retained-notices",
                new_bytes_license_authority: None,
            },
            LifecyclePathRule {
                path_patterns: &[
                    "frontend/android/**",
                    "frontend/ios/**",
                    "frontend/.expo/types/**",
                ],
                lifecycle: "ephemeral-generated-output",
                priority: 300,
                hand_editable: false,
                workspace_source_authority: false,
                provenance_authority: "declared-native-project-generator",
                license_notice_authority: "generator-input-provenance-and-retained-notices",
                new_bytes_license_authority: None,
            },
            LifecyclePathRule {
                path_patterns: &["target/**", "frontend/dist/**", "evidence/**"],
                lifecycle: "evidence-build-output",
                priority: 400,
                hand_editable: false,
                workspace_source_authority: false,
                provenance_authority: "producing-check-or-build",
                license_notice_authority: "producing-inputs-and-tool-notices",
                new_bytes_license_authority: None,
            },
        ],
    };
    let mut output =
        serde_json::to_vec_pretty(&inventory).context("encode Distribution inventory")?;
    output.push(b'\n');
    Ok(output)
}

fn collect_files<'a>(directory: &'a Dir<'a>, files: &mut Vec<&'a File<'a>>) {
    for entry in directory.entries() {
        match entry {
            DirEntry::Dir(child) => collect_files(child, files),
            DirEntry::File(file) => files.push(file),
        }
    }
}

fn doctor(workspace: &Path, reporter: &Reporter) -> Result<()> {
    reporter.phase(
        "doctor.verify",
        "DOCTOR_WORKSPACE_VERIFY",
        Some(workspace),
        Some("restore the reported authority or install the exact Distribution version named by the Workspace Origin Record"),
        || verify_workspace(workspace).map(|_| ()),
    )
}

pub(crate) fn verify_workspace(workspace: &Path) -> Result<(PathBuf, WorkspaceOriginRecord)> {
    let verified = verify_workspace_origin_details(workspace)?;
    verify_snapshot_authorities_with_origin(
        &verified.root,
        &verified.origin,
        &verified.normalized,
        &verified.expected_template,
    )?;
    Ok((verified.root, verified.origin))
}

pub(crate) fn verify_origin_authority(workspace: &Path) -> Result<()> {
    verify_workspace_origin_details(workspace).map(|_| ())
}

pub(crate) fn verify_snapshot_authorities(root: &Path) -> Result<()> {
    let origin = read_workspace_origin_record(root)?;
    let normalized = NormalizedInput::new(
        &origin.product_name,
        &origin.product_id,
        &origin.product_source_license,
    )
    .context("Workspace Origin Record has invalid normalized creation inputs")?;
    verify_snapshot_authorities_with_origin(root, &origin, &normalized, &template_digest())
}

struct VerifiedWorkspaceOrigin {
    root: PathBuf,
    origin: WorkspaceOriginRecord,
    normalized: NormalizedInput,
    expected_template: String,
}

fn verify_workspace_origin_details(workspace: &Path) -> Result<VerifiedWorkspaceOrigin> {
    let root = find_workspace_root(workspace)?;
    let origin = read_workspace_origin_record(&root)?;
    semver::Version::parse(&origin.distribution_version)
        .context("Workspace Origin Record has invalid distribution version")?;

    if origin.distribution_version != DISTRIBUTION_VERSION {
        bail!(
            "distribution mismatch; install exactly with: cargo install yydra-cli --version {} --locked",
            origin.distribution_version
        );
    }
    if origin.schema_version != 1 {
        bail!(
            "origin schema mismatch: expected 1, found {}; install exactly with: cargo install yydra-cli --version {DISTRIBUTION_VERSION} --locked",
            origin.schema_version
        );
    }
    if origin.template_identity != TEMPLATE_IDENTITY {
        bail!(
            "template identity mismatch: expected {TEMPLATE_IDENTITY}, found {}; install exactly with: cargo install yydra-cli --version {DISTRIBUTION_VERSION} --locked",
            origin.template_identity
        );
    }
    let expected_template = template_digest();
    if origin.template_sha256 != expected_template {
        bail!(
            "template digest mismatch for Distribution {DISTRIBUTION_VERSION}; install exactly with: cargo install yydra-cli --version {DISTRIBUTION_VERSION} --locked"
        );
    }
    let normalized = NormalizedInput::new(
        &origin.product_name,
        &origin.product_id,
        &origin.product_source_license,
    )
    .context("Workspace Origin Record has invalid normalized creation inputs")?;
    if normalized.product_name != origin.product_name
        || normalized.product_id != origin.product_id
        || normalized.product_source_license != origin.product_source_license
    {
        bail!("Workspace Origin Record creation inputs are not normalized");
    }
    if origin.creation_inputs_sha256 != creation_inputs_digest(&normalized, &expected_template) {
        bail!(
            "Workspace Origin Record creation fingerprint mismatch; restore the reviewed generated record from version control"
        );
    }
    Ok(VerifiedWorkspaceOrigin {
        root,
        origin,
        normalized,
        expected_template,
    })
}

fn verify_snapshot_authorities_with_origin(
    root: &Path,
    origin: &WorkspaceOriginRecord,
    normalized: &NormalizedInput,
    expected_template: &str,
) -> Result<()> {
    for (relative, expected) in [
        ("LICENSE-APACHE", LICENSE_APACHE),
        ("LICENSE-MIT", LICENSE_MIT),
    ] {
        let path = root.join(relative);
        let actual = fs::read(&path)
            .with_context(|| format!("read exact Distribution snapshot '{}'", path.display()))?;
        if actual != expected {
            bail!(
                "exact Distribution snapshot drift at '{relative}'; restore its reviewed bytes from the yydra-cli {DISTRIBUTION_VERSION} package"
            );
        }
    }
    let inventory_path = root.join(".yydra/distribution-inventory.json");
    let actual_inventory = fs::read(&inventory_path).with_context(|| {
        format!(
            "read committed generated inventory '{}'",
            inventory_path.display()
        )
    })?;
    if actual_inventory != distribution_inventory_json()? {
        bail!(
            "committed generated inventory drift at '.yydra/distribution-inventory.json'; restore the reviewed generated file from version control"
        );
    }
    let policy_relative = Path::new(".yydra/product-source-license.toml");
    let policy_template = TEMPLATE
        .get_file(policy_relative)
        .expect("embedded product source license policy");
    let policy_source = std::str::from_utf8(policy_template.contents())
        .expect("embedded product source license policy is UTF-8");
    let render = RenderContext {
        input: normalized,
        template_digest: expected_template,
    };
    let expected_policy = render_template(policy_source, &render)?;
    let policy_path = root.join(policy_relative);
    let actual_policy = fs::read_to_string(&policy_path).with_context(|| {
        format!(
            "read committed generated provenance '{}'",
            policy_path.display()
        )
    })?;
    if actual_policy != expected_policy {
        bail!(
            "committed generated provenance drift at '.yydra/product-source-license.toml'; restore the reviewed generated file from version control"
        );
    }
    if origin.product_source_license != normalized.product_source_license {
        bail!("Workspace provenance license does not match its normalized Origin Record")
    }
    Ok(())
}

fn setup(workspace: &Path, reporter: &Reporter) -> Result<()> {
    let root = reporter.phase(
        "setup.verify-workspace",
        "SETUP_WORKSPACE_VERIFY",
        Some(workspace),
        Some("run `yydra doctor` and resolve the reported exact-Distribution mismatch"),
        || verify_workspace(workspace).map(|(root, _)| root),
    )?;
    let shutdown = install_shutdown_handler().context("install setup shutdown handler")?;
    let cargo_lock = root.join("Cargo.lock");
    let npm_lock = root.join("frontend/package-lock.json");
    let (cargo_before, npm_before) = reporter.phase(
        "setup.snapshot-locks",
        "SETUP_LOCK_SNAPSHOT",
        Some(&root),
        Some("restore both committed lock files from version control before setup"),
        || {
            let cargo_before = fs::read(&cargo_lock)
                .with_context(|| format!("read committed Cargo lock '{}'", cargo_lock.display()))?;
            let npm_before = fs::read(&npm_lock)
                .with_context(|| format!("read committed npm lock '{}'", npm_lock.display()))?;
            Ok((cargo_before, npm_before))
        },
    )?;

    let command_result = (|| {
        reporter.phase(
            "setup.cargo",
            "SETUP_CARGO_FETCH",
            Some(&cargo_lock),
            Some("restore Cargo.lock and retry with the exact locked dependency graph"),
            || run_process(&root, "cargo", &["fetch", "--locked"], reporter, &shutdown),
        )?;
        reporter.phase(
            "setup.npm",
            "SETUP_NPM_CI",
            Some(&npm_lock),
            Some("restore frontend/package-lock.json and retry `yydra setup`"),
            || {
                run_process(
                    &root.join("frontend"),
                    npm_program(),
                    &["ci"],
                    reporter,
                    &shutdown,
                )
            },
        )
    })();
    let integrity_result = reporter.phase(
        "setup.verify-locks",
        "SETUP_LOCK_INTEGRITY",
        Some(&root),
        Some("restore both committed locks; setup never accepts a rewritten resolution"),
        || verify_and_restore_locks(&[(&cargo_lock, &cargo_before), (&npm_lock, &npm_before)]),
    );
    match (command_result, integrity_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(command_error), Ok(())) => Err(command_error),
        (Ok(()), Err(integrity_error)) => Err(integrity_error),
        (Err(command_error), Err(integrity_error)) => Err(integrity_error).with_context(|| {
            format!("setup command also failed before lock verification: {command_error:#}")
        }),
    }
}

fn verify_and_restore_locks(locks: &[(&Path, &[u8])]) -> Result<()> {
    let mut drifted = Vec::new();
    for (path, committed) in locks {
        let unchanged = fs::read(path)
            .map(|actual| actual == *committed)
            .unwrap_or(false);
        if unchanged {
            continue;
        }
        fs::write(path, committed)
            .with_context(|| format!("restore committed lock '{}'", path.display()))?;
        drifted.push(path.display().to_string());
    }
    if !drifted.is_empty() {
        bail!(
            "setup tool changed committed lock file(s); original bytes restored: {}",
            drifted.join(", ")
        );
    }
    Ok(())
}

fn db_migration_add(workspace: &Path, name: &str, reporter: &Reporter) -> Result<()> {
    let (root, origin) = reporter.phase(
        "db.migration.verify-workspace",
        "DB_WORKSPACE_VERIFY",
        Some(workspace),
        Some("run `yydra doctor` and resolve the reported Workspace mismatch"),
        || verify_workspace(workspace),
    )?;
    let migrations = root.join("migrations");
    let next_version = reporter.phase(
        "db.migration.plan",
        "DB_MIGRATION_PLAN",
        Some(&migrations),
        Some("use a lowercase snake_case name and inspect existing sequential migration files"),
        || {
            validate_migration_name(name)?;
            next_migration_version(&migrations)
        },
    )?;
    let path = migrations.join(format!("{next_version:04}_{name}.sql"));
    reporter.phase(
        "db.migration.create",
        "DB_MIGRATION_STUB_CREATE",
        Some(&path),
        Some("choose another migration name only after inspecting the existing committed files"),
        || {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .with_context(|| format!("create migration stub '{}'", path.display()))?;
            writeln!(
                file,
                "-- SPDX-License-Identifier: {}\n\n-- Add migration SQL here.",
                origin.product_source_license
            )
            .with_context(|| format!("write migration stub '{}'", path.display()))?;
            set_mode(&path, 0o644)
                .with_context(|| format!("set migration mode '{}'", path.display()))
        },
    )
}

fn next_migration_version(migrations: &Path) -> Result<i64> {
    let mut versions = BTreeSet::new();
    for entry in fs::read_dir(migrations)
        .with_context(|| format!("read migrations directory '{}'", migrations.display()))?
    {
        let entry = entry.context("read migration directory entry")?;
        if !entry
            .file_type()
            .with_context(|| format!("read migration type '{}'", entry.path().display()))?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            if entry.path().extension() == Some(OsStr::new("sql")) {
                bail!("migration SQL filename is not valid UTF-8");
            }
            continue;
        };
        let Some(stem) = name.strip_suffix(".sql") else {
            continue;
        };
        let Some((version, description)) = stem.split_once('_') else {
            continue;
        };
        if description.is_empty() {
            bail!("migration filename '{name}' has an empty description");
        }
        let version = version
            .parse::<i64>()
            .with_context(|| format!("migration filename '{name}' has an invalid version"))?;
        if version <= 0 {
            bail!("migration filename '{name}' must use a positive version");
        }
        if !versions.insert(version) {
            bail!("migration history contains duplicate version {version}");
        }
    }
    versions
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .context("migration version overflow")
}

fn validate_migration_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && name.as_bytes()[0].is_ascii_lowercase()
        && name.as_bytes()[name.len() - 1].is_ascii_alphanumeric();
    if !valid {
        bail!(
            "migration name must start with a lowercase letter, end with a letter or digit, contain only lowercase ASCII letters, digits, or internal underscores, and be at most 63 characters"
        );
    }
    Ok(())
}

fn db_migrate(workspace: &Path, reporter: &Reporter) -> Result<()> {
    let root = reporter.phase(
        "db.migrate.verify-workspace",
        "DB_WORKSPACE_VERIFY",
        Some(workspace),
        Some("run `yydra doctor` and resolve the reported Workspace mismatch"),
        || verify_workspace(workspace).map(|(root, _)| root),
    )?;
    let shutdown = install_shutdown_handler().context("install migration shutdown handler")?;
    reporter.phase(
        "db.migrate.apply",
        "DB_MIGRATE_APPLY",
        Some(&root.join("migrations")),
        Some("verify DATABASE_URL and inspect the committed migration history before retrying"),
        || {
            run_process(
                &root,
                "cargo",
                &["run", "--locked", "--bin", "migrate"],
                reporter,
                &shutdown,
            )
        },
    )
}

fn dev(workspace: &Path, reporter: &Reporter) -> Result<()> {
    let root = reporter.phase(
        "dev.verify-workspace",
        "DEV_WORKSPACE_VERIFY",
        Some(workspace),
        Some("run `yydra doctor` and resolve the reported Workspace mismatch"),
        || verify_workspace(workspace).map(|(root, _)| root),
    )?;
    let shutdown = install_shutdown_handler().context("install development shutdown handler")?;
    if run_dev_migration(&root, reporter, &shutdown)? {
        return Ok(());
    }

    let mut backend = spawn_reported_dev_child(
        &root,
        "cargo",
        &["run", "--locked", "--bin", "server"],
        "dev.backend",
        "DEV_BACKEND",
        "backend",
        reporter,
    )?;
    let mut frontend = match spawn_reported_dev_child(
        &root.join("frontend"),
        npm_program(),
        &["run", "web"],
        "dev.frontend",
        "DEV_FRONTEND",
        "frontend",
        reporter,
    ) {
        Ok(child) => child,
        Err(error) => {
            reporter.emit(Diagnostic {
                phase: "dev.shutdown",
                code: "DEV_PEER_SHUTDOWN",
                severity: "info",
                status: "started",
                message: "terminating backend after frontend spawn failure",
                location: Some(&root),
                remediation: None,
            });
            terminate_child(&mut backend)
                .context("terminate backend after frontend start failure")?;
            reporter.emit(Diagnostic {
                phase: "dev.shutdown",
                code: "DEV_PEER_SHUTDOWN",
                severity: "info",
                status: "pass",
                message: "backend terminated after frontend spawn failure",
                location: Some(&root),
                remediation: None,
            });
            return Err(error).context("start frontend development process");
        }
    };

    loop {
        if shutdown.load(Ordering::SeqCst) {
            reporter.emit(Diagnostic {
                phase: "dev.shutdown",
                code: "DEV_PEER_SHUTDOWN",
                severity: "info",
                status: "started",
                message: "shutdown requested; terminating child processes",
                location: Some(&root),
                remediation: None,
            });
            terminate_child(&mut frontend).context("terminate frontend during shutdown")?;
            terminate_child(&mut backend).context("terminate backend during shutdown")?;
            reporter.emit(Diagnostic {
                phase: "dev.shutdown",
                code: "DEV_PEER_SHUTDOWN",
                severity: "info",
                status: "pass",
                message: "all development child processes terminated",
                location: Some(&root),
                remediation: None,
            });
            return Ok(());
        }
        if let Some(status) = backend.try_wait().context("poll backend child")? {
            return dev_child_exited(
                ExitedDevChild {
                    name: "backend",
                    code: "DEV_BACKEND",
                    status,
                    location: &root.join("crates/server"),
                },
                &mut backend,
                &mut frontend,
                reporter,
            );
        }
        if let Some(status) = frontend.try_wait().context("poll frontend child")? {
            return dev_child_exited(
                ExitedDevChild {
                    name: "frontend",
                    code: "DEV_FRONTEND",
                    status,
                    location: &root.join("frontend"),
                },
                &mut frontend,
                &mut backend,
                reporter,
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn install_shutdown_handler() -> Result<Arc<AtomicBool>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&shutdown);
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
        .context("install shutdown handler")?;
    Ok(shutdown)
}

fn run_dev_migration(root: &Path, reporter: &Reporter, shutdown: &AtomicBool) -> Result<bool> {
    let location = root.join("migrations");
    let mut migration = spawn_reported_dev_child(
        root,
        "cargo",
        &["run", "--locked", "--bin", "migrate"],
        "dev.migration",
        "DEV_MIGRATION",
        "migration",
        reporter,
    )?;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            reporter.emit(Diagnostic {
                phase: "dev.shutdown",
                code: "DEV_PEER_SHUTDOWN",
                severity: "info",
                status: "started",
                message: "shutdown requested; terminating migration child",
                location: Some(root),
                remediation: None,
            });
            terminate_child(&mut migration).context("terminate migration during shutdown")?;
            reporter.emit(Diagnostic {
                phase: "dev.shutdown",
                code: "DEV_PEER_SHUTDOWN",
                severity: "info",
                status: "pass",
                message: "migration child terminated before backend or frontend startup",
                location: Some(root),
                remediation: None,
            });
            return Ok(true);
        }
        if let Some(status) = migration.try_wait().context("poll migration child")? {
            terminate_child(&mut migration)
                .context("terminate descendants after migration child exit")?;
            if status.success() {
                reporter.emit(Diagnostic {
                    phase: "dev.migration",
                    code: "DEV_MIGRATION",
                    severity: "info",
                    status: "pass",
                    message: "migration child completed",
                    location: Some(&location),
                    remediation: None,
                });
                return Ok(false);
            }
            let message = format!(
                "migration child exited with {}",
                status
                    .code()
                    .map_or_else(|| "signal".to_owned(), |code| code.to_string())
            );
            reporter.emit(Diagnostic {
                phase: "dev.migration",
                code: "DEV_MIGRATION",
                severity: "error",
                status: "fail",
                message: &message,
                location: Some(&location),
                remediation: Some(
                    "verify DATABASE_URL and run `yydra db migrate` for focused diagnostics",
                ),
            });
            bail!(message);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn spawn_reported_dev_child(
    directory: &Path,
    program: &str,
    arguments: &[&str],
    phase: &str,
    code: &str,
    child_name: &str,
    reporter: &Reporter,
) -> Result<ManagedChild> {
    reporter.emit(Diagnostic {
        phase,
        code,
        severity: "info",
        status: "started",
        message: &format!("starting {child_name} child"),
        location: Some(directory),
        remediation: None,
    });
    match spawn_dev_child(directory, program, arguments, reporter) {
        Ok(child) => Ok(child),
        Err(error) => {
            let message = format!("could not spawn {child_name} child: {error:#}");
            reporter.emit(Diagnostic {
                phase,
                code,
                severity: "error",
                status: "fail",
                message: &message,
                location: Some(directory),
                remediation: Some(
                    "verify the locked tool installation and executable permissions, then rerun `yydra dev`",
                ),
            });
            Err(error)
        }
    }
}

fn spawn_dev_child(
    directory: &Path,
    program: &str,
    arguments: &[&str],
    reporter: &Reporter,
) -> Result<ManagedChild> {
    let mut command = ProcessCommand::new(program);
    command.args(arguments).current_dir(directory);
    #[cfg(unix)]
    command.process_group(0);
    if reporter.format == MessageFormat::Json {
        command.stdout(Stdio::piped()).stderr(Stdio::inherit());
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("start {program} {}", arguments.join(" ")))?;
    #[cfg(windows)]
    let job = match create_kill_on_close_job(&child) {
        Ok(job) => Some(job),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error).context("place child in a kill-on-close Windows Job Object");
        }
    };
    let forwarder = if reporter.format == MessageFormat::Json {
        let mut stdout = child.stdout.take().context("capture child stdout")?;
        Some(thread::spawn(move || forward_child_output(&mut stdout)))
    } else {
        None
    };
    Ok(ManagedChild {
        child,
        armed: true,
        forwarder,
        #[cfg(windows)]
        job,
    })
}

struct ManagedChild {
    child: Child,
    armed: bool,
    forwarder: Option<thread::JoinHandle<()>>,
    #[cfg(windows)]
    job: Option<WindowsJob>,
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        #[cfg(unix)]
        {
            use nix::sys::signal::{Signal, killpg};
            use nix::unistd::Pid;

            if let Ok(process_group) = i32::try_from(self.child.id()) {
                let _ = killpg(Pid::from_raw(process_group), Signal::SIGKILL);
            }
            let _ = self.child.wait();
            if let Some(forwarder) = self.forwarder.take() {
                let _ = forwarder.join();
            }
        }
        #[cfg(windows)]
        {
            drop(self.job.take());
            let _ = self.child.wait();
            if let Some(forwarder) = self.forwarder.take() {
                let _ = forwarder.join();
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(forwarder) = self.forwarder.take() {
                let _ = forwarder.join();
            }
        }
    }
}

fn join_child_output(child: &mut ManagedChild) -> Result<()> {
    let Some(forwarder) = child.forwarder.take() else {
        return Ok(());
    };
    forwarder
        .join()
        .map_err(|_| anyhow::anyhow!("child output forwarder panicked"))
}

fn forward_child_output(stdout: &mut impl Read) {
    let mut buffer = [0_u8; 8192];
    let mut sink_open = true;
    loop {
        let read = match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if sink_open {
            sink_open = write_forwarded_chunk(&buffer[..read]);
        }
    }
}

fn write_forwarded_chunk(bytes: &[u8]) -> bool {
    let mut stderr = io::stderr();
    write_forwarded_chunk_to(&mut stderr, bytes, Duration::from_secs(1))
}

fn write_forwarded_chunk_to(writer: &mut impl Write, mut bytes: &[u8], max_wait: Duration) -> bool {
    let deadline = Instant::now() + max_wait;
    while !bytes.is_empty() {
        match writer.write(bytes) {
            Ok(0) => return false,
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return false;
                }
                thread::sleep(Duration::from_millis(1));
            }
            Err(_) => return false,
        }
    }
    true
}

impl Deref for ManagedChild {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

#[cfg(windows)]
pub(crate) struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
pub(crate) fn create_kill_on_close_job(child: &Child) -> Result<WindowsJob> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use std::ptr;

    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error()).context("create Windows Job Object");
    }
    let job = WindowsJob(handle);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .context("Windows Job Object configuration size overflow")?,
        )
    };
    if configured == 0 {
        return Err(io::Error::last_os_error()).context("configure Windows Job Object");
    }
    let assigned = unsafe {
        AssignProcessToJobObject(
            job.0,
            child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
        )
    };
    if assigned == 0 {
        return Err(io::Error::last_os_error()).context("assign child to Windows Job Object");
    }
    Ok(job)
}

struct ExitedDevChild<'a> {
    name: &'a str,
    code: &'a str,
    status: std::process::ExitStatus,
    location: &'a Path,
}

fn dev_child_exited(
    exit: ExitedDevChild<'_>,
    exited: &mut ManagedChild,
    peer: &mut ManagedChild,
    reporter: &Reporter,
) -> Result<()> {
    let message = format!(
        "{} child exited unexpectedly with {}",
        exit.name,
        exit.status
            .code()
            .map_or_else(|| "signal".to_owned(), |code| code.to_string())
    );
    reporter.emit(Diagnostic {
        phase: &format!("dev.{}", exit.name),
        code: exit.code,
        severity: "error",
        status: "fail",
        message: &message,
        location: Some(exit.location),
        remediation: Some(
            "inspect the forwarded child diagnostics, correct the failure, and rerun `yydra dev`",
        ),
    });
    reporter.emit(Diagnostic {
        phase: "dev.shutdown",
        code: "DEV_PEER_SHUTDOWN",
        severity: "info",
        status: "started",
        message: "terminating development child process groups after a child exit",
        location: exit.location.parent(),
        remediation: None,
    });
    terminate_child(exited).context("terminate descendants of exited development child")?;
    terminate_child(peer).context("terminate remaining development child")?;
    reporter.emit(Diagnostic {
        phase: "dev.shutdown",
        code: "DEV_PEER_SHUTDOWN",
        severity: "info",
        status: "pass",
        message: "development child process groups terminated",
        location: exit.location.parent(),
        remediation: None,
    });
    bail!(message)
}

fn terminate_child(child: &mut ManagedChild) -> Result<()> {
    let already_reaped = child
        .try_wait()
        .context("poll child before termination")?
        .is_some();

    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        let process_group = Pid::from_raw(
            i32::try_from(child.id()).context("child process id does not fit Unix pid_t")?,
        );
        if let Err(error) = killpg(process_group, Signal::SIGTERM)
            && error != Errno::ESRCH
        {
            return Err(error).context("signal child process group");
        }
        let mut reaped = already_reaped;
        for _ in 0..40 {
            if !reaped && child.try_wait().context("poll signalled child")?.is_some() {
                reaped = true;
            }
            match killpg(process_group, None) {
                Err(Errno::ESRCH) => {
                    if !reaped {
                        child.wait().context("reap terminated child")?;
                    }
                    child.armed = false;
                    return join_child_output(child);
                }
                Ok(()) => {}
                Err(error) => return Err(error).context("probe child process group"),
            }
            thread::sleep(Duration::from_millis(25));
        }
        if let Err(error) = killpg(process_group, Signal::SIGKILL)
            && error != Errno::ESRCH
        {
            return Err(error).context("force-terminate child process group");
        }
        if !reaped {
            child.wait().context("reap terminated child")?;
        }
        child.armed = false;
        join_child_output(child)
    }

    #[cfg(windows)]
    {
        drop(child.job.take());
        if !already_reaped {
            child.wait().context("reap terminated Windows child")?;
        }
        child.armed = false;
        join_child_output(child)
    }

    #[cfg(not(any(unix, windows)))]
    {
        if already_reaped {
            child.armed = false;
            return join_child_output(child);
        }
        child.kill().context("terminate child")?;
        child.wait().context("reap terminated child")?;
        child.armed = false;
        join_child_output(child)
    }
}

pub(crate) fn npm_program() -> &'static str {
    npm_program_for(cfg!(windows))
}

fn npm_program_for(windows: bool) -> &'static str {
    if windows { "npm.cmd" } else { "npm" }
}

fn run_process(
    directory: &Path,
    program: &str,
    arguments: &[&str],
    reporter: &Reporter,
    shutdown: &AtomicBool,
) -> Result<()> {
    let mut child = spawn_dev_child(directory, program, arguments, reporter)?;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            terminate_child(&mut child)
                .with_context(|| format!("terminate {program} after shutdown request"))?;
            bail!(
                "shutdown requested while running {program} {}",
                arguments.join(" ")
            );
        }
        if let Some(status) = child.try_wait().context("poll supervised child")? {
            terminate_child(&mut child)
                .with_context(|| format!("terminate descendants after {program} exit"))?;
            if !status.success() {
                bail!(
                    "{program} {} exited with {}",
                    arguments.join(" "),
                    status
                        .code()
                        .map_or_else(|| "signal".to_owned(), |code| code.to_string())
                );
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(crate) fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let start = start
        .canonicalize()
        .with_context(|| format!("resolve workspace path '{}'", start.display()))?;
    for candidate in start.ancestors() {
        if candidate.join(".yydra/origin.toml").is_file() {
            return Ok(candidate.to_path_buf());
        }
    }
    bail!(
        "no .yydra/origin.toml found at or above '{}'",
        start.display()
    )
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceOriginRecord {
    schema_version: u64,
    distribution_version: String,
    template_identity: String,
    template_sha256: String,
    creation_inputs_sha256: String,
    product_name: String,
    product_id: String,
    product_source_license: String,
}

fn read_workspace_origin_record(root: &Path) -> Result<WorkspaceOriginRecord> {
    let origin_path = root.join(".yydra/origin.toml");
    let origin = fs::read_to_string(&origin_path)
        .with_context(|| format!("read Workspace Origin Record '{}'", origin_path.display()))?;
    toml::from_str(&origin).context("invalid Workspace Origin Record")
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::time::Duration;

    use super::{npm_program_for, write_forwarded_chunk_to};

    #[test]
    fn npm_executable_uses_the_windows_command_shim() {
        assert_eq!(npm_program_for(true), "npm.cmd");
        assert_eq!(npm_program_for(false), "npm");
    }

    #[test]
    fn diagnostic_sink_backpressure_never_changes_child_status() {
        struct WouldBlockOnce {
            blocked: bool,
            output: Vec<u8>,
        }

        impl Write for WouldBlockOnce {
            fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
                if !self.blocked {
                    self.blocked = true;
                    return Err(io::Error::from(io::ErrorKind::WouldBlock));
                }
                self.output.extend_from_slice(buffer);
                Ok(buffer.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut sink = WouldBlockOnce {
            blocked: false,
            output: Vec::new(),
        };
        assert!(write_forwarded_chunk_to(
            &mut sink,
            b"complete detail",
            Duration::from_secs(1),
        ));
        assert_eq!(sink.output, b"complete detail");

        struct AlwaysWouldBlock;

        impl Write for AlwaysWouldBlock {
            fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::WouldBlock))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        assert!(!write_forwarded_chunk_to(
            &mut AlwaysWouldBlock,
            b"discardable detail",
            Duration::ZERO,
        ));
    }
}
