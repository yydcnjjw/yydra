// SPDX-License-Identifier: MIT OR Apache-2.0

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use include_dir::{Dir, DirEntry, File, include_dir};
use sha2::{Digest, Sha256};

const DISTRIBUTION_VERSION: &str = env!("CARGO_PKG_VERSION");
const TEMPLATE_IDENTITY: &str = "yydra-v0-product-workspace";
const TEMPLATE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/template/product-workspace");
const SPDX_EXPRESSION: &str = "MIT OR Apache-2.0";
const LICENSE_MIT: &[u8] = include_bytes!("../LICENSE-MIT");
const LICENSE_APACHE: &[u8] = include_bytes!("../LICENSE-APACHE");

#[derive(Debug, Parser)]
#[command(name = "yydra", version, about = "Yydra V0 Distribution CLI")]
struct Cli {
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::New {
            destination,
            product_name,
            product_id,
            product_source_license,
        } => {
            let input = resolve_input(product_name, product_id, product_source_license)?;
            create_workspace(&destination, &input)
        }
        Command::Doctor { workspace } => doctor(&workspace),
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

    println!("created Product Workspace at {}", destination.display());
    println!("distribution={DISTRIBUTION_VERSION}");
    println!("template_sha256={template_digest}");
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
                let output = destination.join(file.path());
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

fn template_source_files() -> Vec<(String, &'static [u8])> {
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
            let (lifecycle, hand_editable_after_creation) = match path.as_str() {
                "LICENSE-APACHE" | "LICENSE-MIT" => ("exact-distribution-snapshot", false),
                ".yydra/origin.toml" | ".yydra/product-source-license.toml" => {
                    ("committed-generated-output", false)
                }
                _ => ("product-owned-source", true),
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

fn doctor(workspace: &Path) -> Result<()> {
    let root = find_workspace_root(workspace)?;
    let origin = read_workspace_origin_record(&root)?;
    semver::Version::parse(&origin.distribution_version)
        .context("Workspace Origin Record has invalid distribution version")?;

    println!("workspace={}", root.display());
    println!("origin_distribution={}", origin.distribution_version);
    println!("cli_distribution={DISTRIBUTION_VERSION}");
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
        input: &normalized,
        template_digest: &expected_template,
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
    println!("status=pass");
    Ok(())
}

fn find_workspace_root(start: &Path) -> Result<PathBuf> {
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
