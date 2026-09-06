// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Component as PathComponent;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use flate2::read::DeflateDecoder;

use crate::DISTRIBUTION_VERSION;

const CLI_GRAPH: &[u8] = include_bytes!("../supply-chain/cli-graph.json");
const BOLTS_COMMIT: &str = "5465bcc3bbea3350dbb2affb4511a5726efb321e";
const BOLTS_PURL: &str =
    "pkg:github/boltsframework/bolts-android@5465bcc3bbea3350dbb2affb4511a5726efb321e#bolts-tasks";
const BOLTS_MODULE: &str = "frontend/modules/yydra-bolts-tasks";
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RELEASE_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_RELEASE_TREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_REQUIRED_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RELEASE_TREE_FILES: usize = 100_000;
const MAX_GRADLE_COMPONENTS: usize = 10_000;
const MAX_GRADLE_DEPENDENCY_EDGES: usize = 100_000;
const RAW_GRADLE_MATERIAL_SCHEMA_VERSION: u64 = 3;
const GRADLE_MATERIAL_SCHEMA_VERSION: u64 = 4;
const GRADLE_MATERIAL_AUTHORITY: &str = "Gradle ResolutionResult dependency edges, selected variants and actual artifact identities; dependency source trust and upstream provenance not evaluated";
const COVERAGE: &[&str] = &[
    "versions",
    "features",
    "transitives",
    "dependency-kinds",
    "targets",
    "build-tool-exposure",
];
const NOT_EVALUATED: &[&str] = &[
    "license-review",
    "dependency-source-trust",
    "upstream-provenance",
    "notice-completeness",
];

#[derive(Debug)]
pub(crate) struct SupplyChainFailure {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl SupplyChainFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

type SupplyResult<T> = std::result::Result<T, SupplyChainFailure>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SupplyChainPolicy {
    schema_version: u64,
    distribution_version: String,
    advisory_service: AdvisoryService,
    #[serde(rename = "allowedSources", default)]
    _legacy_allowed_sources: serde::de::IgnoredAny,
    #[serde(rename = "allowedLicenses", default)]
    _legacy_allowed_licenses: serde::de::IgnoredAny,
    #[serde(rename = "reviewRequiredLicensePrefixes", default)]
    _legacy_review_required_license_prefixes: serde::de::IgnoredAny,
    #[serde(rename = "prohibitedLicensePrefixes", default)]
    _legacy_prohibited_license_prefixes: serde::de::IgnoredAny,
    #[serde(rename = "prohibitedTerms", default)]
    _legacy_prohibited_terms: serde::de::IgnoredAny,
    #[serde(rename = "requiredNotices", default)]
    _legacy_required_notices: serde::de::IgnoredAny,
    report_boundary: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdvisoryService {
    name: String,
    endpoint: String,
    response_schema: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SupplyChainExceptions {
    schema_version: u64,
    distribution_version: String,
    #[serde(rename = "licenseReviews", default)]
    _legacy_license_reviews: serde::de::IgnoredAny,
    vulnerability_exceptions: Vec<VulnerabilityException>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VulnerabilityException {
    pub(crate) advisory_id: String,
    pub(crate) purl: String,
    pub(crate) version: String,
    pub(crate) target: String,
    pub(crate) impact_analysis: String,
    pub(crate) owner: String,
    pub(crate) evidence: String,
    pub(crate) approved_by: String,
    pub(crate) approved_at: String,
    pub(crate) expires_at: String,
    pub(crate) re_review_trigger: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct NoticeEvidence {
    path: String,
    sha256: String,
    #[serde(default, skip_serializing)]
    text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Component {
    bom_ref: String,
    ecosystem: String,
    name: String,
    version: String,
    features: Vec<String>,
    dependency_kinds: Vec<String>,
    targets: Vec<String>,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    declared_license: Option<String>,
    detected_license: String,
    notices: Vec<NoticeEvidence>,
    exposure: String,
    install_paths: Vec<String>,
    provenance: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    metadata_observations: Vec<ComponentMetadataObservation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComponentMetadataObservation {
    declared_license: Option<String>,
    detected_license: String,
    install_paths: Vec<String>,
    targets: Vec<String>,
    provenance: BTreeMap<String, String>,
}

impl Component {
    fn metadata_observations(&self) -> Vec<ComponentMetadataObservation> {
        if self.metadata_observations.is_empty() {
            vec![ComponentMetadataObservation {
                declared_license: self.declared_license.clone(),
                detected_license: self.detected_license.clone(),
                install_paths: self.install_paths.clone(),
                targets: self.targets.clone(),
                provenance: self.provenance.clone(),
            }]
        } else {
            self.metadata_observations.clone()
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyEdge {
    from: String,
    to: String,
    kinds: Vec<String>,
    targets: Vec<String>,
    conditions: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyInventory<'a> {
    schema_version: u64,
    distribution_version: &'static str,
    status: &'static str,
    coverage: &'static [&'static str],
    not_evaluated: &'static [&'static str],
    advisory_authority: AdvisoryAuthority<'a>,
    policy_sha256: String,
    exceptions_sha256: String,
    report_boundary: &'a str,
    components: &'a [Component],
    target_components: &'a [Component],
    dependencies: &'a [DependencyEdge],
    license_reviews: &'a [serde_json::Value],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdvisoryAuthority<'a> {
    name: &'a str,
    endpoint: &'a str,
    response_schema: &'a str,
}

#[derive(Debug)]
pub(crate) struct AdvisoryInvocation {
    pub(crate) script: PathBuf,
    pub(crate) request: PathBuf,
    pub(crate) response: PathBuf,
    pub(crate) endpoint: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdvisoryQueryMap {
    schema_version: u64,
    queries: Vec<AdvisoryQueryComponent>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdvisoryQueryComponent {
    purl: String,
    ecosystem: String,
    name: String,
    version: String,
    targets: Vec<String>,
    exposure: String,
    #[serde(default)]
    identity_basis: String,
    #[serde(default)]
    local_source_identity: String,
}

#[derive(Debug, Deserialize)]
struct OsvBatchResponse {
    results: Vec<OsvResult>,
}

#[derive(Debug, Deserialize)]
struct OsvResult {
    #[serde(default)]
    vulns: Vec<OsvVulnerability>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OsvVulnerability {
    id: String,
    modified: String,
    #[serde(default)]
    severity: Vec<serde_json::Value>,
    #[serde(default)]
    affected: Vec<OsvAffected>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct OsvAffected {
    #[serde(default)]
    database_specific: OsvAffectedDatabaseSpecific,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct OsvAffectedDatabaseSpecific {
    #[serde(default)]
    informational: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    resolve: CargoResolve,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    version: String,
    source: Option<String>,
    license: Option<String>,
    license_file: Option<String>,
    manifest_path: String,
    targets: Vec<CargoTarget>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoResolve {
    nodes: Vec<CargoNode>,
}

#[derive(Debug, Deserialize)]
struct CargoNode {
    id: String,
    features: Vec<String>,
    deps: Vec<CargoDependency>,
}

#[derive(Debug, Deserialize)]
struct CargoDependency {
    pkg: String,
    dep_kinds: Vec<CargoDependencyKind>,
}

#[derive(Debug, Deserialize)]
struct CargoDependencyKind {
    kind: Option<String>,
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CliGraph {
    schema_version: u64,
    generated_by: String,
    target: String,
    root: String,
    packages: Vec<CliPackage>,
    dependencies: Vec<CliDependencySet>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CliPackage {
    id: String,
    purl: String,
    name: String,
    version: String,
    source: String,
    checksum: Option<String>,
    declared_license: Option<String>,
    license_file: Option<String>,
    manifest_path: String,
    features: Vec<String>,
    target_kinds: Vec<String>,
    notices: Vec<NoticeEvidence>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CliDependencySet {
    from: String,
    to: Vec<CliDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CliDependency {
    purl: String,
    kinds: Vec<CliDependencyKind>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CliDependencyKind {
    kind: String,
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmLock {
    packages: BTreeMap<String, NpmPackage>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NpmPackage {
    name: Option<String>,
    version: Option<String>,
    resolved: Option<String>,
    integrity: Option<String>,
    license: Option<serde_json::Value>,
    #[serde(default)]
    dev: bool,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default)]
    dev_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    optional_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    peer_dependencies: BTreeMap<String, String>,
}

pub(crate) fn dependency_evidence(
    root: &Path,
    evidence_root: &Path,
    cargo_metadata: &[u8],
) -> SupplyResult<()> {
    let policy_path = root.join(".yydra/supply-chain-policy.json");
    let exceptions_path = root.join(".yydra/supply-chain-exceptions.json");
    let policy_bytes = read_required(&policy_path, "SUPPLY_CHAIN_POLICY_INVALID")?;
    let exception_bytes = read_required(&exceptions_path, "SUPPLY_CHAIN_EXCEPTION_INVALID")?;
    let policy: SupplyChainPolicy = serde_json::from_slice(&policy_bytes).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_POLICY_INVALID",
            format!("parse '{}': {error}", policy_path.display()),
        )
    })?;
    let exceptions: SupplyChainExceptions =
        serde_json::from_slice(&exception_bytes).map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_EXCEPTION_INVALID",
                format!("parse '{}': {error}", exceptions_path.display()),
            )
        })?;
    validate_policy(&policy, &exceptions)?;

    let metadata: CargoMetadata = serde_json::from_slice(cargo_metadata).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_CARGO_METADATA_INVALID",
            format!("cargo metadata did not return schema-compatible JSON: {error}"),
        )
    })?;
    let mut components = Vec::new();
    let mut dependencies = Vec::new();
    collect_cli_components(&policy, &mut components, &mut dependencies)?;
    collect_cargo_components(root, &metadata, &policy, &mut components, &mut dependencies)?;
    collect_npm_components(root, &policy, &mut components, &mut dependencies)?;
    let bolts = declared_bolts_component();
    let mut cli_bolts = bolts.clone();
    cli_bolts.targets = vec!["cli".to_owned()];
    cli_bolts.exposure = "shipped-linked".to_owned(); // Source bytes copied into the embedded template.
    dependencies.push(DependencyEdge {
        from: format!("pkg:cargo/yydra-cli@{DISTRIBUTION_VERSION}"),
        to: bolts.bom_ref.clone(),
        kinds: vec!["embedded-template-source".to_owned()],
        targets: vec!["cli".to_owned()],
        conditions: Vec::new(),
    });
    components.push(bolts);
    components.push(cli_bolts);
    let mut target_components = components.clone();
    merge_target_components(&mut target_components)?;
    components.sort_by(|left, right| left.bom_ref.cmp(&right.bom_ref));
    merge_components(&mut components)?;
    dependencies.sort_by(|left, right| {
        (
            &left.from,
            &left.to,
            &left.kinds,
            &left.targets,
            &left.conditions,
        )
            .cmp(&(
                &right.from,
                &right.to,
                &right.kinds,
                &right.targets,
                &right.conditions,
            ))
    });
    dependencies.dedup_by(|left, right| {
        left.from == right.from
            && left.to == right.to
            && left.kinds == right.kinds
            && left.targets == right.targets
            && left.conditions == right.conditions
    });

    let artifact_root = evidence_root.join("artifacts/supply-chain.dependencies");
    create_private_dir_all(&artifact_root).map_err(evidence_failure)?;
    let inventory = DependencyInventory {
        schema_version: 2,
        distribution_version: DISTRIBUTION_VERSION,
        status: "pass",
        coverage: COVERAGE,
        not_evaluated: NOT_EVALUATED,
        advisory_authority: AdvisoryAuthority {
            name: &policy.advisory_service.name,
            endpoint: &policy.advisory_service.endpoint,
            response_schema: &policy.advisory_service.response_schema,
        },
        policy_sha256: sha256_identity(&policy_bytes),
        exceptions_sha256: sha256_identity(&exception_bytes),
        report_boundary: &policy.report_boundary,
        components: &components,
        target_components: &target_components,
        dependencies: &dependencies,
        license_reviews: &[],
    };
    write_json(&artifact_root.join("inventory.json"), &inventory)?;
    for target in ["cli", "server", "h5", "android"] {
        write_cyclonedx(
            &artifact_root.join(format!("{target}.cdx.json")),
            target,
            &target_components,
            &dependencies,
        )?;
        write_notices(
            &artifact_root.join(format!("{target}.THIRD-PARTY-NOTICES.txt")),
            &target_components
                .iter()
                .filter(|component| {
                    component
                        .targets
                        .iter()
                        .any(|candidate| candidate == target)
                        && component.exposure != "build-tool-executed"
                })
                .cloned()
                .collect::<Vec<_>>(),
            &policy.report_boundary,
        )?;
    }
    write_notices(
        &artifact_root.join("THIRD-PARTY-NOTICES.txt"),
        &components,
        &policy.report_boundary,
    )?;
    Ok(())
}

// The declared commit is an OSV query identity, not an attestation about local
// source bytes. License and upstream source review are outside the current scope.
fn declared_bolts_component() -> Component {
    Component {
        bom_ref: BOLTS_PURL.to_owned(),
        ecosystem: "github".to_owned(),
        name: "BoltsFramework/Bolts-Android/bolts-tasks".to_owned(),
        version: BOLTS_COMMIT.to_owned(),
        features: Vec::new(),
        dependency_kinds: vec!["source-copy".to_owned()],
        targets: vec!["android".to_owned()],
        source: format!("git+https://github.com/BoltsFramework/Bolts-Android#{BOLTS_COMMIT}"),
        source_checksum: None,
        declared_license: Some("MIT".to_owned()),
        detected_license: "not-evaluated".to_owned(),
        notices: Vec::new(),
        exposure: "shipped-candidate".to_owned(),
        install_paths: vec![BOLTS_MODULE.to_owned()],
        metadata_observations: Vec::new(),
        provenance: BTreeMap::from([
            (
                "identityBasis".to_owned(),
                "distribution-declaration".to_owned(),
            ),
            ("localSourceIdentity".to_owned(), "not-evaluated".to_owned()),
            ("declaredSourceCommit".to_owned(), BOLTS_COMMIT.to_owned()),
            (
                "replaces".to_owned(),
                "pkg:maven/com.parse.bolts/bolts-tasks@1.4.0".to_owned(),
            ),
        ]),
    }
}

pub(crate) fn validate_policy_authorities(root: &Path) -> SupplyResult<()> {
    let policy_path = root.join(".yydra/supply-chain-policy.json");
    let exceptions_path = root.join(".yydra/supply-chain-exceptions.json");
    let policy: SupplyChainPolicy =
        serde_json::from_slice(&read_required(&policy_path, "SUPPLY_CHAIN_POLICY_INVALID")?)
            .map_err(|error| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_POLICY_INVALID",
                    format!("parse '{}': {error}", policy_path.display()),
                )
            })?;
    let exceptions: SupplyChainExceptions = serde_json::from_slice(&read_required(
        &exceptions_path,
        "SUPPLY_CHAIN_EXCEPTION_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_EXCEPTION_INVALID",
            format!("parse '{}': {error}", exceptions_path.display()),
        )
    })?;
    validate_policy(&policy, &exceptions)
}

pub(crate) fn prepare_advisory_query(
    root: &Path,
    evidence_root: &Path,
) -> SupplyResult<AdvisoryInvocation> {
    let policy_path = root.join(".yydra/supply-chain-policy.json");
    let exceptions_path = root.join(".yydra/supply-chain-exceptions.json");
    let policy: SupplyChainPolicy =
        serde_json::from_slice(&read_required(&policy_path, "SUPPLY_CHAIN_POLICY_INVALID")?)
            .map_err(|error| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_POLICY_INVALID",
                    format!("parse '{}': {error}", policy_path.display()),
                )
            })?;
    let exceptions: SupplyChainExceptions = serde_json::from_slice(&read_required(
        &exceptions_path,
        "SUPPLY_CHAIN_EXCEPTION_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_EXCEPTION_INVALID",
            format!("parse '{}': {error}", exceptions_path.display()),
        )
    })?;
    validate_policy(&policy, &exceptions)?;

    let inventory_path = evidence_root.join("artifacts/supply-chain.dependencies/inventory.json");
    let inventory: serde_json::Value = serde_json::from_slice(&read_required(
        &inventory_path,
        "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            format!("parse '{}': {error}", inventory_path.display()),
        )
    })?;
    if inventory
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
        || inventory
            .get("distributionVersion")
            .and_then(serde_json::Value::as_str)
            != Some(DISTRIBUTION_VERSION)
        || inventory.get("status").and_then(serde_json::Value::as_str) != Some("pass")
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            "dependency inventory is not a passing schema-2 artifact for this exact Distribution",
        ));
    }
    let components = inventory
        .get("components")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
                "dependency inventory has no component array",
            )
        })?;
    let mut queries = Vec::new();
    for component in components {
        let source = required_json_string(component, "source")?;
        if source == "workspace-path" {
            continue;
        }
        let ecosystem = required_json_string(component, "ecosystem")?;
        let osv_ecosystem = match ecosystem {
            "cargo" => "crates.io",
            "npm" => "npm",
            "github"
                if required_json_string(component, "bomRef")? == BOLTS_PURL
                    && required_json_string(component, "version")? == BOLTS_COMMIT =>
            {
                "GIT"
            }
            other => {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
                    format!("unsupported advisory ecosystem {other:?}"),
                ));
            }
        };
        let targets = component
            .get("targets")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
                    "component has no target array",
                )
            })?
            .iter()
            .map(|target| {
                target.as_str().map(str::to_owned).ok_or_else(|| {
                    SupplyChainFailure::new(
                        "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
                        "component target is not a string",
                    )
                })
            })
            .collect::<SupplyResult<Vec<_>>>()?;
        if targets.is_empty() {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
                "external component has no applicable target",
            ));
        }
        queries.push(AdvisoryQueryComponent {
            purl: required_json_string(component, "bomRef")?.to_owned(),
            identity_basis: if ecosystem == "github" {
                "distribution-declaration"
            } else {
                "resolved-package-metadata"
            }
            .to_owned(),
            local_source_identity: "not-evaluated".to_owned(),
            ecosystem: osv_ecosystem.to_owned(),
            name: required_json_string(component, "name")?.to_owned(),
            version: required_json_string(component, "version")?.to_owned(),
            targets: sorted_unique(targets),
            exposure: required_json_string(component, "exposure")?.to_owned(),
        });
    }
    queries.sort_by(|left, right| left.purl.cmp(&right.purl));
    if queries.is_empty() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            "dependency inventory has no external components to query",
        ));
    }
    let mut unique = BTreeSet::new();
    if queries
        .iter()
        .any(|query| !unique.insert(query.purl.clone()))
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            "dependency inventory contains duplicate advisory identities",
        ));
    }

    write_advisory_invocation(
        evidence_root,
        "supply-chain.advisories",
        &policy.advisory_service,
        queries,
    )
}

pub(crate) fn prepare_android_advisory_query(
    root: &Path,
    evidence_root: &Path,
) -> SupplyResult<AdvisoryInvocation> {
    let policy_path = root.join(".yydra/supply-chain-policy.json");
    let exceptions_path = root.join(".yydra/supply-chain-exceptions.json");
    let policy: SupplyChainPolicy =
        serde_json::from_slice(&read_required(&policy_path, "SUPPLY_CHAIN_POLICY_INVALID")?)
            .map_err(|error| {
                SupplyChainFailure::new("SUPPLY_CHAIN_POLICY_INVALID", error.to_string())
            })?;
    let exceptions: SupplyChainExceptions = serde_json::from_slice(&read_required(
        &exceptions_path,
        "SUPPLY_CHAIN_EXCEPTION_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new("SUPPLY_CHAIN_EXCEPTION_INVALID", error.to_string())
    })?;
    validate_policy(&policy, &exceptions)?;

    let inventory_path = evidence_root.join("artifacts/android.release/gradle-materials.json");
    let inventory: AndroidGradleMaterialInventory = serde_json::from_slice(&read_required(
        &inventory_path,
        "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            format!("parse '{}': {error}", inventory_path.display()),
        )
    })?;
    if inventory.schema_version != GRADLE_MATERIAL_SCHEMA_VERSION
        || inventory.distribution_version != DISTRIBUTION_VERSION
        || inventory.configuration != "releaseRuntimeClasspath"
        || inventory.source_authority != GRADLE_MATERIAL_AUTHORITY
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            "Android advisory input is not the exact schema-4 releaseRuntimeClasspath material inventory for this Distribution",
        ));
    }
    let advisory_external = inventory
        .components
        .iter()
        .cloned()
        .map(|component| (component.purl.clone(), component))
        .collect::<BTreeMap<_, _>>();
    let advisory_local = inventory
        .local_components
        .iter()
        .cloned()
        .map(|component| (component.project_path.clone(), component))
        .collect::<BTreeMap<_, _>>();
    if advisory_external.len() != inventory.components.len()
        || advisory_local.len() != inventory.local_components.len()
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            "Android advisory input repeats a Gradle component identity",
        ));
    }
    validate_gradle_dependency_edges(&inventory.dependencies, &advisory_external, &advisory_local)?;
    let mut queries = Vec::new();
    for component in inventory.components {
        if component.exposure
            != (if component.artifacts.is_empty() {
                "dependency-graph-only"
            } else {
                "runtime-build-input"
            })
            || component.purl
                != format!(
                    "pkg:maven/{}/{}@{}",
                    component.group, component.name, component.version
                )
            || component.group.is_empty()
            || component.name.is_empty()
            || component.version.is_empty()
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
                format!(
                    "{} is not an exact resolved Maven runtime-classpath component",
                    component.purl
                ),
            ));
        }
        if component.artifacts.is_empty() {
            continue;
        }
        queries.push(AdvisoryQueryComponent {
            purl: component.purl,
            identity_basis: "gradle-resolution".to_owned(),
            local_source_identity: "not-evaluated".to_owned(),
            ecosystem: "Maven".to_owned(),
            name: format!("{}:{}", component.group, component.name),
            version: component.version,
            targets: vec!["android".to_owned()],
            exposure: component.exposure,
        });
    }
    queries.sort_by(|left, right| left.purl.cmp(&right.purl));
    if queries.is_empty() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            "Android release material inventory has no selected Maven build artifacts to query",
        ));
    }
    let mut unique = BTreeSet::new();
    if queries
        .iter()
        .any(|query| !unique.insert(query.purl.clone()))
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
            "Android release material inventory contains duplicate advisory identities",
        ));
    }
    write_advisory_invocation(
        evidence_root,
        "supply-chain.android-advisories",
        &policy.advisory_service,
        queries,
    )
}

fn write_advisory_invocation(
    evidence_root: &Path,
    artifact_name: &str,
    advisory_service: &AdvisoryService,
    queries: Vec<AdvisoryQueryComponent>,
) -> SupplyResult<AdvisoryInvocation> {
    let artifact_root = evidence_root.join("artifacts").join(artifact_name);
    create_private_dir_all(&artifact_root).map_err(evidence_failure)?;
    let request_path = artifact_root.join("request.json");
    let request = json!({
        "queries": queries.iter().map(|query| if query.ecosystem == "GIT" {
            json!({"commit": query.version})
        } else { json!({
            "package": {
                "ecosystem": query.ecosystem,
                "name": query.name,
            },
            "version": query.version,
        }) }).collect::<Vec<_>>()
    });
    write_json(&request_path, &request)?;
    write_json(
        &artifact_root.join("query-map.json"),
        &AdvisoryQueryMap {
            schema_version: 1,
            queries,
        },
    )?;
    let script_path = artifact_root.join("osv-query.mjs");
    write_bytes(
        &script_path,
        br#"import { readFileSync, writeFileSync } from "node:fs";

const [requestPath, responsePath, endpoint] = process.argv.slice(2);
const request = JSON.parse(readFileSync(requestPath, "utf8"));
const response = await fetch(endpoint, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(request),
  signal: AbortSignal.timeout(30_000),
});
if (!response.ok) {
  console.error(`OSV querybatch returned HTTP ${response.status}`);
  process.exit(3);
}
const text = await response.text();
const batch = JSON.parse(text);
writeFileSync(`${responsePath}.batch`, text.endsWith("\n") ? text : `${text}\n`);
if (!Array.isArray(batch.results) || batch.results.length !== request.queries.length) {
  console.error("OSV querybatch returned a malformed result count");
  process.exit(4);
}
for (const result of batch.results) {
  if (!Array.isArray(result.vulns)) continue;
  const full = [];
  for (const compact of result.vulns) {
    const detailUrl = new URL(endpoint);
    detailUrl.pathname = `/v1/vulns/${encodeURIComponent(compact.id)}`;
    detailUrl.search = "";
    const detailResponse = await fetch(detailUrl, {
      method: "GET",
      signal: AbortSignal.timeout(30_000),
    });
    if (!detailResponse.ok) {
      console.error(`OSV vulnerability detail ${compact.id} returned HTTP ${detailResponse.status}`);
      process.exit(5);
    }
    const detail = await detailResponse.json();
    if (detail.id !== compact.id || detail.modified !== compact.modified) {
      console.error(`OSV vulnerability detail ${compact.id} changed during this evidence invocation`);
      process.exit(6);
    }
    full.push(detail);
  }
  result.vulns = full;
}
writeFileSync(responsePath, `${JSON.stringify(batch)}\n`);
"#,
    )?;
    Ok(AdvisoryInvocation {
        script: script_path,
        request: request_path,
        response: artifact_root.join("response.json"),
        endpoint: advisory_service.endpoint.clone(),
    })
}

pub(crate) fn advisory_evidence(root: &Path, evidence_root: &Path) -> SupplyResult<()> {
    advisory_evidence_for(
        root,
        evidence_root,
        "supply-chain.advisories",
        &["pkg:cargo/", "pkg:npm/", BOLTS_PURL],
    )
}

pub(crate) fn android_advisory_evidence(root: &Path, evidence_root: &Path) -> SupplyResult<()> {
    advisory_evidence_for(
        root,
        evidence_root,
        "supply-chain.android-advisories",
        &["pkg:maven/"],
    )
}

fn advisory_evidence_for(
    root: &Path,
    evidence_root: &Path,
    artifact_name: &str,
    scope_prefixes: &[&str],
) -> SupplyResult<()> {
    let artifact_root = evidence_root.join("artifacts").join(artifact_name);
    let query_map_path = artifact_root.join("query-map.json");
    let response_path = artifact_root.join("response.json");
    let query_map: AdvisoryQueryMap = serde_json::from_slice(&read_required(
        &query_map_path,
        "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
            format!("parse '{}': {error}", query_map_path.display()),
        )
    })?;
    if query_map.schema_version != 1 || query_map.queries.is_empty() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
            "advisory query map is empty or uses an unsupported schema",
        ));
    }
    if query_map.queries.iter().any(|query| {
        !scope_prefixes.iter().any(|prefix| {
            if *prefix == BOLTS_PURL {
                query.purl == BOLTS_PURL
                    && query.ecosystem == "GIT"
                    && query.version == BOLTS_COMMIT
            } else {
                query.purl.starts_with(prefix)
            }
        })
    }) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
            format!("{artifact_name} query map contains an out-of-scope package identity"),
        ));
    }
    let response_bytes = read_required(&response_path, "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID")?;
    let batch_response_path = artifact_root.join("response.json.batch");
    let batch_response_sha256 = batch_response_path
        .is_file()
        .then(|| {
            read_required(
                &batch_response_path,
                "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
            )
            .map(|bytes| sha256_identity(&bytes))
        })
        .transpose()?;
    let response: OsvBatchResponse = serde_json::from_slice(&response_bytes).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
            format!("parse OSV querybatch response: {error}"),
        )
    })?;
    if response.results.len() != query_map.queries.len() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
            format!(
                "OSV querybatch returned {} results for {} exact queries",
                response.results.len(),
                query_map.queries.len()
            ),
        ));
    }
    if response.results.iter().any(|result| {
        result
            .next_page_token
            .as_deref()
            .is_some_and(|token| !token.is_empty())
    }) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ADVISORY_RESPONSE_INCOMPLETE",
            "OSV querybatch response is paginated; this Distribution refuses to report partial advisory coverage",
        ));
    }

    let policy_path = root.join(".yydra/supply-chain-policy.json");
    let exceptions_path = root.join(".yydra/supply-chain-exceptions.json");
    let policy: SupplyChainPolicy =
        serde_json::from_slice(&read_required(&policy_path, "SUPPLY_CHAIN_POLICY_INVALID")?)
            .map_err(|error| {
                SupplyChainFailure::new("SUPPLY_CHAIN_POLICY_INVALID", error.to_string())
            })?;
    let exceptions: SupplyChainExceptions = serde_json::from_slice(&read_required(
        &exceptions_path,
        "SUPPLY_CHAIN_EXCEPTION_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new("SUPPLY_CHAIN_EXCEPTION_INVALID", error.to_string())
    })?;
    validate_policy(&policy, &exceptions)?;

    let mut findings = Vec::new();
    let mut unexcepted = Vec::new();
    let mut used_exceptions = BTreeSet::new();
    let mut vulnerability_count = 0_u64;
    let mut informational_count = 0_u64;
    for (query, result) in query_map.queries.iter().zip(response.results) {
        let mut seen_advisories = BTreeSet::new();
        for vulnerability in result.vulns {
            if vulnerability.id.is_empty()
                || vulnerability.modified.is_empty()
                || !seen_advisories.insert(vulnerability.id.clone())
            {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ADVISORY_RESPONSE_INVALID",
                    format!(
                        "OSV returned an invalid or duplicate advisory for {}",
                        query.purl
                    ),
                ));
            }
            let informational = vulnerability
                .affected
                .iter()
                .filter_map(|affected| affected.database_specific.informational.as_deref())
                .map(str::trim)
                .collect::<Vec<_>>();
            if vulnerability.severity.is_empty()
                && !informational.is_empty()
                && informational.len() == vulnerability.affected.len()
                && informational
                    .iter()
                    .all(|classification| !classification.is_empty())
            {
                informational_count = informational_count.saturating_add(1);
                findings.push(json!({
                    "advisoryId": vulnerability.id,
                    "modified": vulnerability.modified,
                    "purl": query.purl,
                    "version": query.version,
                    "targets": query.targets,
                    "exposure": query.exposure,
                    "classification": "informational",
                    "informationalKinds": informational,
                    "exceptedTargets": [],
                    "unexceptedTargets": [],
                }));
                continue;
            }
            vulnerability_count = vulnerability_count.saturating_add(1);
            let mut missing_targets = Vec::new();
            let mut applied = Vec::new();
            for target in &query.targets {
                let matched = exceptions
                    .vulnerability_exceptions
                    .iter()
                    .find(|exception| {
                        exception.advisory_id == vulnerability.id
                            && exception.purl == query.purl
                            && exception.version == query.version
                            && exception.target == *target
                    });
                if let Some(exception) = matched {
                    used_exceptions.insert((
                        exception.advisory_id.clone(),
                        exception.purl.clone(),
                        exception.version.clone(),
                        exception.target.clone(),
                    ));
                    applied.push(target.clone());
                } else {
                    missing_targets.push(target.clone());
                }
            }
            if !missing_targets.is_empty() {
                unexcepted.push(format!(
                    "{} {} for targets {}",
                    query.purl,
                    vulnerability.id,
                    missing_targets.join(",")
                ));
            }
            findings.push(json!({
                "advisoryId": vulnerability.id,
                "modified": vulnerability.modified,
                "purl": query.purl,
                "version": query.version,
                "targets": query.targets,
                "exposure": query.exposure,
                "classification": "vulnerability",
                "exceptedTargets": applied,
                "unexceptedTargets": missing_targets,
            }));
        }
    }
    for exception in exceptions
        .vulnerability_exceptions
        .iter()
        .filter(|exception| {
            scope_prefixes
                .iter()
                .any(|prefix| exception.purl.starts_with(prefix))
        })
    {
        let key = (
            exception.advisory_id.clone(),
            exception.purl.clone(),
            exception.version.clone(),
            exception.target.clone(),
        );
        if !used_exceptions.contains(&key) {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_EXCEPTION_UNMATCHED",
                format!(
                    "vulnerability exception for {} / {} / {} matched no current exact OSV result",
                    exception.purl, exception.advisory_id, exception.target
                ),
            ));
        }
    }
    let report = json!({
        "schemaVersion": 1,
        "distributionVersion": DISTRIBUTION_VERSION,
        "status": if unexcepted.is_empty() { "pass" } else { "fail" },
        "authority": {
            "name": policy.advisory_service.name,
            "endpoint": policy.advisory_service.endpoint,
            "responseSchema": policy.advisory_service.response_schema,
        },
        "queries": query_map.queries.len(),
        "queryIdentities": query_map.queries,
        "vulnerabilities": vulnerability_count,
        "informationalAdvisories": informational_count,
        "exceptionsApplied": used_exceptions.len(),
        "batchResponseSha256": batch_response_sha256,
        "responseSha256": sha256_identity(&response_bytes),
        "findings": findings,
        "reportBoundary": "OSV results cover only the reported query identities at check time. A Distribution-declared commit is not a verified local-source identity; local modifications and upstream source authenticity are not evaluated. Informational-only records are retained but are not vulnerabilities or waivers. The report does not prove absence of vulnerabilities in unqueried material, or unknown, unpublished, malicious, or later-disclosed vulnerabilities.",
    });
    write_json(&artifact_root.join("report.json"), &report)?;
    if !unexcepted.is_empty() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_VULNERABILITY_FOUND",
            format!(
                "known applicable vulnerabilities lack exact current exceptions: {}",
                unexcepted.join("; ")
            ),
        ));
    }
    Ok(())
}

fn required_json_string<'a>(value: &'a serde_json::Value, field: &str) -> SupplyResult<&'a str> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ADVISORY_INPUT_INVALID",
                format!("dependency component has no non-empty {field:?}"),
            )
        })
}

fn validate_policy(
    policy: &SupplyChainPolicy,
    exceptions: &SupplyChainExceptions,
) -> SupplyResult<()> {
    if policy.schema_version != 1
        || exceptions.schema_version != 1
        || policy.distribution_version != DISTRIBUTION_VERSION
        || exceptions.distribution_version != DISTRIBUTION_VERSION
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_POLICY_INVALID",
            "supply-chain policy and exceptions must use schema 1 and this exact Distribution version",
        ));
    }
    if policy.advisory_service.name != "OSV"
        || policy.advisory_service.endpoint != "https://api.osv.dev/v1/querybatch"
        || policy.advisory_service.response_schema != "OSV API 1.0 querybatch"
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_POLICY_INVALID",
            "the exact Distribution advisory authority is OSV API 1.0 querybatch at https://api.osv.dev/v1/querybatch",
        ));
    }
    if !policy
        .report_boundary
        .contains("not blanket legal compatibility")
        || !policy.report_boundary.contains("absence of malicious code")
        || !policy.report_boundary.contains("notice completeness")
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_POLICY_INVALID",
            "the report must retain the legal, malicious-code, and notice-completeness claim boundary",
        ));
    }
    // Historical license reviews are not admission or expiry gates under the
    // maintainer-approved Issue #26/#39 scope. Vulnerability exceptions below
    // retain their exact identity, approval and expiry requirements.
    let mut seen_vulnerability_exceptions = BTreeSet::new();
    for exception in &exceptions.vulnerability_exceptions {
        let key = (
            exception.advisory_id.as_str(),
            exception.purl.as_str(),
            exception.version.as_str(),
            exception.target.as_str(),
        );
        if !seen_vulnerability_exceptions.insert(key)
            || !["pkg:cargo/", "pkg:npm/", "pkg:maven/"]
                .iter()
                .any(|prefix| exception.purl.starts_with(prefix))
            || exception.advisory_id.is_empty()
            || exception.purl.is_empty()
            || exception.version.is_empty()
            || !exception.purl.ends_with(&format!("@{}", exception.version))
            || !matches!(
                exception.target.as_str(),
                "cli" | "server" | "h5" | "android"
            )
            || exception.impact_analysis.is_empty()
            || exception.owner.is_empty()
            || !exception.evidence.starts_with("https://")
            || exception.approved_by.is_empty()
            || exception.approved_at.is_empty()
            || exception.expires_at.is_empty()
            || exception.re_review_trigger.is_empty()
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_EXCEPTION_OVERBROAD",
                format!(
                    "vulnerability exception for {:?} lacks an exact advisory, component version, target, analysis, owner, evidence, approval, or time bound",
                    exception.purl
                ),
            ));
        }
        let approved_at = utc_timestamp_seconds(&exception.approved_at)?;
        let expires_at = utc_timestamp_seconds(&exception.expires_at)?;
        let now = unix_seconds()?;
        if approved_at > now || approved_at >= expires_at {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_EXCEPTION_OVERBROAD",
                format!(
                    "vulnerability exception for {} / {} has a future approval or a non-forward expiry",
                    exception.purl, exception.advisory_id
                ),
            ));
        }
        if expires_at <= now {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_EXCEPTION_STALE",
                format!(
                    "vulnerability exception for {} / {} expired at {}",
                    exception.purl, exception.advisory_id, exception.expires_at
                ),
            ));
        }
    }
    Ok(())
}

fn unix_seconds() -> SupplyResult<u64> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            SupplyChainFailure::new("SUPPLY_CHAIN_CLOCK_UNAVAILABLE", error.to_string())
        })?
        .as_secs();
    Ok(seconds)
}

fn utc_timestamp_seconds(value: &str) -> SupplyResult<u64> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_EXCEPTION_OVERBROAD",
            format!("exception timestamp {value:?} must be UTC YYYY-MM-DDTHH:MM:SSZ"),
        ));
    }
    let part = |start: usize, end: usize| -> SupplyResult<i64> {
        value[start..end].parse::<i64>().map_err(|_| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_EXCEPTION_OVERBROAD",
                format!("exception timestamp {value:?} contains invalid digits"),
            )
        })
    };
    let year = part(0, 4)?;
    let month = part(5, 7)?;
    let day = part(8, 10)?;
    let hour = part(11, 13)?;
    let minute = part(14, 16)?;
    let second = part(17, 19)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_EXCEPTION_OVERBROAD",
            format!("exception timestamp {value:?} has an out-of-range field"),
        ));
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = (if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    }) / 400;
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    if days < 0 || civil_from_days(days) != value[..10] {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_EXCEPTION_OVERBROAD",
            format!("exception timestamp {value:?} is not a valid post-epoch UTC timestamp"),
        ));
    }
    let total = days
        .checked_mul(86_400)
        .and_then(|value| value.checked_add(hour * 3_600 + minute * 60 + second))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_EXCEPTION_OVERBROAD",
                format!("exception timestamp {value:?} is out of range"),
            )
        })?;
    Ok(total)
}

fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CargoArtifactExposure {
    BuildToolExecuted,
    ShippedLinked,
}

impl CargoArtifactExposure {
    fn through(self, links_runtime_artifact: bool) -> Self {
        if self == Self::ShippedLinked && links_runtime_artifact {
            Self::ShippedLinked
        } else {
            Self::BuildToolExecuted
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::BuildToolExecuted => "build-tool-executed",
            Self::ShippedLinked => "shipped-linked",
        }
    }
}

#[derive(Debug)]
struct CargoExposureEdge {
    from: String,
    to: String,
    links_runtime_artifact: bool,
}

fn cargo_target_kinds_link_runtime_artifact<'a>(kinds: impl IntoIterator<Item = &'a str>) -> bool {
    kinds
        .into_iter()
        .any(|kind| matches!(kind, "lib" | "rlib" | "dylib" | "cdylib" | "staticlib"))
}

fn cargo_artifact_exposures(
    roots: impl IntoIterator<Item = String>,
    edges: impl IntoIterator<Item = CargoExposureEdge>,
) -> BTreeMap<String, CargoArtifactExposure> {
    let mut adjacency = BTreeMap::<String, Vec<(String, bool)>>::new();
    for edge in edges {
        adjacency
            .entry(edge.from)
            .or_default()
            .push((edge.to, edge.links_runtime_artifact));
    }
    let mut exposures = BTreeMap::new();
    let mut pending = VecDeque::new();
    for root in roots {
        exposures.insert(root.clone(), CargoArtifactExposure::ShippedLinked);
        pending.push_back(root);
    }
    while let Some(from) = pending.pop_front() {
        let parent = exposures[&from];
        for (to, links_runtime_artifact) in adjacency.get(&from).into_iter().flatten() {
            let candidate = parent.through(*links_runtime_artifact);
            let should_propagate = exposures.get(to).is_none_or(|current| candidate > *current);
            if should_propagate {
                exposures.insert(to.clone(), candidate);
                pending.push_back(to.clone());
            }
        }
    }
    exposures
}

fn collect_cli_components(
    _policy: &SupplyChainPolicy,
    components: &mut Vec<Component>,
    dependencies: &mut Vec<DependencyEdge>,
) -> SupplyResult<()> {
    let graph: CliGraph = serde_json::from_slice(CLI_GRAPH).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_CLI_GRAPH_INVALID",
            format!("parse embedded CLI graph: {error}"),
        )
    })?;
    if graph.schema_version != 2
        || graph.generated_by != "scripts/generate-cli-supply-chain.mjs"
        || graph.root != format!("pkg:cargo/yydra-cli@{DISTRIBUTION_VERSION}")
        || graph.target != env!("YYDRA_BUILD_TARGET")
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_CLI_GRAPH_INVALID",
            format!(
                "embedded CLI graph does not identify this exact Distribution and build target (embedded {:?}, executable {:?})",
                graph.target,
                env!("YYDRA_BUILD_TARGET")
            ),
        ));
    }
    let incoming = cargo_incoming_from_cli(&graph.dependencies);
    let target_kinds = graph
        .packages
        .iter()
        .map(|package| (package.purl.as_str(), package.target_kinds.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let exposures = cargo_artifact_exposures(
        [graph.root.clone()],
        graph.dependencies.iter().flat_map(|set| {
            set.to.iter().map(|edge| CargoExposureEdge {
                from: set.from.clone(),
                to: edge.purl.clone(),
                links_runtime_artifact: edge.kinds.iter().any(|kind| kind.kind == "normal")
                    && target_kinds.get(edge.purl.as_str()).is_some_and(|kinds| {
                        cargo_target_kinds_link_runtime_artifact(kinds.iter().map(String::as_str))
                    }),
            })
        }),
    );
    for package in graph.packages {
        let declared = package.declared_license.clone();
        let detected = "not-evaluated".to_owned();
        let license_detection = "not-evaluated".to_owned();
        let kinds = incoming
            .get(&package.purl)
            .cloned()
            .unwrap_or_else(|| BTreeSet::from(["root".to_owned()]));
        let exposure = exposures.get(&package.purl).copied().ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_CLI_GRAPH_INVALID",
                format!(
                    "embedded CLI graph contains unreachable package {}",
                    package.purl
                ),
            )
        })?;
        components.push(Component {
            bom_ref: package.purl.clone(),
            ecosystem: "cargo".to_owned(),
            name: package.name,
            version: package.version,
            features: sorted_unique(package.features),
            dependency_kinds: kinds.into_iter().collect(),
            targets: vec!["cli".to_owned()],
            source: package.source.clone(),
            source_checksum: package
                .checksum
                .map(|checksum| format!("sha256:{checksum}")),
            declared_license: declared,
            detected_license: detected,
            notices: package.notices,
            exposure: exposure.as_str().to_owned(),
            install_paths: vec![
                package
                    .manifest_path
                    .strip_suffix("/Cargo.toml")
                    .unwrap_or(&package.manifest_path)
                    .to_owned(),
            ],
            metadata_observations: Vec::new(),
            provenance: BTreeMap::from([
                ("cli.authority".to_owned(), "embedded-cli-graph".to_owned()),
                ("cli.packageId".to_owned(), package.id),
                ("cli.manifestPath".to_owned(), package.manifest_path),
                ("cli.hostTarget".to_owned(), graph.target.clone()),
                ("cli.licenseDetection".to_owned(), license_detection),
                (
                    "cli.licenseFile".to_owned(),
                    package
                        .license_file
                        .unwrap_or_else(|| "manifest-license".to_owned()),
                ),
            ]),
        });
    }
    for set in graph.dependencies {
        for edge in set.to {
            dependencies.push(DependencyEdge {
                from: set.from.clone(),
                to: edge.purl,
                kinds: sorted_unique(edge.kinds.iter().map(|kind| kind.kind.clone()).collect()),
                targets: vec!["cli".to_owned()],
                conditions: sorted_unique(
                    edge.kinds
                        .into_iter()
                        .filter_map(|kind| kind.target)
                        .collect(),
                ),
            });
        }
    }
    Ok(())
}

fn cargo_incoming_from_cli(
    dependencies: &[CliDependencySet],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut incoming = BTreeMap::<String, BTreeSet<String>>::new();
    for set in dependencies {
        for edge in &set.to {
            let kinds = incoming.entry(edge.purl.clone()).or_default();
            for kind in &edge.kinds {
                kinds.insert(kind.kind.clone());
            }
        }
    }
    incoming
}

fn collect_cargo_components(
    root: &Path,
    metadata: &CargoMetadata,
    _policy: &SupplyChainPolicy,
    components: &mut Vec<Component>,
    dependencies: &mut Vec<DependencyEdge>,
) -> SupplyResult<()> {
    let packages = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    let checksums = cargo_lock_checksums(&root.join("Cargo.lock"))?;
    let canonical_root = root.canonicalize().map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_SOURCE_PROHIBITED",
            format!("canonicalize workspace root '{}': {error}", root.display()),
        )
    })?;
    let mut incoming = BTreeMap::<String, BTreeSet<String>>::new();
    for node in &metadata.resolve.nodes {
        for dependency in &node.deps {
            for kind in &dependency.dep_kinds {
                incoming
                    .entry(dependency.pkg.clone())
                    .or_default()
                    .insert(kind.kind.clone().unwrap_or_else(|| "normal".to_owned()));
            }
        }
    }
    let server_roots = metadata
        .packages
        .iter()
        .filter(|package| {
            package.targets.iter().any(|target| {
                target.name == "server" && target.kind.iter().any(|kind| kind == "bin")
            })
        })
        .map(|package| package.id.clone())
        .collect::<Vec<_>>();
    if server_roots.len() != 1 {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_CARGO_METADATA_INVALID",
            format!(
                "exact server dependency inventory requires one binary target named server, found {}",
                server_roots.len()
            ),
        ));
    }
    let exposures = cargo_artifact_exposures(
        server_roots,
        metadata.resolve.nodes.iter().flat_map(|node| {
            node.deps.iter().map(|dependency| CargoExposureEdge {
                from: node.id.clone(),
                to: dependency.pkg.clone(),
                links_runtime_artifact: dependency
                    .dep_kinds
                    .iter()
                    .any(|kind| kind.kind.as_deref().unwrap_or("normal") == "normal")
                    && packages
                        .get(dependency.pkg.as_str())
                        .is_some_and(|package| {
                            cargo_target_kinds_link_runtime_artifact(
                                package
                                    .targets
                                    .iter()
                                    .flat_map(|target| target.kind.iter().map(String::as_str)),
                            )
                        }),
            })
        }),
    );
    for node in &metadata.resolve.nodes {
        let Some(exposure) = exposures.get(&node.id).copied() else {
            continue;
        };
        let package = packages.get(node.id.as_str()).ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_CARGO_METADATA_INVALID",
                format!("resolved package {:?} has no metadata", node.id),
            )
        })?;
        let source = package
            .source
            .clone()
            .unwrap_or_else(|| "workspace-path".to_owned());
        if source == "workspace-path" {
            let canonical_manifest =
                Path::new(&package.manifest_path)
                    .canonicalize()
                    .map_err(|error| {
                        SupplyChainFailure::new(
                            "SUPPLY_CHAIN_SOURCE_PROHIBITED",
                            format!(
                                "canonicalize workspace Cargo manifest '{}': {error}",
                                package.manifest_path
                            ),
                        )
                    })?;
            if !canonical_manifest.starts_with(&canonical_root) {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_SOURCE_PROHIBITED",
                    format!(
                        "workspace Cargo package {} resolves outside the Product Workspace: {}",
                        package.name,
                        canonical_manifest.display()
                    ),
                ));
            }
        }
        let checksum = checksums
            .get(&(
                package.name.clone(),
                package.version.clone(),
                source.clone(),
            ))
            .cloned()
            .flatten();
        let purl = cargo_purl(&package.name, &package.version);
        validate_cargo_lock_identity(&source, checksum.as_deref(), &purl)?;
        let declared = package.license.clone();
        let notices = Vec::new();
        let detected = "not-evaluated".to_owned();
        let license_detection = "not-evaluated".to_owned();
        let kinds = incoming
            .get(&package.id)
            .cloned()
            .unwrap_or_else(|| BTreeSet::from(["root".to_owned()]));
        components.push(Component {
            bom_ref: purl,
            ecosystem: "cargo".to_owned(),
            name: package.name.clone(),
            version: package.version.clone(),
            features: sorted_unique(node.features.clone()),
            dependency_kinds: kinds.into_iter().collect(),
            targets: vec!["server".to_owned()],
            source: source.clone(),
            source_checksum: checksum.map(|value| format!("sha256:{value}")),
            declared_license: declared,
            detected_license: detected,
            notices,
            exposure: exposure.as_str().to_owned(),
            install_paths: vec![stable_cargo_install_path(root, package)],
            metadata_observations: Vec::new(),
            provenance: BTreeMap::from([
                (
                    "server.authority".to_owned(),
                    if source == "workspace-path" {
                        "workspace-manifest-and-origin"
                    } else {
                        "Cargo.lock-and-registry-manifest"
                    }
                    .to_owned(),
                ),
                ("server.packageId".to_owned(), package.id.clone()),
                (
                    "server.manifestPath".to_owned(),
                    stable_cargo_manifest_path(root, package),
                ),
                (
                    "server.licenseFile".to_owned(),
                    stable_cargo_license_file(root, package),
                ),
                ("server.licenseDetection".to_owned(), license_detection),
            ]),
        });
    }
    for node in &metadata.resolve.nodes {
        if !exposures.contains_key(&node.id) {
            continue;
        }
        let from_package = packages.get(node.id.as_str()).ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_CARGO_METADATA_INVALID",
                format!("resolved package {:?} has no metadata", node.id),
            )
        })?;
        for dependency in &node.deps {
            if !exposures.contains_key(&dependency.pkg) {
                continue;
            }
            let to_package = packages.get(dependency.pkg.as_str()).ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_CARGO_METADATA_INVALID",
                    format!("dependency package {:?} has no metadata", dependency.pkg),
                )
            })?;
            dependencies.push(DependencyEdge {
                from: cargo_purl(&from_package.name, &from_package.version),
                to: cargo_purl(&to_package.name, &to_package.version),
                kinds: sorted_unique(
                    dependency
                        .dep_kinds
                        .iter()
                        .map(|kind| kind.kind.clone().unwrap_or_else(|| "normal".to_owned()))
                        .collect(),
                ),
                targets: vec!["server".to_owned()],
                conditions: sorted_unique(
                    dependency
                        .dep_kinds
                        .iter()
                        .filter_map(|kind| kind.target.clone())
                        .collect(),
                ),
            });
        }
    }
    Ok(())
}

fn cargo_lock_checksums(
    path: &Path,
) -> SupplyResult<BTreeMap<(String, String, String), Option<String>>> {
    let bytes = read_required(path, "SUPPLY_CHAIN_PROVENANCE_MISSING")?;
    let source = std::str::from_utf8(&bytes).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISSING",
            format!("read '{}': {error}", path.display()),
        )
    })?;
    let value: toml::Value = toml::from_str(source).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISSING",
            format!("parse '{}': {error}", path.display()),
        )
    })?;
    let packages = value
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISSING",
                "Cargo.lock has no package inventory",
            )
        })?;
    let mut checksums = BTreeMap::new();
    for package in packages {
        let name = package.get("name").and_then(toml::Value::as_str);
        let version = package.get("version").and_then(toml::Value::as_str);
        if let (Some(name), Some(version)) = (name, version) {
            let source = package
                .get("source")
                .and_then(toml::Value::as_str)
                .unwrap_or("workspace-path");
            checksums.insert(
                (name.to_owned(), version.to_owned(), source.to_owned()),
                package
                    .get("checksum")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
            );
        }
    }
    Ok(checksums)
}

fn collect_npm_components(
    root: &Path,
    _policy: &SupplyChainPolicy,
    components: &mut Vec<Component>,
    dependencies: &mut Vec<DependencyEdge>,
) -> SupplyResult<()> {
    let frontend = root.join("frontend");
    let lock_path = frontend.join("package-lock.json");
    let lock_bytes = read_required(&lock_path, "SUPPLY_CHAIN_PROVENANCE_MISSING")?;
    let lock: NpmLock = serde_json::from_slice(&lock_bytes).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISSING",
            format!("parse '{}': {error}", lock_path.display()),
        )
    })?;
    let root_manifest: serde_json::Value = serde_json::from_slice(&read_required(
        &frontend.join("package.json"),
        "SUPPLY_CHAIN_PROVENANCE_MISSING",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISSING",
            format!("parse frontend/package.json: {error}"),
        )
    })?;
    let root_name = root_manifest
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISSING",
                "frontend/package.json has no package name",
            )
        })?;
    let root_version = root_manifest
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISSING",
                "frontend/package.json has no package version",
            )
        })?;
    let root_license = root_manifest
        .get("license")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let root_ref = npm_purl(root_name, root_version);
    let root_notices = Vec::new();
    let root_detected_license = "not-evaluated".to_owned();
    let root_license_detection = "not-evaluated".to_owned();
    components.push(Component {
        bom_ref: root_ref.clone(),
        ecosystem: "npm".to_owned(),
        name: root_name.to_owned(),
        version: root_version.to_owned(),
        features: Vec::new(),
        dependency_kinds: vec!["root".to_owned()],
        targets: vec!["android".to_owned(), "h5".to_owned()],
        source: "workspace-path".to_owned(),
        source_checksum: Some(sha256_identity(&lock_bytes)),
        declared_license: root_license,
        detected_license: root_detected_license,
        notices: root_notices,
        exposure: "product-root".to_owned(),
        install_paths: vec!["frontend".to_owned()],
        metadata_observations: Vec::new(),
        provenance: BTreeMap::from([
            (
                "frontend.authority".to_owned(),
                "workspace-package-and-lock".to_owned(),
            ),
            (
                "frontend.lock".to_owned(),
                "frontend/package-lock.json".to_owned(),
            ),
            (
                "frontend.licenseDetection".to_owned(),
                root_license_detection,
            ),
        ]),
    });

    for (install_path, package) in &lock.packages {
        if install_path.is_empty() || !install_path.starts_with("node_modules/") {
            continue;
        }
        let package_root = frontend.join(install_path);
        if !package_root.is_dir() {
            continue;
        }
        let manifest_path = package_root.join("package.json");
        let manifest_bytes = read_required(&manifest_path, "SUPPLY_CHAIN_PROVENANCE_MISSING")?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&manifest_bytes).map_err(|error| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_PROVENANCE_MISSING",
                    format!("parse '{}': {error}", manifest_path.display()),
                )
            })?;
        let name = manifest
            .get("name")
            .and_then(serde_json::Value::as_str)
            .or(package.name.as_deref())
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_PROVENANCE_MISSING",
                    format!("{install_path} has no package name"),
                )
            })?;
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .or(package.version.as_deref())
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_PROVENANCE_MISSING",
                    format!("{install_path} has no exact package version"),
                )
            })?;
        if package.version.as_deref() != Some(version) {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                format!("{install_path} installed version {version} disagrees with the lock"),
            ));
        }
        let purl = npm_purl(name, version);
        let source = package.resolved.clone().ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISSING",
                format!("{purl} has no exact resolved source"),
            )
        })?;
        let integrity = package.integrity.clone().ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISSING",
                format!("{purl} has no registry integrity"),
            )
        })?;
        validate_npm_lock_integrity(&integrity, &purl)?;
        // Declarations are unreviewed inventory metadata, not admission gates.
        let declared = manifest
            .get("license")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                package
                    .license
                    .as_ref()
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            });
        let notices = Vec::new();
        let detected = "not-evaluated".to_owned();
        let license_detection = "not-evaluated".to_owned();
        components.push(Component {
            bom_ref: purl,
            ecosystem: "npm".to_owned(),
            name: name.to_owned(),
            version: version.to_owned(),
            features: Vec::new(),
            dependency_kinds: vec![if package.dev { "development" } else { "normal" }.to_owned()],
            targets: vec!["android".to_owned(), "h5".to_owned()],
            source: source.clone(),
            source_checksum: Some(integrity.clone()),
            declared_license: declared,
            detected_license: detected,
            notices,
            exposure: if package.dev {
                "build-tool-executed"
            } else {
                "shipped-candidate"
            }
            .to_owned(),
            install_paths: vec![format!("frontend/{install_path}")],
            metadata_observations: Vec::new(),
            provenance: BTreeMap::from([
                (
                    "frontend.authority".to_owned(),
                    "npm-lock-and-installed-manifest".to_owned(),
                ),
                ("frontend.integrity".to_owned(), integrity),
                (
                    "frontend.manifestSha256".to_owned(),
                    sha256_identity(&manifest_bytes),
                ),
                ("frontend.licenseDetection".to_owned(), license_detection),
            ]),
        });
    }

    for (install_path, package) in &lock.packages {
        if !install_path.is_empty()
            && (!install_path.starts_with("node_modules/") || !frontend.join(install_path).is_dir())
        {
            continue;
        }
        let (from, fields) = if install_path.is_empty() {
            (
                root_ref.clone(),
                [
                    (&package.dependencies, "normal"),
                    (&package.dev_dependencies, "development"),
                    (&package.optional_dependencies, "optional"),
                    (&package.peer_dependencies, "peer"),
                ],
            )
        } else {
            let manifest: serde_json::Value = serde_json::from_slice(&read_required(
                &frontend.join(install_path).join("package.json"),
                "SUPPLY_CHAIN_PROVENANCE_MISSING",
            )?)
            .map_err(|error| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_PROVENANCE_MISSING",
                    format!("parse {install_path}/package.json: {error}"),
                )
            })?;
            let name = manifest.get("name").and_then(serde_json::Value::as_str);
            let version = manifest.get("version").and_then(serde_json::Value::as_str);
            let (Some(name), Some(version)) = (name, version) else {
                continue;
            };
            (
                npm_purl(name, version),
                [
                    (&package.dependencies, "normal"),
                    (&package.dev_dependencies, "development"),
                    (&package.optional_dependencies, "optional"),
                    (&package.peer_dependencies, "peer"),
                ],
            )
        };
        let mut by_to = BTreeMap::<String, BTreeSet<String>>::new();
        for (field, kind) in fields {
            for name in field.keys() {
                if let Some(resolved_path) = resolve_npm_path(install_path, name, &lock.packages) {
                    if !frontend.join(&resolved_path).is_dir() {
                        continue;
                    }
                    let target = &lock.packages[&resolved_path];
                    let manifest: serde_json::Value = serde_json::from_slice(&read_required(
                        &frontend.join(&resolved_path).join("package.json"),
                        "SUPPLY_CHAIN_PROVENANCE_MISSING",
                    )?)
                    .map_err(|error| {
                        SupplyChainFailure::new(
                            "SUPPLY_CHAIN_PROVENANCE_MISSING",
                            format!("parse {resolved_path}/package.json: {error}"),
                        )
                    })?;
                    let target_name = manifest
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(name);
                    let Some(version) = manifest
                        .get("version")
                        .and_then(serde_json::Value::as_str)
                        .or(target.version.as_deref())
                    else {
                        continue;
                    };
                    by_to
                        .entry(npm_purl(target_name, version))
                        .or_default()
                        .insert(kind.to_owned());
                }
            }
        }
        for (to, kinds) in by_to {
            dependencies.push(DependencyEdge {
                from: from.clone(),
                to,
                kinds: kinds.into_iter().collect(),
                targets: vec!["android".to_owned(), "h5".to_owned()],
                conditions: Vec::new(),
            });
        }
    }
    Ok(())
}

fn resolve_npm_path(
    from: &str,
    name: &str,
    packages: &BTreeMap<String, NpmPackage>,
) -> Option<String> {
    let mut directory = PathBuf::from(from);
    loop {
        let candidate = if directory.as_os_str().is_empty() {
            PathBuf::from("node_modules").join(name)
        } else {
            directory.join("node_modules").join(name)
        };
        let candidate = candidate.to_string_lossy().replace('\\', "/");
        if packages.contains_key(&candidate) {
            return Some(candidate);
        }
        if directory.as_os_str().is_empty() || !directory.pop() {
            break;
        }
    }
    None
}

fn validate_cargo_lock_identity(
    source: &str,
    checksum: Option<&str>,
    purl: &str,
) -> SupplyResult<()> {
    if source.starts_with("registry+")
        && !checksum.is_some_and(|value| {
            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISSING",
            format!("{purl} registry dependency has no exact Cargo.lock checksum"),
        ));
    }
    Ok(())
}

fn validate_npm_lock_integrity(integrity: &str, purl: &str) -> SupplyResult<()> {
    if !(integrity.starts_with("sha512-") || integrity.starts_with("sha256-")) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISSING",
            format!("{purl} has no supported npm integrity"),
        ));
    }
    Ok(())
}

fn merge_components(components: &mut Vec<Component>) -> SupplyResult<()> {
    let mut merged = BTreeMap::<String, Component>::new();
    for component in components.drain(..) {
        if let Some(existing) = merged.get_mut(&component.bom_ref) {
            merge_component_facts(existing, component)?;
        } else {
            merged.insert(component.bom_ref.clone(), component);
        }
    }
    *components = merged.into_values().collect();
    Ok(())
}

fn merge_target_components(components: &mut Vec<Component>) -> SupplyResult<()> {
    let mut merged = BTreeMap::<(String, Vec<String>), Component>::new();
    for mut component in components.drain(..) {
        component.targets = sorted_unique(component.targets);
        let key = (component.bom_ref.clone(), component.targets.clone());
        if let Some(existing) = merged.get_mut(&key) {
            merge_component_facts(existing, component)?;
        } else {
            merged.insert(key, component);
        }
    }
    *components = merged.into_values().collect();
    Ok(())
}

fn merge_component_facts(existing: &mut Component, mut component: Component) -> SupplyResult<()> {
    if existing.ecosystem != component.ecosystem
        || existing.name != component.name
        || existing.version != component.version
        || existing.source != component.source
        || existing.source_checksum != component.source_checksum
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_COMPONENT_COLLISION",
            format!(
                "{} identifies conflicting component material",
                component.bom_ref
            ),
        ));
    }
    let existing_rank = exposure_rank(&existing.exposure)?;
    let component_rank = exposure_rank(&component.exposure)?;
    // Metadata differences do not change the lock identity. Retain each observed
    // value with its installation/target context instead of making it a gate or
    // silently presenting one installation's metadata as universal.
    let mut observations = existing.metadata_observations();
    observations.extend(component.metadata_observations());
    let mut provenance_values = BTreeMap::<String, BTreeSet<String>>::new();
    for observation in &observations {
        for (key, value) in &observation.provenance {
            provenance_values
                .entry(key.clone())
                .or_default()
                .insert(value.clone());
        }
    }
    existing.provenance = provenance_values
        .into_iter()
        .filter_map(|(key, values)| {
            (values.len() == 1).then(|| (key, values.into_iter().next().unwrap()))
        })
        .collect();
    if existing.declared_license != component.declared_license {
        existing.declared_license = None;
    }
    existing.metadata_observations = observations;
    if component_rank > existing_rank {
        existing.exposure = component.exposure.clone();
    }
    existing.features.append(&mut component.features);
    existing
        .dependency_kinds
        .append(&mut component.dependency_kinds);
    existing.targets.append(&mut component.targets);
    existing.notices.append(&mut component.notices);
    existing.install_paths.append(&mut component.install_paths);
    existing.features = sorted_unique(std::mem::take(&mut existing.features));
    existing.dependency_kinds = sorted_unique(std::mem::take(&mut existing.dependency_kinds));
    existing.targets = sorted_unique(std::mem::take(&mut existing.targets));
    existing
        .notices
        .sort_by(|left, right| (&left.path, &left.sha256).cmp(&(&right.path, &right.sha256)));
    existing.notices.dedup();
    existing.install_paths = sorted_unique(std::mem::take(&mut existing.install_paths));
    Ok(())
}

fn exposure_rank(exposure: &str) -> SupplyResult<u8> {
    match exposure {
        "build-tool-executed" => Ok(0),
        "shipped-candidate" => Ok(1),
        "shipped-linked" => Ok(2),
        _ => Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_COMPONENT_COLLISION",
            format!("unknown component exposure {exposure:?}"),
        )),
    }
}

fn write_cyclonedx(
    path: &Path,
    target: &str,
    components: &[Component],
    dependencies: &[DependencyEdge],
) -> SupplyResult<()> {
    let selected = components
        .iter()
        .filter(|component| {
            component
                .targets
                .iter()
                .any(|candidate| candidate == target)
                && (component.exposure != "build-tool-executed"
                    || matches!(target, "h5" | "android"))
        })
        .collect::<Vec<_>>();
    let refs = selected
        .iter()
        .map(|component| component.bom_ref.as_str())
        .collect::<BTreeSet<_>>();
    let sbom_components = selected
        .iter()
        .map(|component| {
            let mut value = json!({
                "type": "library",
                "bom-ref": component.bom_ref,
                "name": component.name,
                "version": component.version,
                "purl": component.bom_ref,
                "properties": [
                    {"name": "yydra:license-review", "value": "not-evaluated"},
                    {"name": "yydra:dependency-provenance-review", "value": "not-evaluated"},
                    {"name": "yydra:exposure", "value": component.exposure},
                    {"name": "yydra:source", "value": component.source},
                    {"name": "yydra:dependency-kinds", "value": component.dependency_kinds.join(",")},
                    {"name": "yydra:features", "value": component.features.join(",")}
                ]
            });
            if let Some(checksum) = &component.source_checksum {
                value["properties"]
                    .as_array_mut()
                    .expect("component properties are an array")
                    .push(json!({
                        "name": "yydra:source-integrity",
                        "value": checksum
                    }));
            }
            value
        })
        .collect::<Vec<_>>();
    let mut by_ref = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in dependencies {
        if edge.targets.iter().any(|candidate| candidate == target)
            && refs.contains(edge.from.as_str())
            && refs.contains(edge.to.as_str())
        {
            by_ref
                .entry(edge.from.clone())
                .or_default()
                .insert(edge.to.clone());
        }
    }
    for reference in &refs {
        by_ref.entry((*reference).to_owned()).or_default();
    }
    let incoming = by_ref
        .values()
        .flat_map(|dependencies| dependencies.iter().cloned())
        .collect::<BTreeSet<_>>();
    let roots = refs
        .iter()
        .filter(|reference| !incoming.contains(**reference))
        .map(|reference| (*reference).to_owned())
        .collect::<BTreeSet<_>>();
    let application_ref = format!("pkg:generic/yydra-{target}@{DISTRIBUTION_VERSION}");
    by_ref.insert(application_ref.clone(), roots);
    let sbom_dependencies = by_ref
        .into_iter()
        .map(|(reference, depends_on)| {
            json!({
                "ref": reference,
                "dependsOn": depends_on.into_iter().collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    write_json(
        path,
        &json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "version": 1,
            "metadata": {
                "component": {
                    "type": "application",
                    "bom-ref": application_ref,
                    "name": target,
                    "version": DISTRIBUTION_VERSION
                },
                "properties": [{
                    "name": "yydra:claim-boundary",
                    "value": "Inventory and known-policy results only; not blanket legal compatibility, absence of malicious code, or notice completeness."
                }]
            },
            "components": sbom_components,
            "dependencies": sbom_dependencies
        }),
    )
}

fn write_notices(path: &Path, components: &[Component], boundary: &str) -> SupplyResult<()> {
    write_bytes(path, &render_notices(components, boundary)?)
}

fn render_notices(components: &[Component], boundary: &str) -> SupplyResult<Vec<u8>> {
    let mut output = String::from("Yydra V0 target dependency notices\n\n");
    output.push_str(boundary);
    output.push_str("\n\n");
    let mut texts = BTreeMap::<String, (BTreeSet<String>, String)>::new();
    for component in components {
        output.push_str(&format!(
            "{}\n  license: {}\n  source: {}\n  exposure: {}\n",
            component.bom_ref, component.detected_license, component.source, component.exposure
        ));
        for notice in &component.notices {
            output.push_str(&format!("  notice: {} {}\n", notice.path, notice.sha256));
            let entry = texts
                .entry(notice.sha256.clone())
                .or_insert_with(|| (BTreeSet::new(), notice.text.clone()));
            if entry.1 != notice.text {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_NOTICE_INVALID",
                    format!(
                        "notice digest {} identifies conflicting content",
                        notice.sha256
                    ),
                ));
            }
            entry
                .0
                .insert(format!("{} / {}", component.bom_ref, notice.path));
        }
    }
    for (sha256, (applies_to, text)) in texts {
        output.push_str(&format!(
            "\n--- BEGIN NOTICE {sha256} ---\napplies-to: {}\n\n{}",
            applies_to.into_iter().collect::<Vec<_>>().join(", "),
            text
        ));
        if !text.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!("--- END NOTICE {sha256} ---\n"));
    }
    Ok(output.into_bytes())
}

fn cargo_purl(name: &str, version: &str) -> String {
    format!("pkg:cargo/{name}@{version}")
}

fn npm_purl(name: &str, version: &str) -> String {
    let name = if let Some((scope, package)) = name.split_once('/') {
        format!("%40{}/{package}", scope.trim_start_matches('@'))
    } else {
        name.to_owned()
    };
    format!("pkg:npm/{name}@{version}")
}

fn stable_cargo_install_path(root: &Path, package: &CargoPackage) -> String {
    let package_root = Path::new(&package.manifest_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    package_root.strip_prefix(root).map_or_else(
        |_| format!("cargo-registry/{}-{}", package.name, package.version),
        |path| path.display().to_string(),
    )
}

fn stable_cargo_manifest_path(root: &Path, package: &CargoPackage) -> String {
    format!("{}/Cargo.toml", stable_cargo_install_path(root, package))
}

fn stable_cargo_license_file(root: &Path, package: &CargoPackage) -> String {
    let Some(license_file) = package.license_file.as_deref() else {
        return "manifest-license".to_owned();
    };
    let package_root = Path::new(&package.manifest_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let license_path = Path::new(license_file);
    let absolute = if license_path.is_absolute() {
        license_path.to_path_buf()
    } else {
        package_root.join(license_path)
    };
    absolute.strip_prefix(package_root).map_or_else(
        |_| {
            format!(
                "{}/{}",
                stable_cargo_install_path(root, package),
                license_path
                    .file_name()
                    .unwrap_or_else(|| OsStr::new("LICENSE"))
                    .to_string_lossy()
            )
        },
        |relative| {
            format!(
                "{}/{}",
                stable_cargo_install_path(root, package),
                relative.display()
            )
        },
    )
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn sha256_identity(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn read_required(path: &Path, code: &'static str) -> SupplyResult<Vec<u8>> {
    read_bounded(path, MAX_REQUIRED_FILE_BYTES, code)
}

fn read_bounded(path: &Path, max_bytes: u64, code: &'static str) -> SupplyResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        SupplyChainFailure::new(code, format!("inspect '{}': {error}", path.display()))
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        return Err(SupplyChainFailure::new(
            code,
            format!(
                "'{}' is not a plain file within the {max_bytes} byte read limit",
                path.display()
            ),
        ));
    }
    let mut file = File::open(path).map_err(|error| {
        SupplyChainFailure::new(code, format!("open '{}': {error}", path.display()))
    })?;
    let limit = max_bytes
        .checked_add(1)
        .ok_or_else(|| SupplyChainFailure::new(code, "bounded read limit overflow"))?;
    let mut bytes =
        Vec::with_capacity(usize::try_from(metadata.len().min(1024 * 1024)).unwrap_or(1024 * 1024));
    std::io::Read::by_ref(&mut file)
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            SupplyChainFailure::new(code, format!("read '{}': {error}", path.display()))
        })?;
    let actual = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if actual > max_bytes || actual != metadata.len() {
        return Err(SupplyChainFailure::new(
            code,
            format!(
                "'{}' changed while being read or exceeded the {max_bytes} byte read limit",
                path.display()
            ),
        ));
    }
    Ok(bytes)
}

fn sha256_file(path: &Path, max_bytes: u64, code: &'static str) -> SupplyResult<(u64, String)> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        SupplyChainFailure::new(code, format!("inspect '{}': {error}", path.display()))
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        return Err(SupplyChainFailure::new(
            code,
            format!(
                "'{}' is not a plain file within the {max_bytes} byte hash limit",
                path.display()
            ),
        ));
    }
    let mut file = File::open(path).map_err(|error| {
        SupplyChainFailure::new(code, format!("open '{}': {error}", path.display()))
    })?;
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            SupplyChainFailure::new(code, format!("read '{}': {error}", path.display()))
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        bytes = bytes
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or_else(|| SupplyChainFailure::new(code, "file byte count overflow"))?;
        if bytes > max_bytes {
            return Err(SupplyChainFailure::new(
                code,
                format!("'{}' grew beyond the bounded hash limit", path.display()),
            ));
        }
    }
    if bytes != metadata.len() {
        return Err(SupplyChainFailure::new(
            code,
            format!("'{}' changed while being hashed", path.display()),
        ));
    }
    Ok((bytes, format!("sha256:{}", hex::encode(digest.finalize()))))
}

fn write_json(path: &Path, value: &impl Serialize) -> SupplyResult<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(evidence_failure)?;
    bytes.push(b'\n');
    write_bytes(path, &bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> SupplyResult<()> {
    let mut file = create_private_file(path).map_err(evidence_failure)?;
    file.write_all(bytes).map_err(evidence_failure)?;
    file.flush().map_err(evidence_failure)
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

fn evidence_failure(error: impl std::fmt::Display) -> SupplyChainFailure {
    SupplyChainFailure::new("SUPPLY_CHAIN_EVIDENCE_WRITE_FAILED", error.to_string())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseFile {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceMapBinding {
    authority: String,
    bundle_path: String,
    bundle_sha256: String,
    source_map_path: String,
    source_map_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApkEntry {
    path: String,
    crc32: String,
    compressed_bytes: u64,
    bytes: u64,
    compressed_sha256: String,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GradleComponent {
    purl: String,
    group: String,
    name: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawGradleMaterialManifest {
    schema_version: u64,
    configuration: String,
    bundle_binding: RawGradleBundleBinding,
    #[serde(rename = "repositories", default)]
    _legacy_repositories: serde::de::IgnoredAny,
    components: Vec<RawGradleMaterial>,
    dependencies: Vec<GradleDependencyEdge>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawGradleBundleBinding {
    task_path: String,
    bundle_file: String,
    source_map_file: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleRepository {
    scope: String,
    name: String,
    kind: String,
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawGradleMaterial {
    component_type: String,
    group: String,
    name: String,
    version: String,
    project_path: Option<String>,
    source_directory: Option<String>,
    artifacts: Vec<RawGradleArtifact>,
}

impl RawGradleMaterial {
    fn deduplicate_artifacts(&mut self) {
        // Different selected variants can resolve to the same physical artifact.
        // Collapse only identical records before any raw/recorded pairing; conflicting
        // metadata for one path still reaches the fail-closed identity validation.
        let mut seen = BTreeSet::new();
        self.artifacts.retain(|artifact| {
            seen.insert((
                artifact.file.clone(),
                artifact.extension.clone(),
                artifact.classifier.clone(),
            ))
        });
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawGradleArtifact {
    file: String,
    extension: String,
    classifier: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleDependencyEdge {
    from: String,
    to: String,
    selected_variant: String,
    selected_variant_attributes: BTreeMap<String, String>,
    #[serde(default)]
    requested: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleMaterialFile {
    path: String,
    extension: String,
    classifier: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleMaterialNotice {
    artifact_path: String,
    entry_path: String,
    evidence_path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleNativeEntry {
    artifact_path: String,
    entry_path: String,
    apk_path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleRuntimeMaterial {
    purl: String,
    group: String,
    name: String,
    version: String,
    exposure: String,
    source: String,
    source_integrity: String,
    declared_license: Option<String>,
    detected_license: String,
    artifacts: Vec<GradleMaterialFile>,
    metadata: Vec<GradleMaterialFile>,
    notices: Vec<GradleMaterialNotice>,
    #[serde(default)]
    native_entries: Vec<GradleNativeEntry>,
    provenance: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleLocalMaterial {
    project_path: String,
    group: String,
    name: String,
    version: String,
    exposure: String,
    source_path: String,
    source_authority: String,
    artifact_set_sha256: String,
    artifacts: Vec<GradleMaterialFile>,
    #[serde(default)]
    native_entries: Vec<GradleNativeEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AndroidGradleMaterialInventory {
    schema_version: u64,
    distribution_version: String,
    configuration: String,
    source_authority: String,
    bundle_binding: GradleBundleBinding,
    repositories: Vec<GradleRepository>,
    components: Vec<GradleRuntimeMaterial>,
    local_components: Vec<GradleLocalMaterial>,
    dependencies: Vec<GradleDependencyEdge>,
    report_boundary: String,
    #[serde(default)]
    not_evaluated: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GradleBundleBinding {
    task_path: String,
    bundle_path: String,
    bundle_bytes: u64,
    bundle_sha256: String,
    source_map_path: String,
    source_map_bytes: u64,
    source_map_sha256: String,
}

pub(crate) fn record_android_gradle_materials(
    android_root: &Path,
    gradle_user_home: &Path,
    raw_manifest_path: &Path,
    output_path: &Path,
) -> SupplyResult<()> {
    let raw: RawGradleMaterialManifest = serde_json::from_slice(&read_required(
        raw_manifest_path,
        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("parse Gradle runtime material manifest: {error}"),
        )
    })?;
    if raw.schema_version != RAW_GRADLE_MATERIAL_SCHEMA_VERSION
        || raw.configuration != "releaseRuntimeClasspath"
        || raw.bundle_binding.task_path != ":app:createBundleReleaseJsAndAssets"
        || raw.components.is_empty()
        || raw.components.len() > MAX_GRADLE_COMPONENTS
        || raw.dependencies.is_empty()
        || raw.dependencies.len() > MAX_GRADLE_DEPENDENCY_EDGES
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle runtime material manifest must identify exact dependency edges and non-empty releaseRuntimeClasspath materials in schema 3",
        ));
    }
    let canonical_android = fs::canonicalize(android_root).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("resolve generated Android root: {error}"),
        )
    })?;
    let canonical_gradle_home = fs::canonicalize(gradle_user_home).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("resolve isolated Gradle cache: {error}"),
        )
    })?;
    let bundle_binding = record_gradle_bundle_binding(&canonical_android, raw.bundle_binding)?;
    let mut dependencies = raw.dependencies;
    dependencies.sort();
    dependencies.dedup();
    let mut external = BTreeMap::<String, GradleRuntimeMaterial>::new();
    let mut local = BTreeMap::<String, GradleLocalMaterial>::new();
    for mut component in raw.components {
        component.deduplicate_artifacts();
        if component.group.is_empty()
            || component.name.is_empty()
            || component.version.is_empty()
            || component.version == "unspecified"
            || component.group.contains('/')
            || component.name.contains('/')
            || component.version.contains('/')
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "Gradle runtime material contains an invalid exact coordinate",
            ));
        }
        let is_external = component.component_type == "module";
        let project_path = component.project_path.as_deref().unwrap_or_default();
        if (is_external && (!project_path.is_empty() || component.source_directory.is_some()))
            || (!is_external
                && (component.component_type != "project"
                    || project_path.is_empty()
                    || !project_path.starts_with(':')))
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!(
                    "Gradle runtime material {}:{}:{} has unknown component type {:?}",
                    component.group, component.name, component.version, component.component_type
                ),
            ));
        }
        let mut artifacts = record_gradle_material_files(
            &component,
            &canonical_gradle_home,
            &canonical_android,
            is_external,
        )?;
        if is_external {
            let purl = format!(
                "pkg:maven/{}/{}@{}",
                component.group, component.name, component.version
            );
            let native_entries = collect_gradle_native_entries(&component, &artifacts)?;
            artifacts.sort_by(|left, right| {
                (&left.path, &left.sha256).cmp(&(&right.path, &right.sha256))
            });
            let material = GradleRuntimeMaterial {
                purl: purl.clone(),
                group: component.group,
                name: component.name,
                version: component.version,
                exposure: if artifacts.is_empty() {
                    "dependency-graph-only".to_owned()
                } else {
                    "runtime-build-input".to_owned()
                },
                source: "gradle:releaseRuntimeClasspath".to_owned(),
                source_integrity: sha256_identity(
                    artifacts
                        .iter()
                        .map(|artifact| format!("{}\0{}\n", artifact.path, artifact.sha256))
                        .collect::<Vec<_>>()
                        .concat()
                        .as_bytes(),
                ),
                declared_license: None,
                detected_license: "not-evaluated".to_owned(),
                artifacts,
                metadata: Vec::new(),
                notices: Vec::new(),
                native_entries,
                provenance: BTreeMap::from([
                    (
                        "integrityBasis".to_owned(),
                        "selected-artifact-set".to_owned(),
                    ),
                    ("licenseReview".to_owned(), "not-evaluated".to_owned()),
                    ("upstreamProvenance".to_owned(), "not-evaluated".to_owned()),
                ]),
            };
            if external.insert(purl.clone(), material).is_some() {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    format!("Gradle runtime manifest repeats external component {purl}"),
                ));
            }
        } else if project_path != ":app" || !artifacts.is_empty() {
            let source_path = component
                .source_directory
                .as_deref()
                .and_then(|source| {
                    Path::new(source)
                        .strip_prefix(canonical_android.parent()?)
                        .ok()
                })
                .filter(|relative| {
                    relative
                        .components()
                        .all(|part| matches!(part, PathComponent::Normal(_)))
                })
                .map(|relative| {
                    format!("frontend/{}", relative.to_string_lossy().replace('\\', "/"))
                })
                .unwrap_or_else(|| "not-recorded".to_owned());
            let native_entries = collect_gradle_native_entries(&component, &artifacts)?;
            artifacts.sort_by(|left, right| {
                (&left.path, &left.sha256).cmp(&(&right.path, &right.sha256))
            });
            let artifact_set_sha256 = sha256_identity(
                artifacts
                    .iter()
                    .map(|artifact| format!("{}\0{}\n", artifact.path, artifact.sha256))
                    .collect::<Vec<_>>()
                    .concat()
                    .as_bytes(),
            );
            let material = GradleLocalMaterial {
                project_path: project_path.to_owned(),
                group: component.group,
                name: component.name,
                version: component.version,
                exposure: if artifacts.is_empty() {
                    "dependency-graph-only".to_owned()
                } else {
                    "runtime-build-input".to_owned()
                },
                source_path,
                source_authority: "not-evaluated".to_owned(),
                artifact_set_sha256,
                artifacts,
                native_entries,
            };
            if local
                .insert(material.project_path.clone(), material)
                .is_some()
            {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    format!("Gradle runtime manifest repeats project {project_path}"),
                ));
            }
        }
    }
    if external.is_empty() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle releaseRuntimeClasspath contains no external module components",
        ));
    }
    validate_gradle_dependency_edges(&dependencies, &external, &local)?;
    write_json(
        output_path,
        &AndroidGradleMaterialInventory {
            schema_version: GRADLE_MATERIAL_SCHEMA_VERSION,
            distribution_version: DISTRIBUTION_VERSION.to_owned(),
            configuration: "releaseRuntimeClasspath".to_owned(),
            source_authority: GRADLE_MATERIAL_AUTHORITY.to_owned(),
            bundle_binding,
            repositories: Vec::new(),
            components: external.into_values().collect(),
            local_components: local.into_values().collect(),
            dependencies,
            report_boundary: "Runtime-classpath resolution edges, selected variants, artifact hashes and AAR native-entry hashes identify selected build inputs; they do not by themselves prove which classes or resources survive Android packaging. APK entries are inventoried separately. License review, source trust, upstream provenance and notice completeness are not evaluated."
                .to_owned(),
            not_evaluated: NOT_EVALUATED.iter().map(|value| (*value).to_owned()).collect(),
        },
    )
}

fn gradle_project_ref(project_path: &str) -> String {
    format!("urn:yydra:gradle-project:{project_path}")
}

fn validate_gradle_dependency_edges(
    dependencies: &[GradleDependencyEdge],
    external: &BTreeMap<String, GradleRuntimeMaterial>,
    local: &BTreeMap<String, GradleLocalMaterial>,
) -> SupplyResult<()> {
    let mut canonical = dependencies.to_vec();
    canonical.sort();
    canonical.dedup();
    if canonical != dependencies {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle dependency edges are not sorted and unique",
        ));
    }
    let root = gradle_project_ref(":app");
    let mut known = external.keys().cloned().collect::<BTreeSet<_>>();
    known.extend(
        local
            .keys()
            .map(|project_path| gradle_project_ref(project_path)),
    );
    known.insert(root.clone());
    if dependencies.iter().any(|edge| {
        edge.from.is_empty()
            || edge.to.is_empty()
            || edge.selected_variant.is_empty()
            || !known.contains(&edge.from)
            || !known.contains(&edge.to)
            || edge
                .selected_variant_attributes
                .iter()
                .any(|(name, value)| name.is_empty() || value.is_empty())
    }) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle dependency edges contain an unknown component or incomplete selected-variant identity",
        ));
    }
    let mut outgoing = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in dependencies {
        outgoing
            .entry(edge.from.clone())
            .or_default()
            .insert(edge.to.clone());
    }
    let mut reachable = BTreeSet::from([root.clone()]);
    let mut queue = VecDeque::from([root]);
    while let Some(next) = queue.pop_front() {
        for dependency in outgoing.get(&next).into_iter().flatten() {
            if reachable.insert(dependency.clone()) {
                queue.push_back(dependency.clone());
            }
        }
    }
    if known.iter().any(|reference| !reachable.contains(reference)) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle dependency edges do not connect every resolved component to the :app root",
        ));
    }
    Ok(())
}

fn record_gradle_bundle_binding(
    android_root: &Path,
    raw: RawGradleBundleBinding,
) -> SupplyResult<GradleBundleBinding> {
    let expected_bundle =
        android_root.join("app/build/generated/assets/react/release/index.android.bundle");
    let expected_source_map =
        android_root.join("app/build/generated/sourcemaps/react/release/index.android.bundle.map");
    let bundle = fs::canonicalize(&raw.bundle_file).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("resolve Gradle-declared release bundle: {error}"),
        )
    })?;
    let source_map = fs::canonicalize(&raw.source_map_file).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("resolve Gradle-declared release source map: {error}"),
        )
    })?;
    let canonical_expected_bundle = fs::canonicalize(expected_bundle).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("resolve expected release bundle: {error}"),
        )
    })?;
    let canonical_expected_source_map = fs::canonicalize(expected_source_map).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("resolve expected release source map: {error}"),
        )
    })?;
    if raw.task_path != ":app:createBundleReleaseJsAndAssets"
        || bundle != canonical_expected_bundle
        || source_map != canonical_expected_source_map
        || !bundle.starts_with(android_root)
        || !source_map.starts_with(android_root)
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
            "Gradle material capture does not bind the canonical release bundle and source map to :app:createBundleReleaseJsAndAssets",
        ));
    }
    let (bundle_bytes, bundle_sha256) = sha256_file(
        &bundle,
        256 * 1024 * 1024,
        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
    )?;
    let source_map_content = read_bounded(
        &source_map,
        128 * 1024 * 1024,
        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
    )?;
    validate_android_source_map(&source_map_content)?;
    Ok(GradleBundleBinding {
        task_path: raw.task_path,
        bundle_path: "app/build/generated/assets/react/release/index.android.bundle".to_owned(),
        bundle_bytes,
        bundle_sha256,
        source_map_path: "app/build/generated/sourcemaps/react/release/index.android.bundle.map"
            .to_owned(),
        source_map_bytes: source_map_content.len() as u64,
        source_map_sha256: sha256_identity(&source_map_content),
    })
}

fn validate_android_source_map(bytes: &[u8]) -> SupplyResult<()> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("parse Android production source map: {error}"),
        )
    })?;
    // Hermes' composed maps omit `file`. Android's independent identity is the
    // captured Gradle task/output pair, retained hashes, and matching APK bundle.
    // A declaration, when present, must not contradict that identity. Never
    // rewrite the original map to fabricate a declaration or a debug ID.
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(3)
        || value.get("file").is_some_and(|file| {
            file.as_str()
                .and_then(|name| Path::new(name).file_name())
                .and_then(OsStr::to_str)
                != Some("index.android.bundle")
        })
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
            "Android source map has an invalid version or contradicts the canonical Gradle bundle identity",
        ));
    }
    Ok(())
}

fn record_gradle_material_file(
    artifact: &RawGradleArtifact,
    gradle_user_home: &Path,
    android_root: &Path,
    component: &RawGradleMaterial,
    external: bool,
) -> SupplyResult<GradleMaterialFile> {
    if artifact.file.is_empty()
        || artifact.extension.is_empty()
        || artifact.extension.contains('/')
        || artifact.classifier.contains('/')
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle runtime artifact has an invalid path, extension, or classifier",
        ));
    }
    let source = Path::new(&artifact.file);
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "inspect Gradle runtime artifact '{}': {error}",
                source.display()
            ),
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "Gradle runtime artifact '{}' is not a plain file",
                source.display()
            ),
        ));
    }
    let canonical = fs::canonicalize(source).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "resolve Gradle runtime artifact '{}': {error}",
                source.display()
            ),
        )
    })?;
    let frontend_root = android_root.parent().ok_or_else(|| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "generated Android root has no frontend parent",
        )
    })?;
    // This is a bounded-read safety constraint, not repository trust admission or
    // source-to-binary attribution. Hash only files in this build's material roots.
    if !canonical.starts_with(gradle_user_home) && !canonical.starts_with(frontend_root) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle artifact escapes the isolated cache and frontend build workspace",
        ));
    }
    if metadata.len() == 0 || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "Gradle runtime artifact '{}' has invalid bounded size {}",
                canonical.display(),
                metadata.len()
            ),
        ));
    }
    let (bytes, sha256) = sha256_file(
        &canonical,
        MAX_ARCHIVE_BYTES,
        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
    )?;
    let file_name = canonical
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "Gradle runtime artifact has no UTF-8 file name",
            )
        })?;
    let digest = sha256
        .strip_prefix("sha256:")
        .expect("sha256_file always returns a prefixed digest");
    Ok(GradleMaterialFile {
        path: if external {
            format!(
                "{}/{}/{}/{digest}/{}",
                component.group, component.name, component.version, file_name
            )
        } else {
            format!(
                "{}/{digest}/{file_name}",
                component.project_path.as_deref().unwrap_or("")
            )
        },
        extension: artifact.extension.clone(),
        classifier: artifact.classifier.clone(),
        bytes,
        sha256,
    })
}

fn record_gradle_material_files(
    component: &RawGradleMaterial,
    gradle_user_home: &Path,
    android_root: &Path,
    external: bool,
) -> SupplyResult<Vec<GradleMaterialFile>> {
    let mut raw_files = BTreeSet::new();
    let mut recorded_identities = BTreeSet::new();
    let mut recorded = Vec::with_capacity(component.artifacts.len());
    for artifact in &component.artifacts {
        if !raw_files.insert(artifact.file.as_str()) {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!(
                    "Gradle runtime material {}:{}:{} repeats artifact {:?}",
                    component.group, component.name, component.version, artifact.file
                ),
            ));
        }
        let material = record_gradle_material_file(
            artifact,
            gradle_user_home,
            android_root,
            component,
            external,
        )?;
        if !recorded_identities.insert((material.path.clone(), material.sha256.clone())) {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!(
                    "Gradle runtime material {}:{}:{} has ambiguous artifact identity {} {}",
                    component.group,
                    component.name,
                    component.version,
                    material.path,
                    material.sha256
                ),
            ));
        }
        recorded.push(material);
    }
    Ok(recorded)
}

fn validate_android_gradle_inventory(
    root: &Path,
    evidence_root: &Path,
    dependency_graph: &[u8],
) -> SupplyResult<AndroidGradleMaterialInventory> {
    let material_path = evidence_root.join("artifacts/android.release/gradle-materials.json");
    let material_bytes = read_required(&material_path, "SUPPLY_CHAIN_RELEASE_INPUT_INVALID")?;
    let inventory: AndroidGradleMaterialInventory = serde_json::from_slice(&material_bytes)
        .map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("parse Android Gradle material inventory: {error}"),
            )
        })?;
    if inventory.schema_version != GRADLE_MATERIAL_SCHEMA_VERSION
        || inventory.distribution_version != DISTRIBUTION_VERSION
        || inventory.configuration != "releaseRuntimeClasspath"
        || inventory.components.is_empty()
        || inventory.source_authority != GRADLE_MATERIAL_AUTHORITY
        || !inventory
            .report_boundary
            .contains("do not by themselves prove")
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
            "Android Gradle material inventory does not carry the exact schema, Distribution, configuration, authority, or claim boundary",
        ));
    }
    let identity_path = evidence_root.join("artifacts/android.release/artifact.json");
    let identity: serde_json::Value = serde_json::from_slice(&read_required(
        &identity_path,
        "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
    )?)
    .map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
            format!("parse Android artifact identity: {error}"),
        )
    })?;
    let bundle_path = evidence_root.join("artifacts/android.release/index.android.bundle");
    let bundle_bytes = read_bounded(
        &bundle_path,
        256 * 1024 * 1024,
        "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
    )?;
    let source_map_path = evidence_root.join("artifacts/android.release/index.android.bundle.map");
    let source_map_bytes = read_bounded(
        &source_map_path,
        128 * 1024 * 1024,
        "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
    )?;
    if identity
        .get("gradleMaterialInventorySha256")
        .and_then(serde_json::Value::as_str)
        != Some(sha256_identity(&material_bytes).as_str())
        || identity
            .get("resolvedDependencyGraphSha256")
            .and_then(serde_json::Value::as_str)
            != Some(sha256_identity(dependency_graph).as_str())
        || identity
            .get("bundlePath")
            .and_then(serde_json::Value::as_str)
            != Some("index.android.bundle")
        || identity
            .get("bundleBytes")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::try_from(bundle_bytes.len()).unwrap_or(u64::MAX))
        || identity
            .get("bundleSha256")
            .and_then(serde_json::Value::as_str)
            != Some(sha256_identity(&bundle_bytes).as_str())
        || identity
            .get("sourceMapPath")
            .and_then(serde_json::Value::as_str)
            != Some("index.android.bundle.map")
        || identity
            .get("sourceMapBytes")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::try_from(source_map_bytes.len()).unwrap_or(u64::MAX))
        || identity
            .get("sourceMapSha256")
            .and_then(serde_json::Value::as_str)
            != Some(sha256_identity(&source_map_bytes).as_str())
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
            "Android artifact identity does not bind the exact Gradle graph, material inventory, production bundle, and source map",
        ));
    }
    if inventory.bundle_binding.task_path != ":app:createBundleReleaseJsAndAssets"
        || inventory.bundle_binding.bundle_path
            != "app/build/generated/assets/react/release/index.android.bundle"
        || inventory.bundle_binding.bundle_bytes
            != u64::try_from(bundle_bytes.len()).unwrap_or(u64::MAX)
        || inventory.bundle_binding.bundle_sha256 != sha256_identity(&bundle_bytes)
        || inventory.bundle_binding.source_map_path
            != "app/build/generated/sourcemaps/react/release/index.android.bundle.map"
        || inventory.bundle_binding.source_map_bytes
            != u64::try_from(source_map_bytes.len()).unwrap_or(u64::MAX)
        || inventory.bundle_binding.source_map_sha256 != sha256_identity(&source_map_bytes)
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
            "Android Gradle material inventory does not bind the retained production bundle and source map to the canonical release task",
        ));
    }
    validate_android_source_map(&source_map_bytes)?;

    validate_policy_authorities(root)?;

    let retained_external = inventory
        .components
        .iter()
        .cloned()
        .map(|component| (component.purl.clone(), component))
        .collect::<BTreeMap<_, _>>();
    let retained_local = inventory
        .local_components
        .iter()
        .cloned()
        .map(|component| (component.project_path.clone(), component))
        .collect::<BTreeMap<_, _>>();
    if retained_external.len() != inventory.components.len()
        || retained_local.len() != inventory.local_components.len()
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "retained Gradle materials repeat a component identity",
        ));
    }
    validate_gradle_dependency_edges(&inventory.dependencies, &retained_external, &retained_local)?;

    let graph_components = parse_gradle_components(dependency_graph)?
        .into_iter()
        .map(|component| component.purl)
        .collect::<BTreeSet<_>>();
    let actual_external = inventory
        .components
        .iter()
        .map(|component| component.purl.clone())
        .collect::<BTreeSet<_>>();
    if graph_components.is_empty()
        || !graph_components.is_subset(&actual_external)
        || actual_external.len() != inventory.components.len()
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "machine-readable Gradle materials do not cover every external component in the retained releaseRuntimeClasspath graph",
        ));
    }

    for material in &inventory.components {
        if material.group.is_empty()
            || material.name.is_empty()
            || material.version.is_empty()
            || material.purl
                != format!(
                    "pkg:maven/{}/{}@{}",
                    material.group, material.name, material.version
                )
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "retained Maven component has an incomplete or conflicting coordinate",
            ));
        }
        validate_gradle_artifact_inventory(
            &material.purl,
            &material.exposure,
            &material.source_integrity,
            &material.artifacts,
            &material.native_entries,
        )?;
    }
    for material in &inventory.local_components {
        if !material.project_path.starts_with(':')
            || material.project_path == ":app"
            || material.group.is_empty()
            || material.name.is_empty()
            || material.version.is_empty()
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "retained local Gradle component has an incomplete identity",
            ));
        }
        validate_gradle_artifact_inventory(
            &material.project_path,
            &material.exposure,
            &material.artifact_set_sha256,
            &material.artifacts,
            &material.native_entries,
        )?;
    }
    Ok(inventory)
}

fn validate_gradle_artifact_inventory(
    reference: &str,
    exposure: &str,
    artifact_set_sha256: &str,
    artifacts: &[GradleMaterialFile],
    native_entries: &[GradleNativeEntry],
) -> SupplyResult<()> {
    let expected_exposure = if artifacts.is_empty() {
        "dependency-graph-only"
    } else {
        "runtime-build-input"
    };
    let mut paths = BTreeMap::new();
    let mut previous: Option<(&str, &str)> = None;
    for file in artifacts {
        let identity = (file.path.as_str(), file.sha256.as_str());
        if file.path.is_empty()
            || file.extension.is_empty()
            || file.bytes == 0
            || file.bytes > MAX_ARCHIVE_BYTES
            || !valid_sha256(&file.sha256)
            || previous.is_some_and(|last| last >= identity)
            || paths.insert(file.path.as_str(), file).is_some()
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("{reference} contains an invalid or repeated artifact identity"),
            ));
        }
        previous = Some(identity);
    }
    let expected_sha256 = sha256_identity(
        artifacts
            .iter()
            .map(|file| format!("{}\0{}\n", file.path, file.sha256))
            .collect::<Vec<_>>()
            .concat()
            .as_bytes(),
    );
    if exposure != expected_exposure || artifact_set_sha256 != expected_sha256 {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "{reference} has inconsistent artifact-set identity or build-input classification"
            ),
        ));
    }
    let mut native_paths = BTreeSet::new();
    for native in native_entries {
        if !paths.contains_key(native.artifact_path.as_str())
            || gradle_native_apk_path(&native.entry_path).as_deref()
                != Some(native.apk_path.as_str())
            || native.bytes == 0
            || native.bytes > MAX_ARCHIVE_BYTES
            || !valid_sha256(&native.sha256)
            || !native_paths.insert(native.apk_path.as_str())
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("{reference} has an incomplete, repeated or unrecorded native input entry"),
            ));
        }
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.starts_with("sha256:")
        && value.len() == 71
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn release_artifacts(
    root: &Path,
    evidence_root: &Path,
    invoked_cli: &Path,
) -> SupplyResult<()> {
    let dependency_inventory_path =
        evidence_root.join("artifacts/supply-chain.dependencies/inventory.json");
    let advisory_report_path = evidence_root.join("artifacts/supply-chain.advisories/report.json");
    let android_advisory_report_path =
        evidence_root.join("artifacts/supply-chain.android-advisories/report.json");
    let dependency_inventory = read_required(
        &dependency_inventory_path,
        "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
    )?;
    let dependency_inventory_value: serde_json::Value =
        serde_json::from_slice(&dependency_inventory).map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("parse dependency inventory for release linkage: {error}"),
            )
        })?;
    let policy_bytes = read_required(
        &root.join(".yydra/supply-chain-policy.json"),
        "SUPPLY_CHAIN_POLICY_INVALID",
    )?;
    let exception_bytes = read_required(
        &root.join(".yydra/supply-chain-exceptions.json"),
        "SUPPLY_CHAIN_EXCEPTION_INVALID",
    )?;
    let policy: SupplyChainPolicy = serde_json::from_slice(&policy_bytes).map_err(|error| {
        SupplyChainFailure::new("SUPPLY_CHAIN_POLICY_INVALID", error.to_string())
    })?;
    let exceptions: SupplyChainExceptions =
        serde_json::from_slice(&exception_bytes).map_err(|error| {
            SupplyChainFailure::new("SUPPLY_CHAIN_EXCEPTION_INVALID", error.to_string())
        })?;
    validate_policy(&policy, &exceptions)?;
    if dependency_inventory_value
        .get("policySha256")
        .and_then(serde_json::Value::as_str)
        != Some(sha256_identity(&policy_bytes).as_str())
        || dependency_inventory_value
            .get("exceptionsSha256")
            .and_then(serde_json::Value::as_str)
            != Some(sha256_identity(&exception_bytes).as_str())
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
            "dependency inventory policy identities differ from the current exact-Distribution authorities",
        ));
    }
    let advisory_report =
        read_required(&advisory_report_path, "SUPPLY_CHAIN_RELEASE_INPUT_INVALID")?;
    let advisory: serde_json::Value =
        serde_json::from_slice(&advisory_report).map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("parse passing advisory report: {error}"),
            )
        })?;
    if advisory
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || advisory
            .get("distributionVersion")
            .and_then(serde_json::Value::as_str)
            != Some(DISTRIBUTION_VERSION)
        || advisory.get("status").and_then(serde_json::Value::as_str) != Some("pass")
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
            "release evidence requires a passing schema-1 advisory report for this exact Distribution",
        ));
    }
    let android_advisory_report = read_required(
        &android_advisory_report_path,
        "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
    )?;
    let android_advisory: serde_json::Value = serde_json::from_slice(&android_advisory_report)
        .map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("parse passing Android Maven advisory report: {error}"),
            )
        })?;
    if android_advisory
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || android_advisory
            .get("distributionVersion")
            .and_then(serde_json::Value::as_str)
            != Some(DISTRIBUTION_VERSION)
        || android_advisory
            .get("status")
            .and_then(serde_json::Value::as_str)
            != Some("pass")
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
            "release evidence requires a passing schema-1 Android Maven advisory report for this exact Distribution",
        ));
    }
    let dependency_sha256 = sha256_identity(&dependency_inventory);
    let advisory_sha256 = sha256_identity(&advisory_report);
    let android_advisory_sha256 = sha256_identity(&android_advisory_report);
    let output_root = evidence_root.join("artifacts/supply-chain.release-artifacts");
    create_private_dir_all(&output_root).map_err(evidence_failure)?;

    for target in ["cli", "server", "h5", "android"] {
        let target_root = output_root.join(target);
        create_private_dir_all(&target_root).map_err(evidence_failure)?;
        let artifact_root = target_root.join("artifact");
        create_private_dir_all(&artifact_root).map_err(evidence_failure)?;
        let mut gradle_components = Vec::<GradleRuntimeMaterial>::new();
        let mut gradle_local_components = Vec::<GradleLocalMaterial>::new();
        let mut gradle_dependencies = Vec::<GradleDependencyEdge>::new();
        let (files, archive_entries, build_evidence, source_map_paths, source_map_bindings) =
            match target {
                "cli" => {
                    let retained = if cfg!(windows) { "yydra.exe" } else { "yydra" };
                    let file = retain_release_file(
                        invoked_cli,
                        &target_root,
                        &format!("artifact/{retained}"),
                        true,
                        MAX_RELEASE_FILE_BYTES,
                    )?;
                    (
                        vec![file],
                        Vec::new(),
                        json!({
                            "node": "invoked-exact-cli",
                            "source": "the executable running this Mechanical Quality Contract"
                        }),
                        Vec::new(),
                        Vec::new(),
                    )
                }
                "server" => {
                    let retained = if cfg!(windows) {
                        "server.exe"
                    } else {
                        "server"
                    };
                    let source = evidence_root
                        .join("artifacts/server.release")
                        .join(retained);
                    let file = retain_release_file(
                        &source,
                        &target_root,
                        &format!("artifact/{retained}"),
                        true,
                        MAX_RELEASE_FILE_BYTES,
                    )?;
                    (
                        vec![file],
                        Vec::new(),
                        json!({
                            "node": "server.release",
                            "artifactIdentity": evidence_reference(
                                evidence_root,
                                &evidence_root.join("artifacts/server.release/artifact.json")
                            )?
                        }),
                        Vec::new(),
                        Vec::new(),
                    )
                }
                "h5" => {
                    let source = evidence_root.join("artifacts/h5.real-runtime/dist");
                    let files = retain_release_tree(&source, &target_root, "artifact/dist")?;
                    if !files
                        .iter()
                        .any(|file| file.path == "artifact/dist/index.html")
                    {
                        return Err(SupplyChainFailure::new(
                            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                            "the tested production H5 Application Surface artifact has no index.html",
                        ));
                    }
                    let source_maps = files
                        .iter()
                        .filter(|file| file.path.ends_with(".map"))
                        .map(|file| target_root.join(&file.path))
                        .collect::<Vec<_>>();
                    let bindings = validate_h5_source_map_bindings(&target_root, &source_maps)?;
                    (
                        files,
                        Vec::new(),
                        json!({
                            "node": "h5.real-runtime",
                            "source": "the production export exercised by the real-runtime Playwright node"
                        }),
                        source_maps,
                        bindings,
                    )
                }
                "android" => {
                    let source = evidence_root.join("artifacts/android.release/app-release.apk");
                    let file = retain_release_file(
                        &source,
                        &target_root,
                        "artifact/app-release.apk",
                        false,
                        MAX_RELEASE_FILE_BYTES,
                    )?;
                    let apk = read_bounded(
                        &source,
                        MAX_ARCHIVE_BYTES,
                        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    )?;
                    let entries = apk_entries(&apk)?;
                    if !entries.iter().any(|entry| is_native_apk_entry(&entry.path)) {
                        return Err(SupplyChainFailure::new(
                            "SUPPLY_CHAIN_NATIVE_COMPONENT_MISSING",
                            "the Android release APK contains no lib/<abi>/*.so native components",
                        ));
                    }
                    let dependency_graph = evidence_root
                        .join("artifacts/android.release/release-runtime-classpath.txt");
                    let dependency_graph_bytes =
                        read_required(&dependency_graph, "SUPPLY_CHAIN_RELEASE_INPUT_INVALID")?;
                    if dependency_graph_bytes.is_empty() {
                        return Err(SupplyChainFailure::new(
                            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                            "Android release runtime dependency graph is empty",
                        ));
                    }
                    let gradle_inventory = validate_android_gradle_inventory(
                        root,
                        evidence_root,
                        &dependency_graph_bytes,
                    )?;
                    let packaged_bundle = entries
                    .iter()
                    .find(|entry| entry.path == "assets/index.android.bundle")
                    .ok_or_else(|| {
                        SupplyChainFailure::new(
                            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                            "the Android release APK does not contain assets/index.android.bundle",
                        )
                    })?;
                    if packaged_bundle.sha256 != gradle_inventory.bundle_binding.bundle_sha256 {
                        return Err(SupplyChainFailure::new(
                            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                            "the Android release APK JavaScript bundle differs from the Gradle-bound production bundle",
                        ));
                    }
                    let source_map_binding = SourceMapBinding {
                    authority: "exact Gradle bundle task identity plus retained APK bundle entry and source-map SHA-256 identities".to_owned(),
                    bundle_path:
                        "artifact/app-release.apk!/assets/index.android.bundle".to_owned(),
                    bundle_sha256: packaged_bundle.sha256.clone(),
                    source_map_path:
                        "artifacts/android.release/index.android.bundle.map".to_owned(),
                    source_map_sha256: gradle_inventory
                        .bundle_binding
                        .source_map_sha256
                        .clone(),
                };
                    gradle_components = gradle_inventory.components;
                    gradle_local_components = gradle_inventory.local_components;
                    gradle_dependencies = gradle_inventory.dependencies;
                    (
                        vec![file],
                        entries,
                        json!({
                            "node": "android.release",
                            "artifactIdentity": evidence_reference(
                                evidence_root,
                                &evidence_root.join("artifacts/android.release/artifact.json")
                            )?,
                            "nativeGenerationInventory": evidence_reference(
                                evidence_root,
                                &evidence_root.join("artifacts/android.release/native-inventory.json")
                            )?,
                            "releaseRuntimeClasspath": {
                                "path": "artifacts/android.release/release-runtime-classpath.txt",
                                "sha256": sha256_identity(&dependency_graph_bytes)
                            },
                            "gradleMaterialInventory": evidence_reference(
                                evidence_root,
                                &evidence_root.join("artifacts/android.release/gradle-materials.json")
                            )?,
                            "sourceMap": evidence_reference(
                                evidence_root,
                                &evidence_root.join("artifacts/android.release/index.android.bundle.map")
                            )?
                        }),
                        vec![
                            evidence_root
                                .join("artifacts/android.release/index.android.bundle.map"),
                        ],
                        vec![source_map_binding],
                    )
                }
                _ => unreachable!(),
            };
        if files.is_empty() {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("{target} release artifact inventory is empty"),
            ));
        }

        let (source_map_evidence, source_map_sources) = if source_map_paths.is_empty() {
            (Vec::new(), BTreeSet::new())
        } else {
            collect_source_map_evidence(evidence_root, &source_map_paths)?
        };

        let preliminary_sbom = evidence_root
            .join("artifacts/supply-chain.dependencies")
            .join(format!("{target}.cdx.json"));
        let mut sbom: serde_json::Value = serde_json::from_slice(&read_required(
            &preliminary_sbom,
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
        )?)
        .map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("parse {target} preliminary CycloneDX SBOM: {error}"),
            )
        })?;
        let linked_npm_components = if matches!(target, "h5" | "android") {
            link_npm_sbom_to_source_maps(
                target,
                &mut sbom,
                &dependency_inventory_value,
                &source_map_sources,
                &source_map_evidence,
                &BTreeSet::new(),
            )?
        } else {
            Vec::new()
        };
        if target == "android" {
            add_android_artifact_inventory(
                &mut sbom,
                &archive_entries,
                &gradle_components,
                &gradle_local_components,
                &gradle_dependencies,
            )?;
        }
        validate_final_sbom(target, &sbom)?;
        write_json(&target_root.join("sbom.cdx.json"), &sbom)?;

        let inventory_path = target_root.join("artifact-inventory.json");
        write_json(
            &inventory_path,
            &json!({
                "schemaVersion": 4,
                "distributionVersion": DISTRIBUTION_VERSION,
                "target": target,
                "artifacts": files,
                "archiveEntries": archive_entries,
                "sourceMaps": source_map_evidence,
                "sourceMapBindings": source_map_bindings,
                "artifactLinkedNpmComponents": linked_npm_components,
                "nativeGradleComponents": gradle_components,
                "nativeLocalComponents": gradle_local_components,
                "gradleDependencies": gradle_dependencies,
                "notEvaluated": NOT_EVALUATED,
                "claimBoundary": "Archive entries identify actual retained artifact bytes. Gradle components and source-map-listed npm components describe build inputs, not verified upstream source-to-binary attribution. Native entries have no inferred producer or unqueried advisory result.",
            }),
        )?;

        let preliminary_notices = evidence_root
            .join("artifacts/supply-chain.dependencies")
            .join(format!("{target}.THIRD-PARTY-NOTICES.txt"));
        // Retain the available inventory text; do not rescan dependency sources or
        // certify notice completeness. This file is evidence, not license admission.
        let notices = read_required(&preliminary_notices, "SUPPLY_CHAIN_RELEASE_INPUT_INVALID")?;
        write_bytes(&target_root.join("THIRD-PARTY-NOTICES.txt"), &notices)?;

        let inventory_bytes =
            read_required(&inventory_path, "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED")?;
        let sbom_bytes = read_required(
            &target_root.join("sbom.cdx.json"),
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
        )?;
        let mut checksum_entries = files
            .iter()
            .map(|file| (file.path.clone(), file.sha256.clone()))
            .collect::<Vec<_>>();
        checksum_entries.extend([
            (
                "artifact-inventory.json".to_owned(),
                sha256_identity(&inventory_bytes),
            ),
            ("sbom.cdx.json".to_owned(), sha256_identity(&sbom_bytes)),
            (
                "THIRD-PARTY-NOTICES.txt".to_owned(),
                sha256_identity(&notices),
            ),
        ]);
        checksum_entries.sort();
        let checksums = checksum_entries
            .iter()
            .map(|(path, digest)| {
                format!(
                    "{}  {path}\n",
                    digest.strip_prefix("sha256:").unwrap_or(digest)
                )
            })
            .collect::<String>();
        let checksums_path = target_root.join("SHA256SUMS");
        write_bytes(&checksums_path, checksums.as_bytes())?;
        let checksums_bytes =
            read_required(&checksums_path, "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED")?;
        let mut provenance = json!({
            "schemaVersion": 1,
            "distributionVersion": DISTRIBUTION_VERSION,
            "status": "pass",
            "target": target,
            "dependencyInventory": {
                "path": "artifacts/supply-chain.dependencies/inventory.json",
                "sha256": dependency_sha256,
            },
            "advisoryReport": {
                "path": "artifacts/supply-chain.advisories/report.json",
                "sha256": advisory_sha256,
            },
            "artifactInventory": {
                "path": format!("artifacts/supply-chain.release-artifacts/{target}/artifact-inventory.json"),
                "sha256": sha256_identity(&inventory_bytes),
            },
            "checksums": {
                "path": format!("artifacts/supply-chain.release-artifacts/{target}/SHA256SUMS"),
                "sha256": sha256_identity(&checksums_bytes),
            },
            "buildEvidence": build_evidence,
            "notEvaluated": NOT_EVALUATED,
            "claimBoundary": "This report records retained artifact checksums, resolved dependency graphs, reported advisory queries, and build/test evidence in this check run. License review, source trust, upstream provenance and notice completeness are not evaluated. Native file entries are not attributed to upstream producers and do not inherit clean results from unrelated advisory queries. It does not prove cross-host reproducibility, legal compatibility, source authenticity, absence of malicious code, or vulnerability absence outside reported query coverage."
        });
        if target == "android" {
            provenance
                .as_object_mut()
                .expect("provenance is an object")
                .insert(
                    "androidMavenAdvisoryReport".to_owned(),
                    json!({
                        "path": "artifacts/supply-chain.android-advisories/report.json",
                        "sha256": android_advisory_sha256.clone(),
                    }),
                );
        }
        write_json(&target_root.join("provenance.json"), &provenance)?;
    }
    Ok(())
}

fn retain_release_file(
    source: &Path,
    target_root: &Path,
    relative: &str,
    executable: bool,
    byte_budget: u64,
) -> SupplyResult<ReleaseFile> {
    let byte_limit = byte_budget.min(MAX_RELEASE_FILE_BYTES);
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("inspect release artifact '{}': {error}", source.display()),
        )
    })?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > byte_limit
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "release artifact '{}' is not a non-empty plain file within the {} byte limit",
                source.display(),
                byte_limit
            ),
        ));
    }
    let destination = target_root.join(relative);
    let parent = destination.parent().ok_or_else(|| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_EVIDENCE_WRITE_FAILED",
            format!("release destination {relative:?} has no parent"),
        )
    })?;
    create_private_dir_all(parent).map_err(evidence_failure)?;
    let mut input = File::open(source).map_err(evidence_failure)?;
    let mut output = create_private_file(&destination).map_err(evidence_failure)?;
    let mut digest = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(evidence_failure)?;
        if read == 0 {
            break;
        }
        let next_copied = copied
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    "release artifact byte count overflow",
                )
            })?;
        if next_copied > byte_limit {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "release artifact changed beyond the bounded copy limit",
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(evidence_failure)?;
        digest.update(&buffer[..read]);
        copied = next_copied;
    }
    output.flush().map_err(evidence_failure)?;
    if copied != metadata.len() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "release artifact changed while it was retained",
        ));
    }
    #[cfg(unix)]
    if executable {
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700))
            .map_err(evidence_failure)?;
    }
    let _ = executable;
    Ok(ReleaseFile {
        path: relative.to_owned(),
        bytes: copied,
        sha256: format!("sha256:{}", hex::encode(digest.finalize())),
    })
}

fn retain_release_tree(
    source_root: &Path,
    target_root: &Path,
    target_prefix: &str,
) -> SupplyResult<Vec<ReleaseFile>> {
    let metadata = fs::symlink_metadata(source_root).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("inspect release tree '{}': {error}", source_root.display()),
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "release tree '{}' is not a plain directory",
                source_root.display()
            ),
        ));
    }
    let mut pending = vec![source_root.to_path_buf()];
    let mut sources = Vec::new();
    let mut inspected_entries = 0_usize;
    while let Some(directory) = pending.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("read release tree '{}': {error}", directory.display()),
            )
        })? {
            entries.push(entry.map_err(|error| {
                SupplyChainFailure::new("SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED", error.to_string())
            })?);
            inspected_entries = inspected_entries.checked_add(1).ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    "release tree entry count overflow",
                )
            })?;
            if inspected_entries > MAX_RELEASE_TREE_FILES {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    "release tree exceeds the bounded entry-count limit",
                ));
            }
        }
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries.into_iter().rev() {
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
                SupplyChainFailure::new("SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED", error.to_string())
            })?;
            if metadata.file_type().is_symlink() {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    format!("release tree contains symlink '{}'", entry.path().display()),
                ));
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                sources.push(entry.path());
            } else {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    format!(
                        "release tree contains special entry '{}'",
                        entry.path().display()
                    ),
                ));
            }
        }
    }
    sources.sort();
    let total_bytes = sources.iter().try_fold(0_u64, |total, source| {
        let bytes = fs::symlink_metadata(source)
            .map_err(|error| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    format!("inspect release file '{}': {error}", source.display()),
                )
            })?
            .len();
        total.checked_add(bytes).ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "release tree byte count overflow",
            )
        })
    })?;
    if total_bytes > MAX_RELEASE_TREE_BYTES {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!(
                "release tree exceeds the {} byte bounded copy limit",
                MAX_RELEASE_TREE_BYTES
            ),
        ));
    }
    let mut retained = Vec::with_capacity(sources.len());
    let mut retained_bytes = 0_u64;
    for source in &sources {
        let relative = source.strip_prefix(source_root).map_err(|error| {
            SupplyChainFailure::new("SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED", error.to_string())
        })?;
        let remaining = MAX_RELEASE_TREE_BYTES
            .checked_sub(retained_bytes)
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    "release tree retained byte count overflow",
                )
            })?;
        let file = retain_release_file(
            source,
            target_root,
            &format!("{target_prefix}/{}", relative.display()),
            false,
            remaining,
        )?;
        retained_bytes = retained_bytes.checked_add(file.bytes).ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "release tree retained byte count overflow",
            )
        })?;
        retained.push(file);
    }
    Ok(retained)
}

fn evidence_reference(evidence_root: &Path, path: &Path) -> SupplyResult<serde_json::Value> {
    let bytes = read_required(path, "SUPPLY_CHAIN_RELEASE_INPUT_INVALID")?;
    Ok(json!({
        "path": path.strip_prefix(evidence_root)
            .map_err(|error| SupplyChainFailure::new("SUPPLY_CHAIN_RELEASE_INPUT_INVALID", error.to_string()))?
            .display()
            .to_string(),
        "sha256": sha256_identity(&bytes),
    }))
}

fn collect_source_map_evidence(
    evidence_root: &Path,
    paths: &[PathBuf],
) -> SupplyResult<(Vec<ReleaseFile>, BTreeSet<String>)> {
    if paths.is_empty() || paths.len() > 100 {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "H5 Application Surface and Android artifact linkage requires between 1 and 100 source maps",
        ));
    }
    let mut evidence = Vec::new();
    let mut sources = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for path in paths {
        let bytes = read_bounded(
            path,
            128 * 1024 * 1024,
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
        )?;
        total_bytes = total_bytes
            .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    "source-map byte count overflow",
                )
            })?;
        if total_bytes > 256 * 1024 * 1024 {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "source-map evidence exceeds the 256 MiB total read limit",
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("parse source map '{}': {error}", path.display()),
            )
        })?;
        collect_source_map_sources(&value, &mut sources)?;
        let relative = path.strip_prefix(evidence_root).map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!(
                    "source map '{}' escapes evidence root: {error}",
                    path.display()
                ),
            )
        })?;
        evidence.push(ReleaseFile {
            path: relative.display().to_string(),
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: sha256_identity(&bytes),
        });
    }
    evidence.sort_by(|left, right| left.path.cmp(&right.path));
    if sources.is_empty() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "retained source maps contain no source identities",
        ));
    }
    Ok((evidence, sources))
}

fn validate_source_map_declared_bundle(
    source_map: &Path,
    expected_bundle: &str,
) -> SupplyResult<String> {
    let bytes = read_bounded(
        source_map,
        128 * 1024 * 1024,
        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
    )?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("parse source map '{}': {error}", source_map.display()),
        )
    })?;
    let declared = value
        .get("file")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Path::new(value).file_name())
        .and_then(OsStr::to_str);
    if declared != Some(expected_bundle) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
            format!(
                "source map '{}' does not declare its canonical production bundle {expected_bundle:?}",
                source_map.display()
            ),
        ));
    }
    Ok(sha256_identity(&bytes))
}

fn validate_h5_source_map_bindings(
    target_root: &Path,
    source_maps: &[PathBuf],
) -> SupplyResult<Vec<SourceMapBinding>> {
    let mut bindings = Vec::with_capacity(source_maps.len());
    for source_map in source_maps {
        let bundle = source_map.with_extension("");
        if !bundle.is_file() {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                format!(
                    "production H5 Application Surface source map '{}' has no sibling bundle",
                    source_map.display()
                ),
            ));
        }
        let expected_bundle = bundle.file_name().and_then(OsStr::to_str).ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                "production H5 Application Surface bundle name is not UTF-8",
            )
        })?;
        let source_map_sha256 = validate_source_map_declared_bundle(source_map, expected_bundle)?;
        let bundle_bytes = read_bounded(
            &bundle,
            MAX_REQUIRED_FILE_BYTES,
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
        )?;
        let map_name = source_map
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                    "production H5 Application Surface source-map name is not UTF-8",
                )
            })?;
        let bundle_text = String::from_utf8_lossy(&bundle_bytes);
        let web_path = source_map
            .strip_prefix(target_root.join("artifact/dist"))
            .ok()
            .and_then(Path::to_str)
            .map(|path| format!("/{}", path.replace('\\', "/")));
        let declared_urls = bundle_text
            .lines()
            .filter_map(|line| line.strip_prefix("//# sourceMappingURL="))
            .collect::<Vec<_>>();
        if declared_urls.len() != 1
            || (declared_urls[0] != map_name && Some(declared_urls[0]) != web_path.as_deref())
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                format!(
                    "production H5 Application Surface bundle '{}' does not reference its retained source map",
                    bundle.display()
                ),
            ));
        }
        bindings.push(SourceMapBinding {
            authority: "mutual production bundle/source-map filename declarations plus retained SHA-256 identities".to_owned(),
            bundle_path: bundle
                .strip_prefix(target_root)
                .map_err(|error| {
                    SupplyChainFailure::new(
                        "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                        format!("H5 Application Surface bundle escapes target evidence: {error}"),
                    )
                })?
                .display()
                .to_string(),
            bundle_sha256: sha256_identity(&bundle_bytes),
            source_map_path: source_map
                .strip_prefix(target_root)
                .map_err(|error| {
                    SupplyChainFailure::new(
                        "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
                        format!(
                            "H5 Application Surface source map escapes target evidence: {error}"
                        ),
                    )
                })?
                .display()
                .to_string(),
            source_map_sha256,
        });
    }
    bindings.sort_by(|left, right| left.source_map_path.cmp(&right.source_map_path));
    Ok(bindings)
}

fn collect_source_map_sources(
    value: &serde_json::Value,
    sources: &mut BTreeSet<String>,
) -> SupplyResult<()> {
    if let Some(values) = value.get("sources") {
        for source in values.as_array().ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "source-map sources must be an array",
            )
        })? {
            let source = source
                .as_str()
                .filter(|source| !source.is_empty())
                .ok_or_else(|| {
                    SupplyChainFailure::new(
                        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                        "source-map source identity must be a non-empty string",
                    )
                })?;
            sources.insert(source.replace('\\', "/"));
        }
    }
    if let Some(sections) = value.get("sections") {
        for section in sections.as_array().ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "indexed source-map sections must be an array",
            )
        })? {
            let map = section.get("map").ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    "indexed source-map section has no inline map",
                )
            })?;
            collect_source_map_sources(map, sources)?;
        }
    }
    Ok(())
}

fn link_npm_sbom_to_source_maps(
    target: &str,
    sbom: &mut serde_json::Value,
    dependency_inventory: &serde_json::Value,
    sources: &BTreeSet<String>,
    source_maps: &[ReleaseFile],
    additional_linked: &BTreeSet<String>,
) -> SupplyResult<Vec<String>> {
    let inventory_components = dependency_inventory
        .get("components")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                "dependency inventory has no component array for artifact linkage",
            )
        })?;
    let mut roots = BTreeSet::new();
    let mut installs = Vec::<(String, String, String)>::new();
    // This operation narrows npm source-map exposure only. Independently
    // inventoried non-npm materials keep their separate artifact-binding path.
    let other_refs = inventory_components
        .iter()
        .filter(|component| {
            component["ecosystem"] != "npm"
                && component["targets"]
                    .as_array()
                    .is_some_and(|targets| targets.iter().any(|candidate| candidate == target))
        })
        .filter_map(|component| component["bomRef"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    for component in inventory_components {
        if component
            .get("ecosystem")
            .and_then(serde_json::Value::as_str)
            != Some("npm")
            || !component
                .get("targets")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|targets| targets.iter().any(|value| value.as_str() == Some(target)))
        {
            continue;
        }
        let reference = component
            .get("bomRef")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                    "npm inventory component has no bomRef",
                )
            })?;
        let exposure = component
            .get("exposure")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                    format!("{reference} has no exposure for source-map linkage"),
                )
            })?;
        if exposure == "product-root" {
            roots.insert(reference.to_owned());
            continue;
        }
        for path in component
            .get("installPaths")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                    format!("{reference} has no installPaths for source-map linkage"),
                )
            })?
        {
            let path = path.as_str().ok_or_else(|| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                    format!("{reference} has a non-string install path"),
                )
            })?;
            let stable = path
                .strip_prefix("frontend/")
                .unwrap_or(path)
                .replace('\\', "/");
            installs.push((stable, reference.to_owned(), exposure.to_owned()));
        }
    }
    installs.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then(left.cmp(right)));
    let available = installs
        .iter()
        .map(|(_, reference, _)| reference.as_str())
        .chain(roots.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    if additional_linked
        .iter()
        .any(|reference| !available.contains(reference.as_str()))
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISSING",
            format!(
                "{target} Gradle-project or native producer is absent from the exact npm dependency inventory"
            ),
        ));
    }
    let mut source_linked = roots.clone();
    for source in sources {
        let normalized = source.replace('\\', "/");
        if let Some((_, reference, _)) = installs.iter().find(|(install, _, _)| {
            normalized == *install
                || normalized.ends_with(&format!("/{install}"))
                || normalized.contains(&format!("/{install}/"))
                || normalized.starts_with(&format!("{install}/"))
        }) {
            source_linked.insert(reference.clone());
        } else if normalized.contains("node_modules/") {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!(
                    "{target} source map names npm input {normalized:?} that is absent from the exact dependency inventory"
                ),
            ));
        }
    }
    if source_linked.len() == roots.len() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("{target} source maps link no installed npm component"),
        ));
    }
    let mut linked = source_linked.clone();
    linked.extend(additional_linked.iter().cloned());
    let components = sbom
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("{target} SBOM has no mutable components"),
            )
        })?;
    components.retain(|component| {
        component
            .get("bom-ref")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|reference| linked.contains(reference) || other_refs.contains(reference))
    });
    let retained = components
        .iter()
        .filter_map(|component| component.get("bom-ref").and_then(serde_json::Value::as_str))
        .collect::<BTreeSet<_>>();
    if linked
        .iter()
        .any(|reference| !retained.contains(reference.as_str()))
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_BUILD_TOOL_SHIPPED",
            format!(
                "{target} production source map links a component excluded from the final shipped SBOM"
            ),
        ));
    }
    for component in components {
        let reference = component
            .get("bom-ref")
            .and_then(serde_json::Value::as_str)
            .expect("retained SBOM component has a bom-ref")
            .to_owned();
        if other_refs.contains(&reference) {
            continue;
        }
        let properties = component
            .get_mut("properties")
            .and_then(serde_json::Value::as_array_mut)
            .expect("generated SBOM component properties are an array");
        if !roots.contains(&reference) {
            let mut exposure = properties
                .iter_mut()
                .filter(|property| {
                    property.get("name").and_then(serde_json::Value::as_str)
                        == Some("yydra:exposure")
                })
                .collect::<Vec<_>>();
            if exposure.len() != 1 {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                    format!("{reference} final SBOM has no unique exposure property"),
                ));
            }
            exposure[0]["value"] = json!("shipped-linked");
        }
        let artifact_link_authority = match (
            source_linked.contains(&reference),
            additional_linked.contains(&reference),
        ) {
            (true, true) => "retained-production-source-map-and-gradle-artifact-producer",
            (false, true) => "retained-gradle-project-artifact-or-apk-native-producer",
            _ => "retained-production-source-map",
        };
        properties.push(json!({
            "name": "yydra:artifact-link-authority",
            "value": artifact_link_authority
        }));
    }
    let application_ref = sbom
        .pointer("/metadata/component/bom-ref")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("{target} SBOM metadata component has no bom-ref"),
            )
        })?
        .to_owned();
    let mut valid_refs = linked.clone();
    valid_refs.extend(other_refs);
    valid_refs.insert(application_ref);
    let dependencies = sbom
        .get_mut("dependencies")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("{target} SBOM has no dependency array"),
            )
        })?;
    dependencies.retain(|dependency| {
        dependency
            .get("ref")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|reference| valid_refs.contains(reference))
    });
    for dependency in dependencies {
        if let Some(depends_on) = dependency
            .get_mut("dependsOn")
            .and_then(serde_json::Value::as_array_mut)
        {
            depends_on.retain(|reference| {
                reference
                    .as_str()
                    .is_some_and(|reference| valid_refs.contains(reference))
            });
        }
    }
    let properties = sbom
        .pointer_mut("/metadata/properties")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("{target} SBOM metadata has no properties"),
            )
        })?;
    for source_map in source_maps {
        properties.push(json!({
            "name": "yydra:source-map-evidence",
            "value": format!("{} {}", source_map.sha256, source_map.path)
        }));
    }
    Ok(linked.into_iter().collect())
}

fn validate_final_sbom(target: &str, sbom: &serde_json::Value) -> SupplyResult<()> {
    if sbom.get("bomFormat").and_then(serde_json::Value::as_str) != Some("CycloneDX")
        || sbom.get("specVersion").and_then(serde_json::Value::as_str) != Some("1.6")
        || sbom
            .pointer("/metadata/component/name")
            .and_then(serde_json::Value::as_str)
            != Some(target)
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
            format!("{target} preliminary SBOM is not the expected CycloneDX 1.6 target document"),
        ));
    }
    let components = sbom
        .get("components")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
                format!("{target} preliminary SBOM has no components"),
            )
        })?;
    for component in components {
        let build_tool = component
            .get("properties")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|properties| {
                properties.iter().any(|property| {
                    property.get("name").and_then(serde_json::Value::as_str)
                        == Some("yydra:exposure")
                        && property.get("value").and_then(serde_json::Value::as_str)
                            == Some("build-tool-executed")
                })
            });
        if build_tool {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_BUILD_TOOL_SHIPPED",
                format!("{target} final SBOM includes a build-tool-only component"),
            ));
        }
    }
    Ok(())
}

fn collect_gradle_native_entries(
    component: &RawGradleMaterial,
    artifacts: &[GradleMaterialFile],
) -> SupplyResult<Vec<GradleNativeEntry>> {
    let mut native_entries = Vec::new();
    for (raw, recorded) in component.artifacts.iter().zip(artifacts) {
        if !raw.extension.eq_ignore_ascii_case("aar") {
            continue;
        }
        let bytes = read_bounded(
            Path::new(&raw.file),
            MAX_ARCHIVE_BYTES,
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
        )?;
        for entry in apk_entries(&bytes)? {
            let Some(apk_path) = gradle_native_apk_path(&entry.path) else {
                continue;
            };
            native_entries.push(GradleNativeEntry {
                artifact_path: recorded.path.clone(),
                entry_path: entry.path,
                apk_path,
                bytes: entry.bytes,
                sha256: entry.sha256,
            });
        }
    }
    native_entries.sort_by(|left, right| {
        (
            &left.apk_path,
            &left.sha256,
            &left.artifact_path,
            &left.entry_path,
        )
            .cmp(&(
                &right.apk_path,
                &right.sha256,
                &right.artifact_path,
                &right.entry_path,
            ))
    });
    let mut apk_paths = BTreeSet::new();
    if native_entries
        .iter()
        .any(|entry| !apk_paths.insert(entry.apk_path.clone()))
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_PROVENANCE_MISMATCH",
            "one Gradle material declares multiple native producers for the same APK path",
        ));
    }
    Ok(native_entries)
}

fn gradle_native_apk_path(path: &str) -> Option<String> {
    let parts = path.split('/').collect::<Vec<_>>();
    (parts.len() == 3
        && matches!(parts[0], "jni" | "libs")
        && !parts[1].is_empty()
        && parts[2].ends_with(".so"))
    .then(|| format!("lib/{}/{}", parts[1], parts[2]))
}

fn add_android_artifact_inventory(
    sbom: &mut serde_json::Value,
    apk_entries: &[ApkEntry],
    external: &[GradleRuntimeMaterial],
    local: &[GradleLocalMaterial],
    edges: &[GradleDependencyEdge],
) -> SupplyResult<()> {
    let malformed = || {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_RELEASE_INPUT_INVALID",
            "Android SBOM lacks its application, component or dependency identity",
        )
    };
    let application = sbom
        .pointer("/metadata/component/bom-ref")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(malformed)?
        .to_owned();
    let components = sbom
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(malformed)?;
    let mut native_refs = Vec::new();
    for entry in apk_entries
        .iter()
        .filter(|entry| is_native_apk_entry(&entry.path))
    {
        let reference = format!(
            "urn:yydra:apk-entry:{}",
            hex::encode(Sha256::digest(entry.path.as_bytes()))
        );
        native_refs.push(reference.clone());
        components.push(json!({
            "type": "file",
            "bom-ref": reference,
            "name": entry.path,
            "hashes": [{"alg": "SHA-256", "content": entry.sha256.strip_prefix("sha256:").unwrap_or(&entry.sha256)}],
            "properties": [
                {"name": "yydra:exposure", "value": "artifact-entry"},
                {"name": "yydra:apk-native-entry", "value": entry.path},
                {"name": "yydra:upstream-provenance", "value": "not-evaluated"},
                {"name": "yydra:license-review", "value": "not-evaluated"},
                {"name": "yydra:advisory-coverage", "value": "no-entry-level-query-or-upstream-attribution"}
            ]
        }));
    }
    for material in external {
        components.push(json!({
            "type": "library",
            "bom-ref": material.purl,
            "group": material.group,
            "name": material.name,
            "version": material.version,
            "purl": material.purl,
            "properties": [
                {"name": "yydra:exposure", "value": material.exposure},
                {"name": "yydra:inventory-basis", "value": "Gradle releaseRuntimeClasspath ResolutionResult"},
                {"name": "yydra:artifact-set-sha256", "value": material.source_integrity},
                {"name": "yydra:upstream-provenance", "value": "not-evaluated"},
                {"name": "yydra:license-review", "value": "not-evaluated"}
            ]
        }));
    }
    for material in local {
        components.push(json!({
            "type": "library",
            "bom-ref": gradle_project_ref(&material.project_path),
            "group": material.group,
            "name": material.name,
            "version": material.version,
            "properties": [
                {"name": "yydra:exposure", "value": material.exposure},
                {"name": "yydra:gradle-project", "value": material.project_path},
                {"name": "yydra:artifact-set-sha256", "value": material.artifact_set_sha256},
                {"name": "yydra:upstream-provenance", "value": "not-evaluated"},
                {"name": "yydra:license-review", "value": "not-evaluated"},
                {"name": "yydra:advisory-coverage", "value": "no-project-source-query-or-upstream-attribution"}
            ]
        }));
    }
    let dependencies = sbom
        .get_mut("dependencies")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(malformed)?;
    let mut outgoing = BTreeMap::<String, BTreeSet<String>>::new();
    for dependency in dependencies.iter() {
        let reference = dependency
            .get("ref")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(malformed)?;
        let children = dependency
            .get("dependsOn")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(malformed)?;
        let targets = outgoing.entry(reference.to_owned()).or_default();
        for child in children {
            targets.insert(child.as_str().ok_or_else(malformed)?.to_owned());
        }
    }
    let gradle_root = gradle_project_ref(":app");
    let map_root = |reference: &str| {
        if reference == gradle_root {
            application.clone()
        } else {
            reference.to_owned()
        }
    };
    for edge in edges {
        outgoing
            .entry(map_root(&edge.from))
            .or_default()
            .insert(map_root(&edge.to));
    }
    for reference in external.iter().map(|material| material.purl.clone()).chain(
        local
            .iter()
            .map(|material| gradle_project_ref(&material.project_path)),
    ) {
        outgoing.entry(reference).or_default();
    }
    for reference in native_refs {
        outgoing
            .entry(application.clone())
            .or_default()
            .insert(reference.clone());
        // Physical APK membership is known; no producer edge is inferred.
        outgoing.entry(reference).or_default();
    }
    *dependencies = outgoing
        .into_iter()
        .map(|(reference, children)| {
            json!({
                "ref": reference,
                "dependsOn": children.into_iter().collect::<Vec<_>>()
            })
        })
        .collect();
    Ok(())
}

fn parse_gradle_components(bytes: &[u8]) -> SupplyResult<Vec<GradleComponent>> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("Gradle release runtime graph is not UTF-8: {error}"),
        )
    })?;
    if text.lines().any(|line| {
        line.contains("--- ")
            && (line.contains(" FAILED")
                || line.contains(" -> FAILED")
                || line.contains(" (n)")
                || line.contains(" (not resolved)"))
    }) {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Gradle release runtime graph contains an unresolved dependency",
        ));
    }
    let mut components = BTreeMap::new();
    for line in text.lines() {
        let Some(marker) = line.find("--- ") else {
            continue;
        };
        let material = line[marker + 4..].trim();
        if material.starts_with("project ") || material.ends_with("(c)") {
            continue;
        }
        let (requested, selected) = material
            .split_once(" -> ")
            .map_or((material, None), |(left, right)| (left, Some(right)));
        let requested = requested
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches(['(', ')', ',']);
        let fields = requested.split(':').collect::<Vec<_>>();
        if fields.len() < 2 || fields[0].is_empty() || fields[1].is_empty() {
            continue;
        }
        let mut group = fields[0];
        let mut name = fields[1];
        let mut version = fields.get(2).copied().unwrap_or_default();
        if let Some(selected) = selected {
            let selected = selected
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches(['(', ')', ',']);
            if selected == "project" {
                continue;
            }
            let selected_fields = selected.split(':').collect::<Vec<_>>();
            if selected_fields.len() >= 3 {
                group = selected_fields[0];
                name = selected_fields[1];
                version = selected_fields[2];
            } else {
                version = selected;
            }
        }
        if version.is_empty()
            || version.contains('{')
            || version.contains('}')
            || version == "unspecified"
            || group.is_empty()
            || name.is_empty()
        {
            continue;
        }
        let component = GradleComponent {
            purl: format!("pkg:maven/{group}/{name}@{version}"),
            group: group.to_owned(),
            name: name.to_owned(),
            version: version.to_owned(),
        };
        components.insert(component.purl.clone(), component);
    }
    Ok(components.into_values().collect())
}

fn is_native_apk_entry(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() == 3 && parts[0] == "lib" && !parts[1].is_empty() && parts[2].ends_with(".so")
}

fn apk_entries(bytes: &[u8]) -> SupplyResult<Vec<ApkEntry>> {
    const EOCD: u32 = 0x0605_4b50;
    const CENTRAL: u32 = 0x0201_4b50;
    const LOCAL: u32 = 0x0403_4b50;
    let search_start = bytes.len().saturating_sub(65_557);
    let eocd = (search_start..bytes.len().saturating_sub(3))
        .rev()
        .find(|offset| read_u32(bytes, *offset).ok() == Some(EOCD))
        .ok_or_else(|| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "Android release artifact is not a supported ZIP/APK",
            )
        })?;
    let count = usize::from(read_u16(bytes, eocd + 10)?);
    let central_size = usize::try_from(read_u32(bytes, eocd + 12)?).unwrap_or(usize::MAX);
    let central_offset = usize::try_from(read_u32(bytes, eocd + 16)?).unwrap_or(usize::MAX);
    if count == usize::from(u16::MAX)
        || central_size == usize::try_from(u32::MAX).unwrap_or(usize::MAX)
        || central_offset == usize::try_from(u32::MAX).unwrap_or(usize::MAX)
        || central_offset
            .checked_add(central_size)
            .is_none_or(|end| end > bytes.len())
    {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Android APK uses an unsupported or out-of-range central directory",
        ));
    }
    let mut cursor = central_offset;
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut expanded_bytes = 0_u64;
    for _ in 0..count {
        if read_u32(bytes, cursor)? != CENTRAL {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "Android APK central directory entry is malformed",
            ));
        }
        let compression = read_u16(bytes, cursor + 10)?;
        let crc32 = read_u32(bytes, cursor + 16)?;
        let compressed_size = usize::try_from(read_u32(bytes, cursor + 20)?).unwrap_or(usize::MAX);
        let size = u64::from(read_u32(bytes, cursor + 24)?);
        let name_len = usize::from(read_u16(bytes, cursor + 28)?);
        let extra_len = usize::from(read_u16(bytes, cursor + 30)?);
        let comment_len = usize::from(read_u16(bytes, cursor + 32)?);
        let local_offset = usize::try_from(read_u32(bytes, cursor + 42)?).unwrap_or(usize::MAX);
        let name_start = cursor.checked_add(46).ok_or_else(apk_range_failure)?;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or_else(apk_range_failure)?;
        let name = std::str::from_utf8(
            bytes
                .get(name_start..name_end)
                .ok_or_else(apk_range_failure)?,
        )
        .map_err(|error| {
            SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("Android APK entry name is not UTF-8: {error}"),
            )
        })?
        .to_owned();
        let path = Path::new(&name);
        if name.contains('\\')
            || path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, PathComponent::Normal(_)))
        {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("Android APK contains unsafe entry path {name:?}"),
            ));
        }
        let next = name_end
            .checked_add(extra_len)
            .and_then(|value| value.checked_add(comment_len))
            .ok_or_else(apk_range_failure)?;
        if next > central_offset + central_size {
            return Err(apk_range_failure());
        }
        cursor = next;
        if name.ends_with('/') {
            continue;
        }
        expanded_bytes = expanded_bytes
            .checked_add(size)
            .ok_or_else(apk_range_failure)?;
        if expanded_bytes > MAX_RELEASE_TREE_BYTES {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "Android APK expanded entries exceed the 2 GiB evidence limit",
            ));
        }
        if !seen.insert(name.clone()) || read_u32(bytes, local_offset)? != LOCAL {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                format!("Android APK contains duplicate or malformed entry {name:?}"),
            ));
        }
        let local_name_len = usize::from(read_u16(bytes, local_offset + 26)?);
        let local_extra_len = usize::from(read_u16(bytes, local_offset + 28)?);
        let data_start = local_offset
            .checked_add(30)
            .and_then(|value| value.checked_add(local_name_len))
            .and_then(|value| value.checked_add(local_extra_len))
            .ok_or_else(apk_range_failure)?;
        let data_end = data_start
            .checked_add(compressed_size)
            .ok_or_else(apk_range_failure)?;
        let compressed = bytes
            .get(data_start..data_end)
            .ok_or_else(apk_range_failure)?;
        let sha256 = apk_entry_sha256(compressed, compression, size)?;
        entries.push(ApkEntry {
            path: name,
            crc32: format!("{crc32:08x}"),
            compressed_bytes: u64::try_from(compressed_size).unwrap_or(u64::MAX),
            bytes: size,
            compressed_sha256: sha256_identity(compressed),
            sha256,
        });
    }
    if cursor != central_offset + central_size || entries.is_empty() {
        return Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            "Android APK central directory is incomplete or empty",
        ));
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn apk_entry_sha256(
    compressed: &[u8],
    compression: u16,
    expected_bytes: u64,
) -> SupplyResult<String> {
    fn hash_exact(mut reader: impl Read, expected_bytes: u64) -> SupplyResult<String> {
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut bytes = 0_u64;
        loop {
            let read = reader.read(&mut buffer).map_err(|error| {
                SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    format!("decompress Android APK entry: {error}"),
                )
            })?;
            if read == 0 {
                break;
            }
            bytes = bytes
                .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
                .ok_or_else(apk_range_failure)?;
            if bytes > expected_bytes {
                return Err(SupplyChainFailure::new(
                    "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                    "Android APK entry expands beyond its declared size",
                ));
            }
            digest.update(&buffer[..read]);
        }
        if bytes != expected_bytes {
            return Err(SupplyChainFailure::new(
                "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
                "Android APK entry size differs from its central-directory identity",
            ));
        }
        Ok(format!("sha256:{}", hex::encode(digest.finalize())))
    }

    match compression {
        0 => hash_exact(compressed, expected_bytes),
        8 => hash_exact(DeflateDecoder::new(compressed), expected_bytes),
        method => Err(SupplyChainFailure::new(
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
            format!("Android APK entry uses unsupported compression method {method}"),
        )),
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> SupplyResult<u16> {
    let value: [u8; 2] = bytes
        .get(offset..offset.saturating_add(2))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(apk_range_failure)?;
    Ok(u16::from_le_bytes(value))
}

fn read_u32(bytes: &[u8], offset: usize) -> SupplyResult<u32> {
    let value: [u8; 4] = bytes
        .get(offset..offset.saturating_add(4))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(apk_range_failure)?;
    Ok(u32::from_le_bytes(value))
}

fn apk_range_failure() -> SupplyChainFailure {
    SupplyChainFailure::new(
        "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED",
        "Android APK contains an out-of-range ZIP record",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_gradle_repositories() -> Vec<serde_json::Value> {
        vec![
            json!({
                "scope": "project::app",
                "name": "Google",
                "kind": "maven",
                "url": "https://dl.google.com/dl/android/maven2/"
            }),
            json!({
                "scope": "project::app",
                "name": "MavenRepo",
                "kind": "maven",
                "url": "https://repo.maven.apache.org/maven2/"
            }),
            json!({
                "scope": "settings:pluginManagement",
                "name": "Gradle Central Plugin Repository",
                "kind": "maven",
                "url": "https://plugins.gradle.org/m2"
            }),
        ]
    }

    fn test_gradle_bundle_binding(android: &Path) -> serde_json::Value {
        let bundle = android.join("app/build/generated/assets/react/release/index.android.bundle");
        let source_map =
            android.join("app/build/generated/sourcemaps/react/release/index.android.bundle.map");
        fs::create_dir_all(bundle.parent().expect("bundle parent"))
            .expect("create bundle directory");
        fs::create_dir_all(source_map.parent().expect("source-map parent"))
            .expect("create source-map directory");
        fs::write(&bundle, b"test bundle").expect("write test bundle");
        fs::write(
            &source_map,
            br#"{"version":3,"file":"index.android.bundle","sources":["node_modules/react/index.js"],"names":[],"mappings":""}"#,
        )
        .expect("write test source map");
        json!({
            "taskPath": ":app:createBundleReleaseJsAndAssets",
            "bundleFile": bundle,
            "sourceMapFile": source_map,
        })
    }

    fn test_gradle_edge(from: &str, to: &str) -> serde_json::Value {
        json!({
            "from": from,
            "to": to,
            "selectedVariant": "releaseRuntimeElements",
            "selectedVariantAttributes": {
                "org.gradle.usage": "java-runtime"
            }
        })
    }

    fn test_retained_gradle_bundle_binding() -> serde_json::Value {
        json!({
            "taskPath": ":app:createBundleReleaseJsAndAssets",
            "bundlePath": "app/build/generated/assets/react/release/index.android.bundle",
            "bundleBytes": 11,
            "bundleSha256": sha256_identity(b"test bundle"),
            "sourceMapPath": "app/build/generated/sourcemaps/react/release/index.android.bundle.map",
            "sourceMapBytes": 112,
            "sourceMapSha256": sha256_identity(br#"{"version":3,"file":"index.android.bundle","sources":["node_modules/react/index.js"],"names":[],"mappings":""}"#),
        })
    }

    #[test]
    fn civil_date_conversion_matches_epoch_and_leap_boundaries() {
        assert_eq!(civil_from_days(0), "1970-01-01");
        assert_eq!(civil_from_days(19_782), "2024-02-29");
        assert_eq!(civil_from_days(20_699), "2026-09-03");
    }

    #[test]
    fn npm_dependency_resolution_uses_the_nearest_installed_package() {
        let packages = BTreeMap::from([
            ("node_modules/a".to_owned(), empty_npm_package()),
            ("node_modules/x".to_owned(), empty_npm_package()),
            (
                "node_modules/a/node_modules/x".to_owned(),
                empty_npm_package(),
            ),
        ]);
        assert_eq!(
            resolve_npm_path("node_modules/a", "x", &packages).as_deref(),
            Some("node_modules/a/node_modules/x")
        );
        assert_eq!(
            resolve_npm_path("node_modules/a", "missing", &packages),
            None
        );
    }

    #[test]
    fn target_sboms_keep_component_facts_and_edges_target_local() {
        let component = |target: &str, exposure: &str, feature: &str| Component {
            bom_ref: "pkg:cargo/shared@1.0.0".to_owned(),
            ecosystem: "cargo".to_owned(),
            name: "shared".to_owned(),
            version: "1.0.0".to_owned(),
            features: vec![feature.to_owned()],
            dependency_kinds: vec!["normal".to_owned()],
            targets: vec![target.to_owned()],
            source: "registry+https://github.com/rust-lang/crates.io-index".to_owned(),
            source_checksum: Some(format!("sha256:{}", "a".repeat(64))),
            declared_license: Some("MIT".to_owned()),
            detected_license: "MIT".to_owned(),
            notices: Vec::new(),
            exposure: exposure.to_owned(),
            install_paths: vec![format!("cargo/{target}/shared-1.0.0")],
            metadata_observations: Vec::new(),
            provenance: BTreeMap::from([(
                format!("{target}.authority"),
                format!("{target}-fixture"),
            )]),
        };
        let root_component = |target: &str| Component {
            bom_ref: format!("pkg:cargo/{target}-root@1.0.0"),
            ecosystem: "cargo".to_owned(),
            name: format!("{target}-root"),
            version: "1.0.0".to_owned(),
            features: Vec::new(),
            dependency_kinds: vec!["root".to_owned()],
            targets: vec![target.to_owned()],
            source: "workspace-path".to_owned(),
            source_checksum: None,
            declared_license: Some("MIT".to_owned()),
            detected_license: "MIT".to_owned(),
            notices: Vec::new(),
            exposure: "shipped-linked".to_owned(),
            install_paths: vec![format!("crates/{target}-root")],
            metadata_observations: Vec::new(),
            provenance: BTreeMap::new(),
        };
        let mut components = vec![
            component("cli", "build-tool-executed", "cli-feature"),
            component("server", "shipped-linked", "server-feature"),
            root_component("cli"),
            root_component("server"),
        ];
        merge_target_components(&mut components).expect("merge only within exact target scopes");
        assert_eq!(
            components
                .iter()
                .filter(|component| component.bom_ref == "pkg:cargo/shared@1.0.0")
                .count(),
            2
        );
        let dependencies = vec![
            DependencyEdge {
                from: "pkg:cargo/cli-root@1.0.0".to_owned(),
                to: "pkg:cargo/shared@1.0.0".to_owned(),
                kinds: vec!["build".to_owned()],
                targets: vec!["cli".to_owned()],
                conditions: vec!["cfg(unix)".to_owned()],
            },
            DependencyEdge {
                from: "pkg:cargo/server-root@1.0.0".to_owned(),
                to: "pkg:cargo/shared@1.0.0".to_owned(),
                kinds: vec!["normal".to_owned()],
                targets: vec!["server".to_owned()],
                conditions: Vec::new(),
            },
        ];
        let sandbox = tempfile::tempdir().expect("create SBOM sandbox");
        let cli_path = sandbox.path().join("cli.cdx.json");
        let server_path = sandbox.path().join("server.cdx.json");
        write_cyclonedx(&cli_path, "cli", &components, &dependencies)
            .expect("write CLI target SBOM");
        write_cyclonedx(&server_path, "server", &components, &dependencies)
            .expect("write server target SBOM");
        let cli: serde_json::Value =
            serde_json::from_slice(&fs::read(cli_path).expect("read CLI SBOM"))
                .expect("parse CLI SBOM");
        let server: serde_json::Value =
            serde_json::from_slice(&fs::read(server_path).expect("read server SBOM"))
                .expect("parse server SBOM");
        assert!(
            !cli["components"]
                .as_array()
                .expect("CLI components")
                .iter()
                .any(|component| component["bom-ref"] == "pkg:cargo/shared@1.0.0")
        );
        let shared = server["components"]
            .as_array()
            .expect("server components")
            .iter()
            .find(|component| component["bom-ref"] == "pkg:cargo/shared@1.0.0")
            .expect("server keeps shipped shared component");
        assert!(shared.to_string().contains("server-feature"));
        assert!(!shared.to_string().contains("cli-feature"));
        let server_root = server["dependencies"]
            .as_array()
            .expect("server dependencies")
            .iter()
            .find(|dependency| dependency["ref"] == "pkg:cargo/server-root@1.0.0")
            .expect("server root dependency entry");
        assert_eq!(server_root["dependsOn"], json!(["pkg:cargo/shared@1.0.0"]));
        assert!(!server.to_string().contains("pkg:cargo/cli-root@1.0.0"));
    }

    #[test]
    fn gradle_archive_entries_remain_bound_to_their_unsorted_raw_artifacts() {
        let sandbox = tempfile::tempdir().expect("create Gradle artifact sandbox");
        let frontend = sandbox.path().join("frontend");
        let android = frontend.join("android");
        let project = frontend.join("node_modules/native-package/android");
        let first = project.join("libs/z-first.aar");
        let second = project.join("libs/a-second.aar");
        fs::create_dir_all(first.parent().expect("artifact parent"))
            .expect("create artifact directory");
        fs::create_dir_all(&android).expect("create generated Android directory");
        fs::write(
            &first,
            stored_zip("jni/x86_64/libfirst.so", b"first-native"),
        )
        .expect("write first AAR");
        fs::write(
            &second,
            stored_zip("jni/arm64-v8a/libsecond.so", b"second-native"),
        )
        .expect("write second AAR");
        let component = RawGradleMaterial {
            component_type: "project".to_owned(),
            group: "workspace-project".to_owned(),
            name: "native-package".to_owned(),
            version: "workspace".to_owned(),
            project_path: Some(":native-package".to_owned()),
            source_directory: Some(project.display().to_string()),
            artifacts: vec![
                RawGradleArtifact {
                    file: first.display().to_string(),
                    extension: "aar".to_owned(),
                    classifier: String::new(),
                },
                RawGradleArtifact {
                    file: second.display().to_string(),
                    extension: "aar".to_owned(),
                    classifier: String::new(),
                },
            ],
        };
        let artifacts = record_gradle_material_files(
            &component,
            &sandbox.path().join("gradle-home"),
            &android,
            false,
        )
        .expect("record artifacts without reordering their raw association");
        let native_entries = collect_gradle_native_entries(&component, &artifacts)
            .expect("collect native entries from paired artifacts");
        let first_entry = native_entries
            .iter()
            .find(|entry| entry.entry_path.ends_with("libfirst.so"))
            .expect("first native entry");
        let second_entry = native_entries
            .iter()
            .find(|entry| entry.entry_path.ends_with("libsecond.so"))
            .expect("second native entry");
        assert!(first_entry.artifact_path.ends_with("/z-first.aar"));
        assert!(second_entry.artifact_path.ends_with("/a-second.aar"));
        assert!(
            first_entry
                .artifact_path
                .contains(&artifacts[0].sha256[7..])
        );
        assert!(
            second_entry
                .artifact_path
                .contains(&artifacts[1].sha256[7..])
        );
    }

    #[test]
    fn final_npm_sbom_is_filtered_by_retained_source_map_inputs() {
        let root_ref = "pkg:npm/product@0.1.0";
        let react_ref = "pkg:npm/react@19.2.3";
        let build_ref = "pkg:npm/lightningcss@1.33.0";
        let inventory = json!({
            "components": [
                {
                    "bomRef": root_ref,
                    "ecosystem": "npm",
                    "targets": ["android", "h5"],
                    "exposure": "product-root",
                    "installPaths": ["frontend"]
                },
                {
                    "bomRef": react_ref,
                    "ecosystem": "npm",
                    "targets": ["android", "h5"],
                    "exposure": "shipped-candidate",
                    "installPaths": ["frontend/node_modules/react"]
                },
                {
                    "bomRef": build_ref,
                    "ecosystem": "npm",
                    "targets": ["android", "h5"],
                    "exposure": "build-tool-executed",
                    "installPaths": ["frontend/node_modules/lightningcss"]
                }
            ]
        });
        let application_ref = "pkg:generic/yydra-h5@0.1.0";
        let component = |reference: &str| {
            json!({
                "bom-ref": reference,
                "properties": [{"name": "yydra:exposure", "value": "shipped-candidate"}]
            })
        };
        let mut sbom = json!({
            "metadata": {
                "component": {"bom-ref": application_ref},
                "properties": []
            },
            "components": [component(root_ref), component(react_ref)],
            "dependencies": [
                {"ref": application_ref, "dependsOn": [root_ref, react_ref]},
                {"ref": root_ref, "dependsOn": [react_ref]},
                {"ref": react_ref, "dependsOn": []},
            ]
        });
        let mut build_tool_sbom = sbom.clone();
        build_tool_sbom["components"]
            .as_array_mut()
            .expect("component array")
            .push(component(build_ref));
        let build_tool_linked = link_npm_sbom_to_source_maps(
            "h5",
            &mut build_tool_sbom,
            &inventory,
            &BTreeSet::from(["../../node_modules/lightningcss/index.js".to_owned()]),
            &[ReleaseFile {
                path: "artifacts/h5.real-runtime/dist/_expo/static/js/app.js.map".to_owned(),
                bytes: 42,
                sha256: format!("sha256:{}", "0".repeat(64)),
            }],
            &BTreeSet::new(),
        )
        .expect("source-map-linked build tools become actual shipped inputs");
        assert_eq!(
            build_tool_linked,
            vec![build_ref.to_owned(), root_ref.to_owned()]
        );
        assert!(build_tool_sbom.to_string().contains("shipped-linked"));

        let mut native_sbom = sbom.clone();
        native_sbom["components"]
            .as_array_mut()
            .expect("component array")
            .push(component(build_ref));
        let native_linked = link_npm_sbom_to_source_maps(
            "android",
            &mut native_sbom,
            &inventory,
            &BTreeSet::from(["../../node_modules/react/index.js".to_owned()]),
            &[ReleaseFile {
                path: "artifacts/android.release/index.android.bundle.map".to_owned(),
                bytes: 42,
                sha256: format!("sha256:{}", "a".repeat(64)),
            }],
            &BTreeSet::from([build_ref.to_owned()]),
        )
        .expect("retain an exact npm-backed native Gradle producer beside source-map inputs");
        assert_eq!(
            native_linked,
            vec![
                build_ref.to_owned(),
                root_ref.to_owned(),
                react_ref.to_owned()
            ]
        );
        assert!(
            native_sbom
                .to_string()
                .contains("retained-gradle-project-artifact-or-apk-native-producer")
        );

        let linked = link_npm_sbom_to_source_maps(
            "h5",
            &mut sbom,
            &inventory,
            &BTreeSet::from(["../../node_modules/react/index.js".to_owned()]),
            &[ReleaseFile {
                path: "artifacts/h5.real-runtime/dist/_expo/static/js/app.js.map".to_owned(),
                bytes: 42,
                sha256: format!("sha256:{}", "1".repeat(64)),
            }],
            &BTreeSet::new(),
        )
        .expect("link exact source-map input");
        assert_eq!(linked, vec![root_ref.to_owned(), react_ref.to_owned()]);
        let retained = sbom["components"]
            .as_array()
            .expect("component array")
            .iter()
            .filter_map(|component| component["bom-ref"].as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(retained, BTreeSet::from([root_ref, react_ref]));
        assert!(!sbom.to_string().contains(build_ref));
        let react_exposure = sbom["components"]
            .as_array()
            .expect("component array")
            .iter()
            .find(|component| component["bom-ref"] == react_ref)
            .and_then(|component| component["properties"].as_array())
            .and_then(|properties| {
                properties
                    .iter()
                    .find(|property| property["name"] == "yydra:exposure")
            })
            .and_then(|property| property["value"].as_str());
        assert_eq!(react_exposure, Some("shipped-linked"));

        let failure = link_npm_sbom_to_source_maps(
            "h5",
            &mut sbom,
            &inventory,
            &BTreeSet::from(["../../node_modules/unlocked/index.js".to_owned()]),
            &[ReleaseFile {
                path: "map".to_owned(),
                bytes: 1,
                sha256: format!("sha256:{}", "2".repeat(64)),
            }],
            &BTreeSet::new(),
        )
        .expect_err("unlocked source-map input must fail closed");
        assert_eq!(failure.code, "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED");
    }

    #[test]
    fn vulnerability_exceptions_require_unique_exact_current_bounds() {
        let policy_source =
            include_str!("../template/product-workspace/.yydra/supply-chain-policy.json")
                .replace("__YYDRA_DISTRIBUTION_VERSION__", DISTRIBUTION_VERSION);
        let exceptions_source =
            include_str!("../template/product-workspace/.yydra/supply-chain-exceptions.json")
                .replace("__YYDRA_DISTRIBUTION_VERSION__", DISTRIBUTION_VERSION);
        let policy: SupplyChainPolicy =
            serde_json::from_str(&policy_source).expect("parse policy fixture");
        let mut exceptions: SupplyChainExceptions =
            serde_json::from_str(&exceptions_source).expect("parse exception fixture");
        let exact = VulnerabilityException {
            advisory_id: "OSV-TEST-1".to_owned(),
            purl: "pkg:npm/react@19.2.3".to_owned(),
            version: "19.2.3".to_owned(),
            target: "h5".to_owned(),
            impact_analysis:
                "The affected path is not reachable in this exact H5 Application Surface artifact."
                    .to_owned(),
            owner: "Yydra release owner".to_owned(),
            evidence: "https://github.com/yydcnjjw/yydra/issues/39".to_owned(),
            approved_by: "yydcnjjw".to_owned(),
            approved_at: "2026-09-03T00:00:00Z".to_owned(),
            expires_at: "2999-01-01T00:00:00Z".to_owned(),
            re_review_trigger: "Any advisory, version, target, or artifact change".to_owned(),
        };
        exceptions.vulnerability_exceptions.push(exact.clone());
        validate_policy(&policy, &exceptions).expect("accept exact current exception");

        exceptions.vulnerability_exceptions.push(exact.clone());
        assert_eq!(
            validate_policy(&policy, &exceptions)
                .expect_err("reject duplicate exception")
                .code,
            "SUPPLY_CHAIN_EXCEPTION_OVERBROAD"
        );
        exceptions.vulnerability_exceptions.pop();
        exceptions.vulnerability_exceptions[0].approved_at = "2025-01-01T00:00:00Z".to_owned();
        exceptions.vulnerability_exceptions[0].expires_at = "2026-01-01T00:00:00Z".to_owned();
        assert_eq!(
            validate_policy(&policy, &exceptions)
                .expect_err("reject stale exception")
                .code,
            "SUPPLY_CHAIN_EXCEPTION_STALE"
        );
        exceptions.vulnerability_exceptions[0] = exact;
        exceptions.vulnerability_exceptions[0].target = "all".to_owned();
        assert_eq!(
            validate_policy(&policy, &exceptions)
                .expect_err("reject broad target")
                .code,
            "SUPPLY_CHAIN_EXCEPTION_OVERBROAD"
        );
    }

    #[test]
    fn utc_exception_timestamps_reject_invalid_calendar_values() {
        assert_eq!(
            utc_timestamp_seconds("2024-02-29T23:59:59Z").expect("valid leap timestamp"),
            1_709_251_199
        );
        assert_eq!(
            utc_timestamp_seconds("2023-02-29T00:00:00Z")
                .expect_err("reject invalid leap day")
                .code,
            "SUPPLY_CHAIN_EXCEPTION_OVERBROAD"
        );
    }

    #[test]
    fn apk_inventory_identifies_native_entries_and_rejects_non_archives() {
        let archive = stored_zip("lib/x86_64/libreader.so", b"native-bytes");
        let entries = apk_entries(&archive).expect("parse minimal APK fixture");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "lib/x86_64/libreader.so");
        assert_eq!(entries[0].bytes, 12);
        assert_eq!(entries[0].compressed_bytes, 12);
        assert_eq!(entries[0].sha256, sha256_identity(b"native-bytes"));
        assert!(is_native_apk_entry(&entries[0].path));
        let mut sbom = json!({
            "metadata": {"component": {"bom-ref": "pkg:generic/android@0.1.0"}},
            "components": [],
            "dependencies": []
        });
        add_android_artifact_inventory(&mut sbom, &entries, &[], &[], &[])
            .expect("native entries need no invented upstream producer");
        let native = &sbom["components"][0];
        assert_eq!(native["type"], "file");
        assert_eq!(native["name"], "lib/x86_64/libreader.so");
        assert!(native.get("licenses").is_none());
        assert!(native.get("purl").is_none());
        assert!(!native.to_string().contains("yydra:native-producer"));
        assert!(
            native
                .to_string()
                .contains("no-entry-level-query-or-upstream-attribution")
        );
        let native_ref = native["bom-ref"].clone();
        let dependencies = sbom["dependencies"].as_array().unwrap();
        assert_eq!(
            dependencies
                .iter()
                .find(|edge| edge["ref"] == native_ref)
                .unwrap()["dependsOn"],
            json!([])
        );
        assert_eq!(
            dependencies
                .iter()
                .find(|edge| edge["ref"] == "pkg:generic/android@0.1.0")
                .unwrap()["dependsOn"],
            json!([native_ref])
        );
        let maven = GradleRuntimeMaterial {
            purl: "pkg:maven/com.example/platform@1.0.0".to_owned(),
            group: "com.example".to_owned(),
            name: "platform".to_owned(),
            version: "1.0.0".to_owned(),
            exposure: "dependency-graph-only".to_owned(),
            source: "gradle:releaseRuntimeClasspath".to_owned(),
            source_integrity: sha256_identity(b""),
            declared_license: None,
            detected_license: "not-evaluated".to_owned(),
            artifacts: Vec::new(),
            metadata: Vec::new(),
            notices: Vec::new(),
            native_entries: Vec::new(),
            provenance: BTreeMap::new(),
        };
        let local = GradleLocalMaterial {
            project_path: ":java-only".to_owned(),
            group: "workspace-project".to_owned(),
            name: "java-only".to_owned(),
            version: "workspace".to_owned(),
            exposure: "dependency-graph-only".to_owned(),
            source_path: "not-recorded".to_owned(),
            source_authority: "not-evaluated".to_owned(),
            artifact_set_sha256: sha256_identity(b""),
            artifacts: Vec::new(),
            native_entries: Vec::new(),
        };
        let edges = [
            serde_json::from_value(test_gradle_edge(
                "urn:yydra:gradle-project::app",
                &maven.purl,
            ))
            .unwrap(),
            serde_json::from_value(test_gradle_edge(
                &maven.purl,
                "urn:yydra:gradle-project::java-only",
            ))
            .unwrap(),
        ];
        add_android_artifact_inventory(&mut sbom, &[], &[maven], &[local], &edges)
            .expect("retain Gradle graph topology without inventing an npm source binding");
        let dependencies = sbom["dependencies"].as_array().unwrap();
        assert_eq!(
            dependencies
                .iter()
                .find(|edge| edge["ref"] == "pkg:maven/com.example/platform@1.0.0")
                .unwrap()["dependsOn"],
            json!(["urn:yydra:gradle-project::java-only"])
        );
        let root_edges = dependencies
            .iter()
            .find(|edge| edge["ref"] == "pkg:generic/android@0.1.0")
            .unwrap()["dependsOn"]
            .as_array()
            .unwrap();
        assert!(root_edges.contains(&json!("pkg:maven/com.example/platform@1.0.0")));
        assert!(!root_edges.contains(&json!("urn:yydra:gradle-project::java-only")));
        assert_eq!(
            apk_entries(b"not a ZIP")
                .expect_err("reject malformed APK")
                .code,
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED"
        );
    }

    #[test]
    fn h5_source_maps_require_two_sided_bundle_binding() {
        let sandbox = tempfile::tempdir().expect("create sandbox");
        let target_root = sandbox.path().join("h5");
        let artifact = target_root.join("artifact/dist/assets/app.js");
        let source_map = artifact.with_extension("js.map");
        fs::create_dir_all(artifact.parent().expect("artifact parent"))
            .expect("create artifact tree");
        fs::write(
            &artifact,
            b"console.log('ok');\n//# sourceMappingURL=app.js.map\n",
        )
        .expect("write bundle");
        fs::write(
            &source_map,
            br#"{"version":3,"file":"app.js","sources":["node_modules/react/index.js"],"names":[],"mappings":""}"#,
        )
        .expect("write source map");
        let bindings =
            validate_h5_source_map_bindings(&target_root, std::slice::from_ref(&source_map))
                .expect("accept matching production bundle and source map");
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].bundle_sha256,
            sha256_identity(b"console.log('ok');\n//# sourceMappingURL=app.js.map\n")
        );
        assert_eq!(
            bindings[0].source_map_sha256,
            sha256_identity(
                br#"{"version":3,"file":"app.js","sources":["node_modules/react/index.js"],"names":[],"mappings":""}"#
            )
        );

        fs::write(&artifact, b"console.log('unbound');\n").expect("replace bundle");
        assert_eq!(
            validate_h5_source_map_bindings(&target_root, &[source_map])
                .expect_err("reject an arbitrary source map beside a production bundle")
                .code,
            "SUPPLY_CHAIN_PROVENANCE_MISMATCH"
        );
    }

    #[test]
    fn gradle_runtime_graph_records_selected_exact_versions() {
        let graph = b"releaseRuntimeClasspath - Runtime classpath\n+--- androidx.core:core:1.15.0\n|    +--- com.squareup.okio:okio:3.9.0 -> 3.10.2 (*)\n|    +--- com.facebook.react:react-android -> 0.86.3 (*)\n|    +--- constraints.only:bom:4.0.0 (c)\n|    +--- old.example:old-name:1.0.0 -> new.example:new-name:2.0.0\n|    \\--- old.example:local-name:1.0.0 -> project :local-name\n\\--- project :expo-modules-core\n";
        assert_eq!(
            parse_gradle_components(graph).expect("parse Gradle graph"),
            vec![
                GradleComponent {
                    purl: "pkg:maven/androidx.core/core@1.15.0".to_owned(),
                    group: "androidx.core".to_owned(),
                    name: "core".to_owned(),
                    version: "1.15.0".to_owned(),
                },
                GradleComponent {
                    purl: "pkg:maven/com.facebook.react/react-android@0.86.3".to_owned(),
                    group: "com.facebook.react".to_owned(),
                    name: "react-android".to_owned(),
                    version: "0.86.3".to_owned(),
                },
                GradleComponent {
                    purl: "pkg:maven/com.squareup.okio/okio@3.10.2".to_owned(),
                    group: "com.squareup.okio".to_owned(),
                    name: "okio".to_owned(),
                    version: "3.10.2".to_owned(),
                },
                GradleComponent {
                    purl: "pkg:maven/new.example/new-name@2.0.0".to_owned(),
                    group: "new.example".to_owned(),
                    name: "new-name".to_owned(),
                    version: "2.0.0".to_owned(),
                },
            ]
        );
        assert_eq!(
            parse_gradle_components(
                b"releaseRuntimeClasspath - Runtime classpath\n\\--- com.example:missing:1.0.0 FAILED\n"
            )
            .expect_err("reject an incompletely resolved Gradle graph")
            .code,
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED"
        );
    }

    #[test]
    fn gradle_runtime_materials_record_artifacts_without_license_review() {
        let sandbox = tempfile::tempdir().expect("create sandbox");
        let android = sandbox.path().join("android");
        let gradle_home = sandbox.path().join("gradle-home");
        let isolated_maven_local = sandbox.path().join("home/.m2/repository");
        let coordinate = gradle_home.join("caches/modules-2/files-2.1/com.example/native/1.0.0");
        let artifact_dir = coordinate.join("artifact-hash");
        let pom_dir = coordinate.join("pom-hash");
        fs::create_dir_all(&android).expect("create Android root");
        fs::create_dir_all(&isolated_maven_local)
            .expect("create isolated empty Maven local repository");
        fs::create_dir_all(&artifact_dir).expect("create artifact directory");
        fs::create_dir_all(&pom_dir).expect("create POM directory");
        let artifact = artifact_dir.join("native-1.0.0.aar");
        fs::write(
            &artifact,
            stored_zip("jni/x86_64/libnative.so", b"native-bytes"),
        )
        .expect("write archive fixture");
        fs::write(
            pom_dir.join("native-1.0.0.pom"),
            br#"<project><licenses><license><name>MIT License</name><url>https://opensource.org/license/mit</url></license></licenses></project>"#,
        )
        .expect("write POM fixture");
        let local_source = sandbox.path().join("node_modules/native-project/android");
        let local_artifact = local_source.join("build/outputs/native-project.aar");
        fs::create_dir_all(local_artifact.parent().expect("local artifact parent"))
            .expect("create local Gradle project output");
        fs::write(
            &local_artifact,
            stored_zip("jni/arm64-v8a/liblocal.so", b"local-native-bytes"),
        )
        .expect("write local project AAR");
        let raw = sandbox.path().join("raw.json");
        let mut repositories = test_gradle_repositories();
        repositories.push(json!({
            "scope": "settings:dependencyResolutionManagement",
            "name": "MavenLocal",
            "kind": "maven",
            "url": format!("file:{}", isolated_maven_local.display())
        }));
        let legacy_repository = sandbox.path().join("node_modules/react-native/android");
        fs::create_dir_all(legacy_repository.parent().expect("React Native package"))
            .expect("create React Native package without the retired Maven directory");
        repositories.push(json!({
            "scope": "project::react-native-safe-area-context",
            "name": "legacy-react-native",
            "kind": "maven",
            "url": format!("file:{}", legacy_repository.display())
        }));
        repositories.push(json!({
            "scope": "project::react-native-masked-view_masked-view",
            "name": "JitPack",
            "kind": "maven",
            "url": "https://jitpack.io"
        }));
        fs::write(
            &raw,
            serde_json::to_vec(&json!({
                "schemaVersion": 3,
                "configuration": "releaseRuntimeClasspath",
                "bundleBinding": test_gradle_bundle_binding(&android),
                "repositories": repositories,
                "components": [
                    {
                        "componentType": "module",
                        "group": "com.example",
                        "name": "native",
                        "version": "1.0.0",
                        "projectPath": null,
                        "sourceDirectory": null,
                        "artifacts": [{
                            "file": artifact,
                            "extension": "aar",
                            "classifier": ""
                        }]
                    },
                    {
                        "componentType": "project",
                        "group": "workspace-project",
                        "name": "native-project",
                        "version": "workspace",
                        "projectPath": ":native-project",
                        "sourceDirectory": local_source,
                        "artifacts": [{
                            "file": local_artifact,
                            "extension": "aar",
                            "classifier": ""
                        }]
                    }
                ],
                "dependencies": [
                    test_gradle_edge(
                        "urn:yydra:gradle-project::app",
                        "pkg:maven/com.example/native@1.0.0"
                    ),
                    test_gradle_edge(
                        "pkg:maven/com.example/native@1.0.0",
                        "urn:yydra:gradle-project::native-project"
                    )
                ]
            }))
            .expect("encode raw manifest"),
        )
        .expect("write raw manifest");
        let output = sandbox.path().join("evidence/gradle-materials.json");
        create_private_dir_all(output.parent().expect("output parent"))
            .expect("create evidence root");

        let mut repeated: serde_json::Value =
            serde_json::from_slice(&fs::read(&raw).expect("read raw manifest"))
                .expect("decode raw manifest");
        let first_artifact = repeated["components"][0]["artifacts"][0].clone();
        repeated["components"][0]["artifacts"]
            .as_array_mut()
            .expect("artifact array")
            .push(first_artifact);
        fs::write(
            &raw,
            serde_json::to_vec(&repeated).expect("encode repeated artifact"),
        )
        .expect("write repeated artifact selected by multiple variants");
        record_android_gradle_materials(&android, &gradle_home, &raw, &output)
            .expect("record exact Gradle material");
        let inventory: serde_json::Value =
            serde_json::from_slice(&fs::read(&output).expect("read Gradle material inventory"))
                .expect("parse Gradle material inventory");
        let component = &inventory["components"][0];
        assert_eq!(
            component["artifacts"].as_array().expect("artifacts").len(),
            1
        );
        assert_eq!(
            component["nativeEntries"]
                .as_array()
                .expect("native entries")
                .len(),
            1
        );
        assert_eq!(component["purl"], "pkg:maven/com.example/native@1.0.0");
        assert!(component["declaredLicense"].is_null());
        assert_eq!(component["detectedLicense"], "not-evaluated");
        assert_eq!(inventory["repositories"], json!([]));
        assert!(
            component["provenance"]
                .get("configuredRepositories")
                .is_none()
        );
        assert!(
            component["sourceIntegrity"]
                .as_str()
                .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
        );
        assert!(
            component["notices"]
                .as_array()
                .is_some_and(|notices| notices.is_empty())
        );
        assert!(component["provenance"].is_object());
        assert_eq!(
            component["nativeEntries"][0]["apkPath"],
            "lib/x86_64/libnative.so"
        );
        assert_eq!(
            component["nativeEntries"][0]["sha256"],
            sha256_identity(b"native-bytes")
        );
        let local_component = &inventory["localComponents"][0];
        assert_eq!(local_component["projectPath"], ":native-project");
        assert_eq!(
            local_component["sourcePath"],
            "frontend/node_modules/native-project/android"
        );
        assert_eq!(local_component["sourceAuthority"], "not-evaluated");
        assert_eq!(
            local_component["nativeEntries"][0]["apkPath"],
            "lib/arm64-v8a/liblocal.so"
        );
        assert_eq!(
            inventory["dependencies"][0]["from"],
            "pkg:maven/com.example/native@1.0.0"
        );
        assert_eq!(
            inventory["dependencies"][0]["selectedVariantAttributes"]["org.gradle.usage"],
            "java-runtime"
        );
        repeated["components"][0]["artifacts"][1]["classifier"] = json!("conflicting");
        fs::write(
            &raw,
            serde_json::to_vec(&repeated).expect("encode conflicting artifact"),
        )
        .expect("write conflicting artifact identity");
        assert_eq!(
            record_android_gradle_materials(&android, &gradle_home, &raw, &output)
                .expect_err("the same path with inconsistent classifier must fail closed")
                .code,
            "SUPPLY_CHAIN_ARTIFACT_INVENTORY_FAILED"
        );
        fs::create_dir(&legacy_repository).expect("make retired repository active");
        repeated["components"][0]["artifacts"]
            .as_array_mut()
            .unwrap()
            .pop();
        fs::write(&raw, serde_json::to_vec(&repeated).unwrap()).unwrap();
        record_android_gradle_materials(
            &android,
            &gradle_home,
            &raw,
            &sandbox.path().join("with-legacy-repository.json"),
        )
        .expect("repository presence is not source admission");
    }

    #[test]
    fn gradle_runtime_materials_bind_npm_locked_local_maven_artifacts() {
        let sandbox = tempfile::tempdir().expect("create sandbox");
        let frontend = sandbox.path().join("frontend");
        let android = frontend.join("android");
        let gradle_home = sandbox.path().join("gradle-home");
        let coordinate =
            frontend.join("node_modules/expo-local/local-maven-repo/com/example/local/1.0.0");
        fs::create_dir_all(&android).expect("create Android root");
        fs::create_dir_all(&gradle_home).expect("create Gradle home");
        fs::create_dir_all(&coordinate).expect("create local Maven coordinate");
        let artifact = coordinate.join("local-1.0.0.aar");
        fs::write(
            &artifact,
            stored_zip("META-INF/LICENSE", include_bytes!("../../../LICENSE-MIT")),
        )
        .expect("write local Maven artifact");
        fs::write(
            coordinate.join("local-1.0.0.pom"),
            br#"<project><licenses><license><name>MIT License</name><url>https://opensource.org/license/mit</url></license></licenses></project>"#,
        )
        .expect("write local Maven POM");
        let raw = sandbox.path().join("raw.json");
        let mut repositories = test_gradle_repositories();
        repositories.push(json!({
            "scope": "project::app",
            "name": "local",
            "kind": "maven",
            "url": format!(
                "file://{}",
                frontend.join("node_modules/expo-local/local-maven-repo").display()
            )
        }));
        fs::write(
            &raw,
            serde_json::to_vec(&json!({
                "schemaVersion": 3,
                "configuration": "releaseRuntimeClasspath",
                "bundleBinding": test_gradle_bundle_binding(&android),
                "repositories": repositories,
                "components": [{
                    "componentType": "module",
                    "group": "com.example",
                    "name": "local",
                    "version": "1.0.0",
                    "projectPath": null,
                    "artifacts": [{
                        "file": artifact,
                        "extension": "aar",
                        "classifier": ""
                    }]
                }],
                "dependencies": [test_gradle_edge(
                    "urn:yydra:gradle-project::app",
                    "pkg:maven/com.example/local@1.0.0"
                )]
            }))
            .expect("encode raw manifest"),
        )
        .expect("write raw manifest");
        let output = sandbox.path().join("evidence/gradle-materials.json");
        create_private_dir_all(output.parent().expect("output parent"))
            .expect("create evidence root");

        record_android_gradle_materials(&android, &gradle_home, &raw, &output)
            .expect("record npm-locked local Maven material");
        let inventory: serde_json::Value =
            serde_json::from_slice(&fs::read(output).expect("read material inventory"))
                .expect("parse material inventory");
        assert_eq!(
            inventory["components"][0]["provenance"]["integrityBasis"],
            "selected-artifact-set"
        );
        assert_eq!(
            inventory["components"][0]["detectedLicense"],
            "not-evaluated"
        );
        assert!(
            inventory["components"][0]["artifacts"]
                .as_array()
                .is_some_and(|artifacts| !artifacts.is_empty())
        );
    }

    #[test]
    fn android_advisory_queries_cover_selected_maven_artifacts_without_shipping_claims() {
        let sandbox = tempfile::tempdir().expect("create sandbox");
        let root = sandbox.path().join("workspace");
        let evidence = sandbox.path().join("evidence");
        write_supply_chain_authorities(&root);
        let material_path = evidence.join("artifacts/android.release/gradle-materials.json");
        create_private_dir_all(material_path.parent().expect("material parent"))
            .expect("create material directory");
        fs::write(
            &material_path,
            serde_json::to_vec(&json!({
                "schemaVersion": GRADLE_MATERIAL_SCHEMA_VERSION,
                "distributionVersion": DISTRIBUTION_VERSION,
                "configuration": "releaseRuntimeClasspath",
                "bundleBinding": test_retained_gradle_bundle_binding(),
                "sourceAuthority": GRADLE_MATERIAL_AUTHORITY,
                "repositories": test_gradle_repositories(),
                "components": [
                    {
                        "purl": "pkg:maven/com.example/runtime@1.2.3",
                        "group": "com.example",
                        "name": "runtime",
                        "version": "1.2.3",
                        "exposure": "runtime-build-input",
                        "source": "gradle:releaseRuntimeClasspath",
                        "sourceIntegrity": format!("sha256:{}", "1".repeat(64)),
                        "declaredLicense": "MIT",
                        "detectedLicense": "MIT",
                        "artifacts": [{
                            "path": "caches/runtime.jar",
                            "extension": "jar",
                            "classifier": "",
                            "bytes": 1,
                            "sha256": format!("sha256:{}", "2".repeat(64))
                        }],
                        "metadata": [],
                        "notices": [],
                        "provenance": {}
                    },
                    {
                        "purl": "pkg:maven/com.example/platform@9.0.0",
                        "group": "com.example",
                        "name": "platform",
                        "version": "9.0.0",
                        "exposure": "dependency-graph-only",
                        "source": "gradle:releaseRuntimeClasspath",
                        "sourceIntegrity": format!("sha256:{}", "3".repeat(64)),
                        "declaredLicense": "Apache-2.0",
                        "detectedLicense": "Apache-2.0",
                        "artifacts": [],
                        "metadata": [],
                        "notices": [],
                        "provenance": {}
                    }
                ],
                "localComponents": [],
                "dependencies": [
                    test_gradle_edge(
                        "pkg:maven/com.example/runtime@1.2.3",
                        "pkg:maven/com.example/platform@9.0.0"
                    ),
                    test_gradle_edge(
                        "urn:yydra:gradle-project::app",
                        "pkg:maven/com.example/runtime@1.2.3"
                    )
                ],
                "reportBoundary": "Exact materials do not by themselves prove packaged reachability."
            }))
            .expect("encode material inventory"),
        )
        .expect("write material inventory");

        let invocation = prepare_android_advisory_query(&root, &evidence)
            .expect("prepare Android Maven advisory query");
        assert_eq!(
            invocation.response,
            evidence.join("artifacts/supply-chain.android-advisories/response.json")
        );
        let query_map: AdvisoryQueryMap = serde_json::from_slice(
            &fs::read(evidence.join("artifacts/supply-chain.android-advisories/query-map.json"))
                .expect("read query map"),
        )
        .expect("parse query map");
        assert_eq!(query_map.queries.len(), 1);
        assert_eq!(
            query_map.queries[0].purl,
            "pkg:maven/com.example/runtime@1.2.3"
        );
        assert_eq!(query_map.queries[0].ecosystem, "Maven");
        assert_eq!(query_map.queries[0].name, "com.example:runtime");
        assert_eq!(query_map.queries[0].version, "1.2.3");
        assert_eq!(query_map.queries[0].targets, ["android"]);
        assert_eq!(query_map.queries[0].exposure, "runtime-build-input");
    }

    #[test]
    fn android_advisories_require_exact_maven_exceptions_without_consuming_other_scopes() {
        let sandbox = tempfile::tempdir().expect("create sandbox");
        let root = sandbox.path().join("workspace");
        let evidence = sandbox.path().join("evidence");
        write_supply_chain_authorities(&root);
        let artifact_root = evidence.join("artifacts/supply-chain.android-advisories");
        create_private_dir_all(&artifact_root).expect("create advisory evidence root");
        write_json(
            &artifact_root.join("query-map.json"),
            &AdvisoryQueryMap {
                schema_version: 1,
                queries: vec![AdvisoryQueryComponent {
                    purl: "pkg:maven/com.example/runtime@1.2.3".to_owned(),
                    identity_basis: "gradle-resolution".to_owned(),
                    local_source_identity: "not-evaluated".to_owned(),
                    ecosystem: "Maven".to_owned(),
                    name: "com.example:runtime".to_owned(),
                    version: "1.2.3".to_owned(),
                    targets: vec!["android".to_owned()],
                    exposure: "shipped-linked".to_owned(),
                }],
            },
        )
        .expect("write query map");
        fs::write(
            artifact_root.join("response.json"),
            br#"{"results":[{"vulns":[{"id":"OSV-TEST-MAVEN","modified":"2026-09-03T00:00:00Z"}]}]}"#,
        )
        .expect("write OSV response");
        assert_eq!(
            android_advisory_evidence(&root, &evidence)
                .expect_err("unexcepted Maven advisory must block")
                .code,
            "SUPPLY_CHAIN_VULNERABILITY_FOUND"
        );

        let exception_path = root.join(".yydra/supply-chain-exceptions.json");
        let mut exceptions: serde_json::Value =
            serde_json::from_slice(&fs::read(&exception_path).expect("read exception authority"))
                .expect("parse exception authority");
        let values = exceptions["vulnerabilityExceptions"]
            .as_array_mut()
            .expect("exception array");
        for (advisory_id, purl, version, target) in [
            (
                "OSV-TEST-MAVEN",
                "pkg:maven/com.example/runtime@1.2.3",
                "1.2.3",
                "android",
            ),
            (
                "OSV-TEST-NPM-OTHER-SCOPE",
                "pkg:npm/react@19.2.3",
                "19.2.3",
                "h5",
            ),
        ] {
            values.push(json!({
                "advisoryId": advisory_id,
                "purl": purl,
                "version": version,
                "target": target,
                "impactAnalysis": "The affected path is not reachable in this exact retained artifact.",
                "owner": "Yydra release owner",
                "evidence": "https://github.com/yydcnjjw/yydra/issues/39",
                "approvedBy": "yydcnjjw",
                "approvedAt": "2026-09-03T00:00:00Z",
                "expiresAt": "2999-01-01T00:00:00Z",
                "reReviewTrigger": "Any advisory, version, target, or artifact change"
            }));
        }
        fs::write(
            &exception_path,
            serde_json::to_vec_pretty(&exceptions).expect("encode exact exceptions"),
        )
        .expect("write exact exceptions");
        fs::remove_file(artifact_root.join("report.json"))
            .expect("remove first-run failure report from isolated test evidence");
        android_advisory_evidence(&root, &evidence)
            .expect("exact Maven exception passes and unrelated npm exception is deferred");
        let report: serde_json::Value = serde_json::from_slice(
            &fs::read(artifact_root.join("report.json")).expect("read advisory report"),
        )
        .expect("parse advisory report");
        assert_eq!(report["status"], "pass");
        assert_eq!(report["exceptionsApplied"], 1);
    }

    #[test]
    fn embedded_cli_graph_is_strict_and_relocatable() {
        let graph: CliGraph = serde_json::from_slice(CLI_GRAPH).expect("parse embedded CLI graph");
        assert_eq!(graph.generated_by, "scripts/generate-cli-supply-chain.mjs");
        assert_eq!(
            graph.root,
            format!("pkg:cargo/yydra-cli@{DISTRIBUTION_VERSION}")
        );
        assert!(!graph.packages.is_empty());
        for package in &graph.packages {
            assert!(!Path::new(&package.manifest_path).is_absolute());
            assert!(!package.id.contains("file://"));
            assert!(!package.target_kinds.is_empty());
        }
        let exposures = cargo_artifact_exposures(
            [graph.root.clone()],
            graph.dependencies.iter().flat_map(|set| {
                set.to.iter().map(|edge| CargoExposureEdge {
                    from: set.from.clone(),
                    to: edge.purl.clone(),
                    links_runtime_artifact: edge.kinds.iter().any(|kind| kind.kind == "normal")
                        && graph
                            .packages
                            .iter()
                            .find(|package| package.purl == edge.purl)
                            .is_some_and(|package| {
                                cargo_target_kinds_link_runtime_artifact(
                                    package.target_kinds.iter().map(String::as_str),
                                )
                            }),
                })
            }),
        );
        let serde_derive = graph
            .packages
            .iter()
            .find(|package| package.name == "serde_derive")
            .expect("embedded graph contains serde_derive");
        assert_eq!(serde_derive.target_kinds, ["proc-macro"]);
        assert_eq!(
            exposures[&serde_derive.purl],
            CargoArtifactExposure::BuildToolExecuted,
            "a proc-macro normal edge executes during build and is not linked into the CLI artifact"
        );
    }

    #[test]
    fn cargo_build_and_dev_subtrees_never_become_shipped_via_normal_transitives() {
        let exposures = cargo_artifact_exposures(
            ["server".to_owned()],
            [
                CargoExposureEdge {
                    from: "server".to_owned(),
                    to: "runtime".to_owned(),
                    links_runtime_artifact: true,
                },
                CargoExposureEdge {
                    from: "server".to_owned(),
                    to: "build-script".to_owned(),
                    links_runtime_artifact: false,
                },
                CargoExposureEdge {
                    from: "build-script".to_owned(),
                    to: "build-helper".to_owned(),
                    links_runtime_artifact: true,
                },
                CargoExposureEdge {
                    from: "server".to_owned(),
                    to: "test-helper".to_owned(),
                    links_runtime_artifact: false,
                },
                CargoExposureEdge {
                    from: "test-helper".to_owned(),
                    to: "test-transitive".to_owned(),
                    links_runtime_artifact: true,
                },
            ],
        );
        assert_eq!(exposures["runtime"], CargoArtifactExposure::ShippedLinked);
        for package in [
            "build-script",
            "build-helper",
            "test-helper",
            "test-transitive",
        ] {
            assert_eq!(
                exposures[package],
                CargoArtifactExposure::BuildToolExecuted,
                "{package}"
            );
        }
    }

    fn write_supply_chain_authorities(root: &Path) {
        let authority_root = root.join(".yydra");
        fs::create_dir_all(&authority_root).expect("create authority directory");
        for (name, source) in [
            (
                "supply-chain-policy.json",
                include_str!("../template/product-workspace/.yydra/supply-chain-policy.json"),
            ),
            (
                "supply-chain-exceptions.json",
                include_str!("../template/product-workspace/.yydra/supply-chain-exceptions.json"),
            ),
        ] {
            fs::write(
                authority_root.join(name),
                source.replace("__YYDRA_DISTRIBUTION_VERSION__", DISTRIBUTION_VERSION),
            )
            .expect("write supply-chain authority");
        }
    }

    fn stored_zip(name: &str, data: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        push_u32(&mut output, 0x0403_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(
            &mut output,
            u32::try_from(data.len()).expect("fixture size"),
        );
        push_u32(
            &mut output,
            u32::try_from(data.len()).expect("fixture size"),
        );
        push_u16(
            &mut output,
            u16::try_from(name.len()).expect("fixture name"),
        );
        push_u16(&mut output, 0);
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(data);
        let central_offset = u32::try_from(output.len()).expect("fixture central offset");
        push_u32(&mut output, 0x0201_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(
            &mut output,
            u32::try_from(data.len()).expect("fixture size"),
        );
        push_u32(
            &mut output,
            u32::try_from(data.len()).expect("fixture size"),
        );
        push_u16(
            &mut output,
            u16::try_from(name.len()).expect("fixture name"),
        );
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, 0);
        output.extend_from_slice(name.as_bytes());
        let central_size = u32::try_from(output.len()).expect("fixture size") - central_offset;
        push_u32(&mut output, 0x0605_4b50);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 1);
        push_u16(&mut output, 1);
        push_u32(&mut output, central_size);
        push_u32(&mut output, central_offset);
        push_u16(&mut output, 0);
        output
    }

    fn push_u16(output: &mut Vec<u8>, value: u16) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(output: &mut Vec<u8>, value: u32) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn empty_npm_package() -> NpmPackage {
        NpmPackage {
            name: None,
            version: None,
            resolved: None,
            integrity: None,
            license: None,
            dev: false,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            optional_dependencies: BTreeMap::new(),
            peer_dependencies: BTreeMap::new(),
        }
    }
}
