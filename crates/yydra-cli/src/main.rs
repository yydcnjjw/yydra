use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use include_dir::{Dir, DirEntry, File, include_dir};
use sha2::{Digest, Sha256};

mod check_graph;

use check_graph::{CheckProfile, CheckRequest};

pub(crate) const DISTRIBUTION_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const TEMPLATE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../../template/product-workspace");
pub(crate) const BASELINE_SKILL_FILES: &[&str] = &[
    "yydra-diagnose/SKILL.md",
    "yydra-diagnose/references/rule-routing.md",
    "yydra-product-change/SKILL.md",
    "yydra-product-change/references/change-loop.md",
    "yydra-product-change/references/product-presentation-accessibility.md",
    "yydra-product-change/references/workspace-map.md",
];

#[derive(Debug, Parser)]
#[command(name = "yydra", version, about = "PROTOTYPE Yydra V0 Distribution CLI")]
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
        product_name: String,
        #[arg(long)]
        product_id: String,
    },
    /// Diagnose the current Product Workspace without mutating it.
    Doctor {
        #[arg(default_value = ".")]
        workspace: PathBuf,
    },
    /// Regenerate committed artifacts owned by the exact Distribution.
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Run the read-only prototype Mechanical Quality Contract.
    Check {
        #[arg(default_value = ".")]
        workspace: PathBuf,
        /// Select local execution, one CI host shard, or shard aggregation.
        #[arg(long, value_enum, default_value_t = CheckProfile::Local)]
        profile: CheckProfile,
        /// New directory for the evidence manifest and raw node logs.
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
        /// Host-shard manifest to merge; valid only with `--profile aggregate`.
        #[arg(long = "shard", value_name = "MANIFEST")]
        shards: Vec<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum GenerateCommand {
    /// Regenerate code-first OpenAPI and the Orval/Zod client.
    Api {
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
        } => create_workspace(&destination, &product_name, &product_id),
        Command::Doctor { workspace } => doctor(&workspace),
        Command::Generate { command } => match command {
            GenerateCommand::Api { workspace } => generate_api(&workspace),
        },
        Command::Check {
            workspace,
            profile,
            evidence_dir,
            shards,
        } => check_graph::check(CheckRequest {
            workspace,
            profile,
            evidence_dir,
            shards,
        }),
    }
}

fn exact_workspace(start: &Path) -> Result<PathBuf> {
    let root = find_workspace_root(start)?;
    let recorded_version = origin_distribution(&root)?;
    if recorded_version != DISTRIBUTION_VERSION {
        bail!(
            "distribution mismatch; install exactly with: cargo install yydra-cli --version {recorded_version} --locked"
        );
    }
    let recorded_template = origin_template_sha256(&root)?;
    let expected_template = template_digest();
    if recorded_template != expected_template {
        bail!(
            "Workspace Origin Record template digest does not match Distribution {DISTRIBUTION_VERSION}"
        );
    }
    Ok(root)
}

fn generate_api(workspace: &Path) -> Result<()> {
    let root = exact_workspace(workspace)?;
    let openapi = root.join("contracts/openapi.json");
    let client = root.join("frontend/src/generated/public-api");
    let nonce = std::process::id();
    let staged_openapi = root
        .join("contracts")
        .join(format!(".openapi-yydra-generate-{nonce}.json"));
    let openapi_backup = root
        .join("contracts")
        .join(format!(".openapi-yydra-backup-{nonce}.json"));
    let stage_root = root
        .join("frontend/src")
        .join(format!(".yydra-generate-{nonce}"));
    let staged_client = stage_root.join("public-api");
    let client_backup = root
        .join("frontend/src")
        .join(format!(".yydra-client-backup-{nonce}"));
    for path in [
        &staged_openapi,
        &openapi_backup,
        &stage_root,
        &client_backup,
    ] {
        if path.exists() {
            bail!(
                "generation staging path already exists: {}; inspect it before retrying",
                path.display()
            );
        }
    }

    let staged_openapi_arg = staged_openapi.to_string_lossy().into_owned();
    let staged_client_arg = staged_client.to_string_lossy().into_owned();
    let generation_result = run(
        &root,
        "cargo",
        &[
            "run",
            "--locked",
            "--bin",
            "generate-openapi",
            "--",
            &staged_openapi_arg,
        ],
        &[("SQLX_OFFLINE", "true")],
    )
    .and_then(|()| {
        run(
            &root.join("frontend"),
            "npm",
            &["run", "generate:api"],
            &[
                ("YYDRA_OPENAPI_INPUT", staged_openapi_arg.as_str()),
                ("YYDRA_GENERATED_API_ROOT", staged_client_arg.as_str()),
            ],
        )
    });
    if let Err(error) = generation_result {
        remove_path_if_exists(&staged_openapi)?;
        remove_path_if_exists(&stage_root)?;
        return Err(
            error.context("staged API generation failed; committed artifacts are unchanged")
        );
    }

    commit_generated_api(
        &openapi,
        &staged_openapi,
        &openapi_backup,
        &client,
        &staged_client,
        &client_backup,
    )?;
    remove_path_if_exists(&stage_root)?;

    println!("generated={}", openapi.display());
    println!(
        "generated={}",
        root.join("frontend/src/generated/public-api").display()
    );
    println!("status=pass");
    Ok(())
}

fn commit_generated_api(
    openapi: &Path,
    staged_openapi: &Path,
    openapi_backup: &Path,
    client: &Path,
    staged_client: &Path,
    client_backup: &Path,
) -> Result<()> {
    fs::rename(openapi, openapi_backup).with_context(|| {
        format!(
            "stage current OpenAPI '{}' as backup '{}'",
            openapi.display(),
            openapi_backup.display()
        )
    })?;
    if let Err(error) = fs::rename(staged_openapi, openapi) {
        fs::rename(openapi_backup, openapi)
            .context("restore OpenAPI after staged replacement failed")?;
        return Err(error).context("replace committed OpenAPI from staged output");
    }

    if let Err(error) = fs::rename(client, client_backup) {
        remove_path_if_exists(openapi)?;
        fs::rename(openapi_backup, openapi)
            .context("restore OpenAPI after client backup failed")?;
        return Err(error).context("stage current Generated Client as backup");
    }
    if let Err(error) = fs::rename(staged_client, client) {
        fs::rename(client_backup, client)
            .context("restore Generated Client after staged replacement failed")?;
        remove_path_if_exists(openapi)?;
        fs::rename(openapi_backup, openapi)
            .context("restore OpenAPI after client replacement failed")?;
        return Err(error).context("replace committed Generated Client from staged output");
    }

    remove_path_if_exists(openapi_backup)?;
    remove_path_if_exists(client_backup)?;
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("remove directory '{}'", path.display()))
    } else if path.exists() {
        fs::remove_file(path).with_context(|| format!("remove file '{}'", path.display()))
    } else {
        Ok(())
    }
}

fn create_workspace(destination: &Path, product_name: &str, product_id: &str) -> Result<()> {
    validate_product_id(product_id)?;
    if product_name.trim().is_empty() {
        bail!("product name must not be empty");
    }
    if destination.exists() {
        bail!(
            "destination '{}' already exists; prototype creation never merges or overwrites",
            destination.display()
        );
    }

    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create destination parent '{}'", parent.display()))?;
    let destination_name = destination
        .file_name()
        .and_then(OsStr::to_str)
        .context("destination must end in a UTF-8 path component")?;
    let stage = parent.join(format!(
        ".yydra-stage-{destination_name}-{}",
        std::process::id()
    ));
    if stage.exists() {
        bail!("staging path '{}' already exists", stage.display());
    }

    let template_digest = template_digest();
    let native_id = product_id.replace('-', "");
    let render = RenderContext {
        product_name,
        product_id,
        native_id: &native_id,
        template_digest: &template_digest,
    };

    let result: Result<()> = (|| {
        fs::create_dir(&stage).with_context(|| format!("create stage '{}'", stage.display()))?;
        materialize(&TEMPLATE, &stage, &render)?;
        fs::rename(&stage, destination).with_context(|| {
            format!(
                "atomically move staged workspace '{}' to '{}'",
                stage.display(),
                destination.display()
            )
        })?;
        Ok(())
    })();

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

fn validate_product_id(value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric();
    if !valid {
        bail!(
            "product id must start with a lowercase letter and contain only lowercase ASCII letters, digits, or internal hyphens"
        );
    }
    Ok(())
}

struct RenderContext<'a> {
    product_name: &'a str,
    product_id: &'a str,
    native_id: &'a str,
    template_digest: &'a str,
}

fn materialize(directory: &Dir<'_>, destination: &Path, render: &RenderContext<'_>) -> Result<()> {
    for entry in directory.entries() {
        if !embedded_template_path(entry.path()) {
            continue;
        }
        match entry {
            DirEntry::Dir(child) => {
                let output = destination.join(child.path());
                fs::create_dir_all(&output)
                    .with_context(|| format!("create template directory '{}'", output.display()))?;
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
                        "prototype template '{}' must be UTF-8",
                        file.path().display()
                    )
                })?;
                let rendered = source
                    .replace("__YYDRA_DISTRIBUTION_VERSION__", DISTRIBUTION_VERSION)
                    .replace("__YYDRA_TEMPLATE_SHA256__", render.template_digest)
                    .replace("__PRODUCT_NAME__", render.product_name)
                    .replace("__PRODUCT_ID__", render.product_id)
                    .replace("__PRODUCT_NATIVE_ID__", render.native_id);
                fs::write(&output, rendered)
                    .with_context(|| format!("write template file '{}'", output.display()))?;
            }
        }
    }
    Ok(())
}

fn template_digest() -> String {
    let mut files = Vec::new();
    collect_files(&TEMPLATE, &mut files);
    files.sort_by_key(|file| file.path());
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path().as_os_str().as_encoded_bytes());
        hasher.update([0]);
        hasher.update(file.contents());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

fn collect_files<'a>(directory: &'a Dir<'a>, files: &mut Vec<&'a File<'a>>) {
    for entry in directory.entries() {
        if !embedded_template_path(entry.path()) {
            continue;
        }
        match entry {
            DirEntry::Dir(child) => collect_files(child, files),
            DirEntry::File(file) => files.push(file),
        }
    }
}

fn embedded_template_path(path: &Path) -> bool {
    !path.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some("target" | "node_modules" | ".expo" | "dist" | "test-results" | "android" | "ios")
        )
    })
}

fn doctor(workspace: &Path) -> Result<()> {
    let root = find_workspace_root(workspace)?;
    let recorded_version = origin_distribution(&root)?;

    println!("workspace={}", root.display());
    println!("origin_distribution={recorded_version}");
    println!("cli_distribution={DISTRIBUTION_VERSION}");
    if recorded_version != DISTRIBUTION_VERSION {
        bail!(
            "distribution mismatch; install exactly with: cargo install yydra-cli --version {recorded_version} --locked"
        );
    }
    println!("status=pass");
    Ok(())
}

fn origin_distribution(root: &Path) -> Result<String> {
    origin_field(root, "distribution_version")
}

fn origin_template_sha256(root: &Path) -> Result<String> {
    origin_field(root, "template_sha256")
}

fn origin_field(root: &Path, field: &str) -> Result<String> {
    let origin_path = root.join(".yydra/origin.toml");
    let origin = fs::read_to_string(&origin_path)
        .with_context(|| format!("read Workspace Origin Record '{}'", origin_path.display()))?;
    let recorded_version = origin
        .lines()
        .find_map(|line| {
            line.strip_prefix(&format!("{field} = \""))?
                .strip_suffix('"')
        })
        .with_context(|| format!("Workspace Origin Record has no {field}"))?;
    Ok(recorded_version.to_owned())
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

pub(crate) fn check_baseline_skills(root: &Path) -> Result<()> {
    let skills = root.join(".agents/skills");
    let actual_files = tree_files(&skills)?
        .into_iter()
        .map(|path| {
            path.strip_prefix(&skills)
                .map(Path::to_path_buf)
                .map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()?;
    let expected_files = BASELINE_SKILL_FILES
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if actual_files != expected_files {
        bail!("Baseline Skill path inventory differs from the exact Distribution snapshot");
    }

    for relative in expected_files {
        let template_path = Path::new(".agents/skills").join(&relative);
        let template_file = TEMPLATE
            .get_file(&template_path)
            .with_context(|| format!("Distribution is missing '{}'", template_path.display()))?;
        let source = std::str::from_utf8(template_file.contents())
            .with_context(|| format!("Baseline Skill '{}' is not UTF-8", relative.display()))?;
        let expected = source.replace("__YYDRA_DISTRIBUTION_VERSION__", DISTRIBUTION_VERSION);
        let actual_path = skills.join(&relative);
        let actual = fs::read_to_string(&actual_path)
            .with_context(|| format!("read Baseline Skill '{}'", actual_path.display()))?;
        if actual != expected {
            bail!(
                "{} differs from the exact Distribution snapshot",
                actual_path.display()
            );
        }
    }
    Ok(())
}

fn run(root: &Path, program: &str, arguments: &[&str], environment: &[(&str, &str)]) -> Result<()> {
    let status = ProcessCommand::new(program)
        .args(arguments)
        .envs(environment.iter().copied())
        .current_dir(root)
        .status()
        .with_context(|| format!("start {program} {}", arguments.join(" ")))?;
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

pub(crate) fn check_generated_ownership(root: &Path) -> Result<()> {
    let source = root.join("frontend/src");
    let generated = source.join("generated/public-api");
    for file in tree_files(&generated)? {
        let contents = fs::read_to_string(&file)
            .with_context(|| format!("read generated source '{}'", file.display()))?;
        if !contents.contains("Generated by orval") {
            bail!(
                "{} is inside Generated Client ownership but lacks the Orval marker",
                file.display()
            );
        }
    }
    for file in tree_files(&source)? {
        let extension = file.extension();
        if file.starts_with(&generated)
            || (extension != Some(OsStr::new("ts")) && extension != Some(OsStr::new("tsx")))
        {
            continue;
        }
        let relative = file.strip_prefix(&source)?;
        let contents = fs::read_to_string(&file)?;
        if contents.contains("generated/public-api")
            && !relative.starts_with(Path::new("framework/api"))
        {
            bail!(
                "{} imports the Generated Client outside frontend/src/framework/api",
                file.display()
            );
        }
        if relative.starts_with(Path::new("product"))
            && relative
                .components()
                .any(|part| part.as_os_str() == "domain")
            && contents.contains("expo-router")
        {
            bail!("{} imports navigation from Product Domain", file.display());
        }
    }
    Ok(())
}

pub(crate) fn compare_files(expected: &Path, actual: &Path) -> Result<()> {
    if fs::read(expected)? != fs::read(actual)? {
        bail!("{} differs from {}", expected.display(), actual.display());
    }
    Ok(())
}

pub(crate) fn compare_trees(expected: &Path, actual: &Path) -> Result<()> {
    let expected_files = relative_inventory(expected)?;
    let actual_files = relative_inventory(actual)?;
    if expected_files != actual_files {
        bail!("generated path or byte inventory differs");
    }
    Ok(())
}

fn relative_inventory(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut inventory = tree_files(root)?
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(root)?.to_path_buf();
            Ok((relative, fs::read(path)?))
        })
        .collect::<Result<Vec<_>>>()?;
    inventory.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(inventory)
}

pub(crate) fn tree_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_tree_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_tree_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(root).with_context(|| format!("read directory '{}'", root.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            collect_tree_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
