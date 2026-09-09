// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
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
    npm_program, read_workspace_origin_record, template_source_files, verify_origin_authority,
    verify_snapshot_authorities,
};

#[cfg(windows)]
use crate::{WindowsJob, create_kill_on_close_job};

const RESULT_SCHEMA_VERSION: u64 = 2;
const INPUTS_UNCHANGED_NODE: &str = "ownership.authored-inputs-unchanged";

pub(crate) struct CheckRequest {
    pub(crate) workspace: PathBuf,
    pub(crate) evidence_dir: Option<PathBuf>,
    pub(crate) comparison_base: Option<String>,
    pub(crate) selected_nodes: Vec<String>,
    pub(crate) fixture: String,
}

pub(crate) struct AggregateRequest {
    pub(crate) evidence_dir: Option<PathBuf>,
    pub(crate) manifests: Vec<PathBuf>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeSpec {
    id: &'static str,
    prerequisites: &'static [&'static str],
    remediation: &'static str,
    proves: &'static str,
    does_not_prove: &'static str,
}

const NODE_SPECS: &[NodeSpec] = &[
    NodeSpec {
        id: "policy.exceptions",
        prerequisites: &[],
        remediation: "remove .yydra/check-exceptions.toml; this exact Distribution has a deny-all exception policy and required failures must be fixed",
        proves: "the Workspace does not attempt to waive a required Mechanical Quality Contract node",
        does_not_prove: "that future Distributions will never admit a narrowly reviewed exception mechanism",
    },
    NodeSpec {
        id: "origin.exact-distribution",
        prerequisites: &[],
        remediation: "restore the reviewed Workspace Origin Record and exact Distribution snapshots, install the exact CLI version named by the record, or recreate a named aggregate fixture with its catalog-owned exact inputs",
        proves: "the Workspace identity and normalized creation inputs name this exact packaged CLI Distribution and, when selected, the exact catalog-owned aggregate fixture",
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
        does_not_prove: "current API generation correctness or native-host reproducibility owned by later graph nodes",
    },
    NodeSpec {
        id: "database.migration-history",
        prerequisites: &["origin.exact-distribution"],
        remediation: "restore every edited or deleted migration that existed in the exact Distribution or requested Git comparison base, then add a new forward migration for corrections",
        proves: "the Product Workspace has one root SQLx migration authority, exact-Distribution migrations retain their bytes, and a requested Git comparison base contains no edited or deleted migration",
        does_not_prove: "that later Product migrations absent from the requested comparison base have been applied, or that a database accepts the complete history",
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
        id: "frontend.lock",
        prerequisites: &[],
        remediation: "restore frontend/package.json and package-lock.json, then run yydra setup from the exact Distribution",
        proves: "npm can install the exact committed frontend resolution without rewriting its lock",
        does_not_prove: "advisory, provenance, or artifact license policy",
    },
    NodeSpec {
        id: "api.generated-contract",
        prerequisites: &["rust.architecture", "frontend.lock"],
        remediation: "fix the Rust contract or generator configuration and rerun `yydra build`",
        proves: "Rust route collection generates and validates the current OpenAPI and Orval Fetch/TypeScript/Zod build outputs without changing authored inputs",
        does_not_prove: "that the running service returns every documented response or that Product Domain behavior is correct",
    },
    NodeSpec {
        id: "rust.compile",
        prerequisites: &["api.generated-contract"],
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
        id: "api.runtime-conformance",
        prerequisites: &["api.generated-contract"],
        remediation: "align the public-route handler status, content type, headers, and response body with the current Public API Contract",
        proves: "the Framework-owned public router matches its collected contract and discriminating fixtures reject invalid JSON, unknown request or query fields, malformed pagination responses, invalid cursors, undocumented status or content type, prohibited transitions, and incorrect 401/403 authentication meanings",
        does_not_prove: "exhaustive generated-input coverage, a full Identity system, or database-backed Product Domain behavior",
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
        prerequisites: &["api.generated-contract"],
        remediation: "fix the strict no-emit TypeScript errors",
        proves: "the complete frontend TypeScript project passes strict type checking without emit",
        does_not_prove: "browser behavior or server contract conformance",
    },
    NodeSpec {
        id: "frontend.test",
        prerequisites: &["api.generated-contract"],
        remediation: "restore non-empty canonical Vitest coverage and fix the reported failure",
        proves: "the canonical frontend test runner discovered tests and all selected tests passed",
        does_not_prove: "production H5 Application Surface behavior against the real service",
    },
    NodeSpec {
        id: "native.android-generation",
        prerequisites: &["api.generated-contract"],
        remediation: "fix committed Expo app configuration, exact dependencies, standard config plugins, or local Expo Modules; do not patch frontend/android generated source",
        proves: "two clean Android generations from the same authored inputs reproduce the complete path, mode, and byte inventory without changing Workspace inputs",
        does_not_prove: "an Android release build, Android runtime behavior, physical-device behavior, or native accessibility",
    },
    NodeSpec {
        id: "android.release",
        prerequisites: &["native.android-generation"],
        remediation: "inspect the raw Expo and Gradle logs, then fix authored Expo inputs, exact dependencies, local Expo Modules, or the pinned Android runner; do not patch frontend/android generated source or require Expo/EAS credentials",
        proves: "a clean generated Android host produces an identified release APK through the local Gradle wrapper without Expo or EAS credentials",
        does_not_prove: "Android runtime behavior, installation, physical-device behavior, native accessibility, signing for store distribution, or bit-for-bit cross-host reproducibility",
    },
    NodeSpec {
        id: "server.release",
        prerequisites: &["rust.compile"],
        remediation: "fix the exact locked Product server release build and ensure Cargo emits the named binary without changing the build profile or package identity",
        proves: "the exact Product Workspace and committed Cargo.lock produce the named server release binary whose bytes and SHA-256 identity are retained in this evidence root",
        does_not_prove: "deployment compatibility, production configuration, cross-host reproducibility, runtime correctness, or absence of vulnerabilities",
    },
    NodeSpec {
        id: "api.client-contract",
        prerequisites: &["api.generated-contract", "frontend.typecheck"],
        remediation: "restore the generated runtime schemas and fix the handwritten Framework facade without importing Generated Client internals from Product code",
        proves: "the handwritten facade injects credentials and transport concerns, validates strict query-specific pagination input and nullable cursors, validates 401 challenges, and classifies typed Problems, transport, caller cancellation, timeout, and malformed or undocumented responses without parsing Problem prose for behavior",
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
        id: "database.runtime-invariants",
        prerequisites: &[
            "database.migration-history",
            "rust.compile",
            "infrastructure.docker",
        ],
        remediation: "inspect the node log, restore append-only migrations and the named application transaction, row-lock, derived-state, and rollback contracts, then rerun the focused database node",
        proves: "fresh and applied migration histories, explicit use-case transactions, cross-domain synchronous derived state, rollback paths, and the selected READ COMMITTED row-lock contention behavior pass against real PostgreSQL without command retry",
        does_not_prove: "all possible Product Domain invariants, SERIALIZABLE behavior, asynchronous projections, durable work, or production database operations",
    },
    NodeSpec {
        id: "runtime.post-commit-executor",
        prerequisites: &["rust.compile"],
        remediation: "restore the named bounded lossy post-commit executor lifecycle, including capacity rejection, admission-anchored task deadlines, cancellation, tracing, terminal failures, and deadline-bound shutdown; keep business invariants synchronous",
        proves: "the non-durable post-commit seam rejects excess admission, enforces admission-anchored task deadlines across queue wait and execution, runs named tasks without retry, exposes structured lifecycle tracing and metrics, distinguishes failure, timeout, panic, and cancellation, and bounds shutdown",
        does_not_prove: "durable delivery, retry, process-crash recovery, exactly-once execution, or correctness for any business invariant deferred to this lossy seam",
    },
    NodeSpec {
        id: "infrastructure.playwright-chromium",
        prerequisites: &["frontend.lock"],
        remediation: "install the pinned Playwright Chromium browser for this host, then rerun yydra check",
        proves: "the pinned Playwright package resolves to an installed Chromium executable on this host",
        does_not_prove: "that the production H5 Application Surface export or its browser assertions will pass",
    },
    NodeSpec {
        id: "h5.product-presentation-accessibility",
        prerequisites: &[
            "rust.compile",
            "frontend.typecheck",
            "infrastructure.docker",
            "infrastructure.playwright-chromium",
        ],
        remediation: "restore frontend/e2e/product-presentation.accessibility.spec.ts, remove skipped or focused cases, and fix the first visible role, name, state, or heading assertion without weakening the registered Product semantics",
        proves: "the visible Product-owned Playwright semantic specification executed without retry and passed its registered H5 role, accessible-name, state, focus, and dynamic-heading assertions",
        does_not_prove: "complete WCAG conformance, Android or iOS assistive-technology behavior, unregistered Product semantics, hidden acceptance, physical-device accessibility, Agent Safe Completion, or Baseline Skill effect",
    },
    NodeSpec {
        id: "h5.real-runtime",
        prerequisites: &[
            "rust.compile",
            "frontend.typecheck",
            "infrastructure.docker",
            "infrastructure.playwright-chromium",
        ],
        remediation: "inspect the node log, run yydra db migrate and the focused production H5 Application Surface test, then fix the first semantic failure",
        proves: "the production H5 Application Surface export creates, completes, reopens, filters, paginates, refreshes from page one, and restores URL state through the Framework client, real Axum handlers, explicit SQLx transactions, database constraints, and PostgreSQL; stable request, cursor, transition, and authentication Problems plus focused rollback and keyset-order fixtures also pass",
        does_not_prove: "cross-request snapshot consistency, universal totals or pagination, a full Identity system, Android runtime, physical-device behavior, native accessibility, or complete WCAG conformance",
    },
    NodeSpec {
        id: INPUTS_UNCHANGED_NODE,
        prerequisites: &[],
        remediation: "restore every changed authored, snapshot, generated, lock, migration, and configuration input; check mode must remain read-only",
        proves: "all in-scope Workspace input paths and bytes are identical before and after this check invocation",
        does_not_prove: "bit-for-bit build-output reproducibility or absence of external side effects",
    },
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Pass,
    Fail,
    InfrastructureError,
    Skipped,
    NotRun,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckCause {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependency_node_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AttemptResult {
    attempt: u8,
    outcome: Outcome,
    duration_ms: u64,
    cause: Option<CheckCause>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NodeResult {
    schema_version: u64,
    event: String,
    node_id: String,
    prerequisites: Vec<String>,
    outcome: Outcome,
    duration_ms: u64,
    attempts: Vec<AttemptResult>,
    cause: Option<CheckCause>,
    remediation: Option<String>,
    proves: String,
    does_not_prove: String,
    commands: Vec<String>,
    tool_versions: BTreeMap<String, String>,
    log: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckManifest {
    schema_version: u64,
    distribution_version: String,
    cli_version: String,
    executor_digest: String,
    rule_schema_version: u64,
    workspace: String,
    fixture: String,
    profile: String,
    scope: String,
    catalog_digest: String,
    diagnostic_vocabulary: Vec<String>,
    not_evaluated: Vec<String>,
    exception_policy: ExceptionPolicy,
    retry_policy: RetryPolicy,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactEvidence {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExceptionPolicy {
    mode: String,
    configuration_path: String,
    waiver_capable_nodes: Vec<String>,
    non_waivable_nodes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetryPolicy {
    semantic_max_attempts: u8,
    generation_max_attempts: u8,
    conformance_max_attempts: u8,
    infrastructure_establishment_max_attempts: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogDefinition {
    schema_version: u64,
    distribution_version: &'static str,
    result_states: &'static [&'static str],
    fixtures: &'static [&'static str],
    fixture_definitions: &'static [FixtureSpec],
    nodes: &'static [NodeSpec],
    diagnostic_vocabulary: Vec<String>,
    not_evaluated: Vec<String>,
    exception_policy: ExceptionPolicy,
    retry_policy: RetryPolicy,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateSource {
    fixture: String,
    manifest: String,
    manifest_sha256: String,
    executor_digest: String,
    input_digest: String,
    artifacts: Vec<ArtifactEvidence>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateResult<'a> {
    schema_version: u64,
    event: &'static str,
    outcome: Outcome,
    cause: Option<CheckCause>,
    remediation: Option<&'a str>,
    proves: &'static str,
    does_not_prove: &'static str,
    sources: &'a [AggregateSource],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateManifest {
    schema_version: u64,
    distribution_version: String,
    cli_version: String,
    executor_digest: String,
    rule_schema_version: u64,
    profile: &'static str,
    scope: &'static str,
    catalog_digest: String,
    diagnostic_vocabulary: Vec<String>,
    not_evaluated: Vec<String>,
    exception_policy: ExceptionPolicy,
    retry_policy: RetryPolicy,
    required_fixtures: &'static [&'static str],
    status: &'static str,
    complete: bool,
    aggregate_conformance: bool,
    cause: Option<CheckCause>,
    sources: Vec<AggregateSource>,
    artifacts: Vec<ArtifactEvidence>,
    proves: &'static str,
    does_not_prove: &'static str,
}

#[derive(Clone, Debug)]
struct AggregateFailure {
    code: &'static str,
    message: String,
}

impl AggregateFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn cause(&self) -> CheckCause {
        CheckCause {
            code: self.code.to_owned(),
            message: self.message.clone(),
            dependency_node_id: None,
        }
    }
}

const RESULT_STATES: &[&str] = &["pass", "fail", "infrastructure-error", "skipped", "not-run"];
const AGGREGATE_FIXTURES: &[&str] = &["clean", "reading-queue"];

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FixtureSpec {
    id: &'static str,
    product_name: &'static str,
    product_id: &'static str,
    product_source_license: &'static str,
}

const FIXTURE_SPECS: &[FixtureSpec] = &[
    FixtureSpec {
        id: "clean",
        product_name: "Clean Product",
        product_id: "clean-product",
        product_source_license: "Apache-2.0",
    },
    FixtureSpec {
        id: "reading-queue",
        product_name: "Reading Queue",
        product_id: "reading-queue",
        product_source_license: "Apache-2.0",
    },
];

const DIAGNOSTIC_VOCABULARY: &[&str] = &[
    "ACCESSIBILITY_ASSERTION_FAILED",
    "ACCESSIBILITY_FOCUSED_OR_SKIPPED",
    "ACCESSIBILITY_MIGRATION_FAILED",
    "ACCESSIBILITY_NO_EXECUTED_TESTS",
    "ACCESSIBILITY_POSTGRES_CLEANUP_FAILED",
    "ACCESSIBILITY_POSTGRES_UNAVAILABLE",
    "ACCESSIBILITY_REPORT_INVALID",
    "ACCESSIBILITY_SPEC_MISSING",
    "AGGREGATE_ARTIFACT_INVALID",
    "AGGREGATE_CATALOG_MISMATCH",
    "AGGREGATE_DIAGNOSTICS_INVALID",
    "AGGREGATE_DUPLICATE_FIXTURE",
    "AGGREGATE_EVIDENCE_INCOMPLETE",
    "AGGREGATE_EVIDENCE_MALFORMED",
    "AGGREGATE_EVIDENCE_MISSING",
    "AGGREGATE_EXCEPTION_REJECTED",
    "AGGREGATE_FIXTURE_MISSING",
    "AGGREGATE_IDENTITY_MISMATCH",
    "AGGREGATE_NODE_SET_INVALID",
    "ANDROID_RELEASE_BUILD_FAILED",
    "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
    "ANDROID_RELEASE_OUTPUT_MISSING",
    "ANDROID_RELEASE_OUTPUT_UNREADABLE",
    "API_BUILD_FAILED",
    "API_CLIENT_CONTRACT_FAILED",
    "API_CLIENT_GENERATION_FAILED",
    "API_CLIENT_IMPORT_BOUNDARY_VIOLATION",
    "API_CLIENT_IMPORT_SCAN_FAILED",
    "API_CLIENT_LINK_FAILED",
    "API_CLIENT_OUTPUT_INVALID",
    "API_CLIENT_TOOL_VERSION_INVALID",
    "API_CLIENT_TYPECHECK_FAILED",
    "API_GENERATED_CONTRACT_FAILED",
    "API_OPENAPI_CONTENT_TYPE_INVALID",
    "API_OPENAPI_DECIMAL_INVALID",
    "API_OPENAPI_EXPORT_FAILED",
    "API_OPENAPI_FIELD_NAME_INVALID",
    "API_OPENAPI_NULLABILITY_INVALID",
    "API_OPENAPI_OPERATION_ID_INVALID",
    "API_OPENAPI_PROFILE_INVALID",
    "API_OPENAPI_REQUIREDNESS_INVALID",
    "API_OPENAPI_SAFE_INTEGER_INVALID",
    "API_OPENAPI_SHAPE_REUSE_INVALID",
    "API_OPENAPI_TIMESTAMP_INVALID",
    "API_OPENAPI_UNKNOWN_FIELD_POLICY_INVALID",
    "API_OPENAPI_WIRE_TYPE_INVALID",
    "API_OUTPUT_MISSING",
    "API_OUTPUT_PREPARE_FAILED",
    "API_RUNTIME_CONFORMANCE_FAILED",
    "API_WORKSPACE_INVALID",
    "ARCH_DEPENDENCY_CYCLE",
    "ARCH_FORBIDDEN_DEPENDENCY",
    "ARCH_FORBIDDEN_LAYER_EDGE",
    "ARCH_FRAMEWORK_INTERNAL_DEPENDENCY",
    "ARCH_METADATA_INVALID",
    "ARCH_UNKNOWN_WORKSPACE_ROLE",
    "ARCH_WORKSPACE_PATH_ESCAPE",
    "BASELINE_SKILL_INVENTORY_DRIFT",
    "CARGO_LOCK_DRIFT",
    "CHECK_CANCELLED",
    "CHECK_CLEANUP_TIMEOUT",
    "CHECK_CLOCK_UNAVAILABLE",
    "CHECK_EVIDENCE_WRITE_FAILED",
    "CHECK_EXCEPTION_POLICY_VIOLATION",
    "CHECK_INPUT_INVENTORY_FAILED",
    "CHECK_MUTATED_ORIGINAL_INPUTS",
    "CHECK_MUTATED_WORKSPACE_INPUTS",
    "CHECK_NOT_SELECTED",
    "CHECK_PORT_UNAVAILABLE",
    "CHECK_PREFLIGHT_FAILED",
    "CHECK_PREREQUISITE_FAILED",
    "CHECK_SCRATCH_CLEANUP_FAILED",
    "CHECK_SCRATCH_COPY_FAILED",
    "CHECK_SYMLINK_PATH_ESCAPE",
    "CHECK_SYMLINK_TARGET_EXCLUDED",
    "CHECK_SYMLINK_TARGET_INVALID",
    "CHECK_TOOL_POLL_FAILED",
    "CHECK_TOOL_UNAVAILABLE",
    "CHECK_TOOL_VERSION_INVALID",
    "CHECK_TOOL_VERSION_MISMATCH",
    "CHECK_TOOL_VERSION_UNAVAILABLE",
    "DATABASE_MIGRATION_FAILED",
    "DATABASE_POSTGRES_CLEANUP_FAILED",
    "DATABASE_POSTGRES_UNAVAILABLE",
    "DATABASE_RUNTIME_INVARIANTS_FAILED",
    "DATABASE_RUNTIME_INVARIANT_TEST_MISSING",
    "DB_MIGRATION_AUTHORITY_AMBIGUOUS",
    "DB_MIGRATION_COMPARISON_BASE_DELETED",
    "DB_MIGRATION_COMPARISON_BASE_INVALID",
    "DB_MIGRATION_COMPARISON_BASE_MUTATED",
    "DB_MIGRATION_COMPARISON_BASE_UNAVAILABLE",
    "DB_MIGRATION_COMPARISON_FAILED",
    "DB_MIGRATION_DISTRIBUTION_BASE_DELETED",
    "DB_MIGRATION_DISTRIBUTION_BASE_MUTATED",
    "DB_MIGRATION_HISTORY_INVALID",
    "DB_MIGRATION_HISTORY_MISSING",
    "DOCKER_UNAVAILABLE",
    "FRONTEND_FORMAT_FAILED",
    "FRONTEND_LINT_FAILED",
    "FRONTEND_LOCK_INSTALL_FAILED",
    "FRONTEND_LOCK_MISSING",
    "FRONTEND_LOCK_MUTATED",
    "FRONTEND_TESTS_EMPTY",
    "FRONTEND_TEST_DISCOVERY_FAILED",
    "FRONTEND_TEST_FAILED",
    "FRONTEND_TOOLCHAIN_DRIFT",
    "FRONTEND_TYPECHECK_FAILED",
    "FIXTURE_IDENTITY_MISMATCH",
    "GENERATED_SNAPSHOT_DRIFT",
    "H5_E2E_FAILED",
    "H5_MIGRATION_FAILED",
    "H5_POSTGRES_CLEANUP_FAILED",
    "H5_POSTGRES_UNAVAILABLE",
    "H5_SERVER_ADDRESS_INVALID",
    "H5_SERVER_EXITED",
    "H5_SERVER_POLL_FAILED",
    "H5_SERVER_SUPERVISION_UNAVAILABLE",
    "H5_SERVER_TIMEOUT",
    "H5_SERVER_UNAVAILABLE",
    "NATIVE_GENERATION_CLEANUP_FAILED",
    "NATIVE_GENERATION_DIRTY_OUTPUT",
    "NATIVE_GENERATION_FAILED",
    "NATIVE_GENERATION_INPUT_POLICY_FAILED",
    "NATIVE_GENERATION_INVENTORY_FAILED",
    "NATIVE_GENERATION_MUTATED_AUTHORED_INPUTS",
    "NATIVE_GENERATION_NONDETERMINISTIC",
    "NATIVE_GENERATION_OUTPUT_MISSING",
    "ORIGIN_AUTHORITY_DRIFT",
    "PLAYWRIGHT_CHROMIUM_UNAVAILABLE",
    "POST_COMMIT_EXECUTOR_FAILED",
    "POST_COMMIT_EXECUTOR_TEST_MISSING",
    "READING_QUEUE_PAGINATION_POSTGRES_FAILED",
    "READING_QUEUE_POSTGRES_FAILED",
    "RUST_CLIPPY_FAILED",
    "RUST_COMPILE_FAILED",
    "RUST_DOCTEST_FAILED",
    "RUST_FORMAT_FAILED",
    "RUST_TESTS_EMPTY",
    "RUST_TEST_DISCOVERY_FAILED",
    "RUST_TEST_FAILED",
    "RUST_TOOLCHAIN_AUTHORITY_DRIFT",
    "SERVER_RELEASE_BUILD_FAILED",
    "SERVER_RELEASE_OUTPUT_MISSING",
    "SERVER_RELEASE_OUTPUT_UNREADABLE",
];

fn retry_policy() -> RetryPolicy {
    RetryPolicy {
        semantic_max_attempts: 1,
        generation_max_attempts: 1,
        conformance_max_attempts: 1,
        infrastructure_establishment_max_attempts: 2,
    }
}

fn exception_policy() -> ExceptionPolicy {
    ExceptionPolicy {
        mode: "deny-all".to_owned(),
        configuration_path: ".yydra/check-exceptions.toml".to_owned(),
        waiver_capable_nodes: Vec::new(),
        non_waivable_nodes: NODE_SPECS.iter().map(|spec| spec.id.to_owned()).collect(),
    }
}

fn diagnostic_vocabulary() -> Vec<String> {
    DIAGNOSTIC_VOCABULARY
        .iter()
        .map(|code| (*code).to_owned())
        .collect()
}

fn not_evaluated() -> Vec<String> {
    [
        "dependency-inventory",
        "vulnerability-scanning",
        "vulnerability-exceptions",
        "sbom",
        "dependency-material-attribution",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn catalog_bytes() -> Result<Vec<u8>> {
    let catalog = CatalogDefinition {
        schema_version: RESULT_SCHEMA_VERSION,
        distribution_version: DISTRIBUTION_VERSION,
        result_states: RESULT_STATES,
        fixtures: AGGREGATE_FIXTURES,
        fixture_definitions: FIXTURE_SPECS,
        nodes: NODE_SPECS,
        diagnostic_vocabulary: diagnostic_vocabulary(),
        not_evaluated: not_evaluated(),
        exception_policy: exception_policy(),
        retry_policy: retry_policy(),
    };
    let mut bytes = serde_json::to_vec_pretty(&catalog)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sha256_identity(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn retain_bounded_artifact(
    source: &Path,
    destination: &Path,
    max_bytes: u64,
    failure_code: &'static str,
) -> std::result::Result<(u64, String), NodeFailure> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        NodeFailure::fail(
            failure_code,
            format!("inspect release artifact '{}': {error}", source.display()),
        )
    })?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > max_bytes
    {
        return Err(NodeFailure::fail(
            failure_code,
            format!(
                "release artifact '{}' is not a non-empty plain file within the {max_bytes} byte limit",
                source.display()
            ),
        ));
    }
    let mut input = File::open(source).map_err(|error| {
        NodeFailure::fail(
            failure_code,
            format!("open release artifact '{}': {error}", source.display()),
        )
    })?;
    let mut output = create_private_file(destination).map_err(evidence_write_failure)?;
    let mut digest = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| {
            NodeFailure::fail(
                failure_code,
                format!("read release artifact '{}': {error}", source.display()),
            )
        })?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(evidence_write_failure)?;
        digest.update(&buffer[..read]);
        copied = copied
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or_else(|| NodeFailure::fail(failure_code, "release byte count overflow"))?;
        if copied > max_bytes {
            return Err(NodeFailure::fail(
                failure_code,
                "release artifact changed beyond its bounded copy limit",
            ));
        }
    }
    output.flush().map_err(evidence_write_failure)?;
    if copied != metadata.len() {
        return Err(NodeFailure::fail(
            failure_code,
            "release artifact changed while it was retained",
        ));
    }
    Ok((copied, format!("sha256:{}", hex::encode(digest.finalize()))))
}

fn hash_bounded_artifact(
    source: &Path,
    max_bytes: u64,
    failure_code: &'static str,
) -> std::result::Result<String, NodeFailure> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        NodeFailure::fail(
            failure_code,
            format!("inspect evidence artifact '{}': {error}", source.display()),
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        return Err(NodeFailure::fail(
            failure_code,
            format!(
                "evidence artifact '{}' is not a plain file within the {max_bytes} byte limit",
                source.display()
            ),
        ));
    }
    let mut input = File::open(source).map_err(|error| {
        NodeFailure::fail(
            failure_code,
            format!("open evidence artifact '{}': {error}", source.display()),
        )
    })?;
    let mut digest = Sha256::new();
    let mut hashed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| {
            NodeFailure::fail(
                failure_code,
                format!("read evidence artifact '{}': {error}", source.display()),
            )
        })?;
        if read == 0 {
            break;
        }
        hashed = hashed
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or_else(|| NodeFailure::fail(failure_code, "evidence byte count overflow"))?;
        if hashed > max_bytes {
            return Err(NodeFailure::fail(
                failure_code,
                "evidence artifact changed beyond its bounded hash limit",
            ));
        }
        digest.update(&buffer[..read]);
    }
    if hashed != metadata.len() {
        return Err(NodeFailure::fail(
            failure_code,
            "evidence artifact changed while it was hashed",
        ));
    }
    Ok(format!("sha256:{}", hex::encode(digest.finalize())))
}

fn executor_digest() -> Result<String> {
    let executable = std::env::current_exe().context("resolve exact yydra executable")?;
    let bytes = fs::read(&executable)
        .with_context(|| format!("read exact yydra executable '{}'", executable.display()))?;
    Ok(sha256_identity(&bytes))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SummaryEvent {
    schema_version: u64,
    event: String,
    status: String,
    scope: String,
    complete: bool,
    aggregate_conformance: bool,
    evidence: String,
}

#[derive(Clone, Debug)]
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
    let CheckRequest {
        workspace,
        evidence_dir,
        comparison_base,
        selected_nodes,
        fixture,
    } = request;
    let root = find_workspace_root(&workspace)?;
    let evidence_root = evidence_root(&root, evidence_dir)?;
    create_private_dir_all(&evidence_root).with_context(|| {
        format!(
            "create private check evidence directory '{}'",
            evidence_root.display()
        )
    })?;
    create_private_dir_all(&evidence_root.join("logs"))?;
    create_private_dir_all(&evidence_root.join("artifacts"))?;
    let catalog = catalog_bytes()?;
    let catalog_digest = sha256_identity(&catalog);
    let executor_digest = executor_digest()?;
    let catalog_path = evidence_root.join("artifacts/check-catalog.json");
    let mut catalog_file = create_private_file(&catalog_path)?;
    catalog_file.write_all(&catalog)?;
    catalog_file.flush()?;

    let selected = selected_specs(&selected_nodes)?;
    let complete = selected_nodes.is_empty();
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
                    requested_nodes: &selected_nodes,
                    selected: &selected,
                    complete,
                    fixture: &fixture,
                    catalog_digest: &catalog_digest,
                    executor_digest: &executor_digest,
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
                requested_nodes: &selected_nodes,
                selected: &selected,
                complete,
                fixture: &fixture,
                catalog_digest: &catalog_digest,
                executor_digest: &executor_digest,
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
                    requested_nodes: &selected_nodes,
                    selected: &selected,
                    complete,
                    fixture: &fixture,
                    catalog_digest: &catalog_digest,
                    executor_digest: &executor_digest,
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
                comparison_base.as_deref(),
                &fixture,
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
    let manifest_path = evidence_root.join("manifest.json");
    let summary = summary_event(status, "clean-core-local", complete, false);
    serde_json::to_writer(&mut diagnostics, &summary)?;
    diagnostics.write_all(b"\n")?;
    diagnostics.flush()?;
    let artifacts = vec![
        artifact_evidence(&evidence_root, &evidence_root.join("logs"))?,
        artifact_evidence(&evidence_root, &evidence_root.join("artifacts"))?,
        artifact_evidence(&evidence_root, &diagnostics_path)?,
    ];
    let manifest = CheckManifest {
        schema_version: RESULT_SCHEMA_VERSION,
        distribution_version: DISTRIBUTION_VERSION.to_owned(),
        cli_version: DISTRIBUTION_VERSION.to_owned(),
        executor_digest,
        rule_schema_version: RESULT_SCHEMA_VERSION,
        workspace: root.display().to_string(),
        fixture,
        profile: "clean-core-local".to_owned(),
        scope: "clean-core-local".to_owned(),
        catalog_digest,
        diagnostic_vocabulary: diagnostic_vocabulary(),
        not_evaluated: not_evaluated(),
        exception_policy: exception_policy(),
        retry_policy: retry_policy(),
        catalog_nodes: NODE_SPECS.iter().map(|spec| spec.id.to_owned()).collect(),
        selected_nodes,
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
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    manifest_bytes.push(b'\n');
    let mut manifest_file = create_private_file(&manifest_path)?;
    manifest_file.write_all(&manifest_bytes)?;
    manifest_file.flush()?;
    emit_summary(&summary, &manifest_path, format);
    if failed {
        bail!(
            "Mechanical Quality Contract failed; inspect '{}'",
            manifest_path.display()
        );
    }
    Ok(())
}

pub(crate) fn aggregate(request: AggregateRequest, format: MessageFormat) -> Result<()> {
    let authority_root = std::env::current_dir()?.canonicalize()?;
    let evidence_root = evidence_root(&authority_root, request.evidence_dir)?;
    create_private_dir_all(&evidence_root)?;
    create_private_dir_all(&evidence_root.join("logs"))?;
    create_private_dir_all(&evidence_root.join("artifacts"))?;
    let catalog = catalog_bytes()?;
    let catalog_digest = sha256_identity(&catalog);
    let executor_digest = executor_digest()?;
    let mut catalog_file =
        create_private_file(&evidence_root.join("artifacts/check-catalog.json"))?;
    catalog_file.write_all(&catalog)?;
    catalog_file.flush()?;
    let log_path = evidence_root.join("logs/aggregate.clean-and-reading-queue.log");
    let mut log = create_private_file(&log_path)?;
    let diagnostics_path = evidence_root.join("diagnostics.jsonl");
    let mut diagnostics = create_private_file(&diagnostics_path)?;

    let verification = verify_aggregate_sources(
        &request.manifests,
        &catalog,
        &catalog_digest,
        &executor_digest,
    );
    let (status, complete, aggregate_conformance, outcome, cause, sources) = match verification {
        Ok(sources) => ("pass-aggregate", true, true, Outcome::Pass, None, sources),
        Err(failure) => {
            writeln!(log, "{}: {}", failure.code, failure.message)?;
            (
                "fail",
                false,
                false,
                Outcome::Fail,
                Some(failure.cause()),
                Vec::new(),
            )
        }
    };
    writeln!(
        log,
        "aggregate-conformance={aggregate_conformance} source-count={}",
        sources.len()
    )?;
    log.flush()?;

    let result = AggregateResult {
        schema_version: RESULT_SCHEMA_VERSION,
        event: "check-aggregate",
        outcome,
        cause: cause.clone(),
        remediation: cause.as_ref().map(|_| {
            "rerun both complete fixture checks with the exact Distribution, upload every evidence file unchanged, and aggregate those two manifests"
        }),
        proves: "exact complete clean and Reading Queue evidence from this Distribution is present, internally consistent, and passed every required node",
        does_not_prove: "supply-chain evaluation, claims excluded by individual nodes, macOS/iOS, native runtime, physical-device behavior, native accessibility, Agent performance, or Baseline Skill effect",
        sources: &sources,
    };
    serde_json::to_writer(&mut diagnostics, &result)?;
    diagnostics.write_all(b"\n")?;
    let manifest_path = evidence_root.join("manifest.json");
    let summary = summary_event(
        status,
        "clean-and-reading-queue",
        complete,
        aggregate_conformance,
    );
    serde_json::to_writer(&mut diagnostics, &summary)?;
    diagnostics.write_all(b"\n")?;
    diagnostics.flush()?;

    let artifacts = vec![
        artifact_evidence(&evidence_root, &evidence_root.join("logs"))?,
        artifact_evidence(&evidence_root, &evidence_root.join("artifacts"))?,
        artifact_evidence(&evidence_root, &diagnostics_path)?,
    ];
    let manifest = AggregateManifest {
        schema_version: RESULT_SCHEMA_VERSION,
        distribution_version: DISTRIBUTION_VERSION.to_owned(),
        cli_version: DISTRIBUTION_VERSION.to_owned(),
        executor_digest,
        rule_schema_version: RESULT_SCHEMA_VERSION,
        profile: "aggregate-v0",
        scope: "clean-and-reading-queue",
        catalog_digest,
        diagnostic_vocabulary: diagnostic_vocabulary(),
        not_evaluated: not_evaluated(),
        exception_policy: exception_policy(),
        retry_policy: retry_policy(),
        required_fixtures: AGGREGATE_FIXTURES,
        status,
        complete,
        aggregate_conformance,
        cause: cause.clone(),
        sources: sources.clone(),
        artifacts,
        proves: result.proves,
        does_not_prove: result.does_not_prove,
    };
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    let mut manifest_file = create_private_file(&manifest_path)?;
    manifest_file.write_all(&bytes)?;
    manifest_file.flush()?;
    emit_aggregate_result(&result, format);
    emit_summary(&summary, &manifest_path, format);

    if let Some(cause) = cause {
        bail!(
            "aggregate Mechanical Quality Contract failed with {}: {}; inspect '{}'",
            cause.code,
            cause.message,
            manifest_path.display()
        );
    }
    Ok(())
}

fn verify_aggregate_sources(
    manifest_paths: &[PathBuf],
    catalog: &[u8],
    catalog_digest: &str,
    executor_digest: &str,
) -> std::result::Result<Vec<AggregateSource>, AggregateFailure> {
    if manifest_paths.len() < AGGREGATE_FIXTURES.len() {
        return Err(AggregateFailure::new(
            "AGGREGATE_FIXTURE_MISSING",
            "aggregate conformance requires uploaded clean and reading-queue manifests",
        ));
    }
    let mut sources = BTreeMap::new();
    for path in manifest_paths {
        let (fixture, source) =
            verify_aggregate_source(path, catalog, catalog_digest, executor_digest)?;
        if sources.insert(fixture.clone(), source).is_some() {
            return Err(AggregateFailure::new(
                "AGGREGATE_DUPLICATE_FIXTURE",
                format!("aggregate evidence contains more than one {fixture} manifest"),
            ));
        }
    }
    for fixture in AGGREGATE_FIXTURES {
        if !sources.contains_key(*fixture) {
            return Err(AggregateFailure::new(
                "AGGREGATE_FIXTURE_MISSING",
                format!("aggregate evidence is missing the required {fixture} manifest"),
            ));
        }
    }
    if sources.len() != AGGREGATE_FIXTURES.len() {
        return Err(AggregateFailure::new(
            "AGGREGATE_DUPLICATE_FIXTURE",
            "aggregate evidence must contain exactly one clean and one reading-queue manifest",
        ));
    }
    if sources
        .values()
        .map(|source| &source.input_digest)
        .collect::<BTreeSet<_>>()
        .len()
        != AGGREGATE_FIXTURES.len()
    {
        return Err(AggregateFailure::new(
            "AGGREGATE_IDENTITY_MISMATCH",
            "clean and reading-queue evidence must identify independent Workspace inputs",
        ));
    }
    Ok(AGGREGATE_FIXTURES
        .iter()
        .map(|fixture| sources.remove(*fixture).expect("required fixture exists"))
        .collect())
}

fn verify_aggregate_source(
    manifest_path: &Path,
    catalog: &[u8],
    catalog_digest: &str,
    executor_digest: &str,
) -> std::result::Result<(String, AggregateSource), AggregateFailure> {
    verify_uploaded_path_ancestors(manifest_path, "AGGREGATE_EVIDENCE_MISSING")?;
    let metadata = fs::symlink_metadata(manifest_path).map_err(|error| {
        AggregateFailure::new(
            "AGGREGATE_EVIDENCE_MISSING",
            format!(
                "inspect source manifest '{}': {error}",
                manifest_path.display()
            ),
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(AggregateFailure::new(
            "AGGREGATE_EVIDENCE_MISSING",
            format!(
                "source manifest '{}' must be a regular uploaded file",
                manifest_path.display()
            ),
        ));
    }
    if manifest_path.file_name().and_then(OsStr::to_str) != Some("manifest.json") {
        return Err(AggregateFailure::new(
            "AGGREGATE_EVIDENCE_MALFORMED",
            format!(
                "source evidence path '{}' must name manifest.json",
                manifest_path.display()
            ),
        ));
    }
    let manifest_bytes = fs::read(manifest_path).map_err(|error| {
        AggregateFailure::new(
            "AGGREGATE_EVIDENCE_MISSING",
            format!(
                "read source manifest '{}': {error}",
                manifest_path.display()
            ),
        )
    })?;
    let manifest: CheckManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        AggregateFailure::new(
            "AGGREGATE_EVIDENCE_MALFORMED",
            format!(
                "parse source manifest '{}': {error}",
                manifest_path.display()
            ),
        )
    })?;
    if manifest.schema_version != RESULT_SCHEMA_VERSION
        || manifest.rule_schema_version != RESULT_SCHEMA_VERSION
        || manifest.distribution_version != DISTRIBUTION_VERSION
        || manifest.cli_version != DISTRIBUTION_VERSION
        || manifest.executor_digest != executor_digest
    {
        return Err(AggregateFailure::new(
            "AGGREGATE_IDENTITY_MISMATCH",
            format!(
                "source manifest '{}' does not name this exact Distribution and evidence schema",
                manifest_path.display()
            ),
        ));
    }
    if manifest.catalog_digest != catalog_digest
        || manifest.diagnostic_vocabulary != diagnostic_vocabulary()
        || manifest.not_evaluated != not_evaluated()
        || manifest.retry_policy != retry_policy()
        || manifest.required_tool_versions != required_tool_versions()
    {
        return Err(AggregateFailure::new(
            "AGGREGATE_CATALOG_MISMATCH",
            format!(
                "source manifest '{}' does not match the exact current catalog",
                manifest_path.display()
            ),
        ));
    }
    if manifest.exception_policy != exception_policy() || !manifest.exceptions.is_empty() {
        return Err(AggregateFailure::new(
            "AGGREGATE_EXCEPTION_REJECTED",
            format!(
                "source manifest '{}' contains an unknown or unsupported exception",
                manifest_path.display()
            ),
        ));
    }
    if !AGGREGATE_FIXTURES.contains(&manifest.fixture.as_str()) {
        return Err(AggregateFailure::new(
            "AGGREGATE_FIXTURE_MISSING",
            format!(
                "source manifest '{}' has unsupported fixture {:?}",
                manifest_path.display(),
                manifest.fixture
            ),
        ));
    }
    if manifest.profile != "clean-core-local"
        || manifest.scope != "clean-core-local"
        || !manifest.complete
        || manifest.aggregate_conformance
        || manifest.status != "pass-core"
        || !manifest.selected_nodes.is_empty()
    {
        return Err(AggregateFailure::new(
            "AGGREGATE_EVIDENCE_INCOMPLETE",
            format!(
                "source manifest '{}' is selected, failed, skipped, not-run, or otherwise incomplete",
                manifest_path.display()
            ),
        ));
    }
    if manifest.observed_tool_versions != observed_tool_versions(&manifest.nodes) {
        return Err(AggregateFailure::new(
            "AGGREGATE_IDENTITY_MISMATCH",
            format!(
                "source manifest '{}' does not preserve its exact observed tool identities",
                manifest_path.display()
            ),
        ));
    }
    if required_tool_versions()
        .iter()
        .any(|(name, version)| manifest.observed_tool_versions.get(name) != Some(version))
    {
        return Err(AggregateFailure::new(
            "AGGREGATE_IDENTITY_MISMATCH",
            format!(
                "source manifest '{}' did not observe every exact required tool identity",
                manifest_path.display()
            ),
        ));
    }
    let expected_nodes = NODE_SPECS
        .iter()
        .map(|spec| spec.id.to_owned())
        .collect::<Vec<_>>();
    if manifest.catalog_nodes != expected_nodes || manifest.nodes.len() != NODE_SPECS.len() {
        return Err(AggregateFailure::new(
            "AGGREGATE_NODE_SET_INVALID",
            format!(
                "source manifest '{}' is missing or adds required catalog nodes",
                manifest_path.display()
            ),
        ));
    }
    for (node, spec) in manifest.nodes.iter().zip(NODE_SPECS) {
        let max_attempts = if infrastructure_establishment_node(spec.id) {
            retry_policy().infrastructure_establishment_max_attempts
        } else {
            1
        };
        let attempts_valid = !node.attempts.is_empty()
            && node.attempts.len() <= usize::from(max_attempts)
            && node.attempts.iter().enumerate().all(|(index, attempt)| {
                attempt.attempt == u8::try_from(index + 1).unwrap_or(u8::MAX)
                    && if index + 1 == node.attempts.len() {
                        attempt.outcome == Outcome::Pass && attempt.cause.is_none()
                    } else {
                        attempt.outcome == Outcome::InfrastructureError && attempt.cause.is_some()
                    }
            });
        if node.schema_version != RESULT_SCHEMA_VERSION
            || node.event != "check-node"
            || node.node_id != spec.id
            || node.prerequisites
                != spec
                    .prerequisites
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            || node.outcome != Outcome::Pass
            || node.cause.is_some()
            || node.remediation.is_some()
            || node.proves != spec.proves
            || node.does_not_prove != spec.does_not_prove
            || !attempts_valid
            || node.log != format!("logs/{}.log", spec.id)
        {
            return Err(AggregateFailure::new(
                "AGGREGATE_EVIDENCE_INCOMPLETE",
                format!(
                    "source manifest '{}' has incomplete or mismatched node {}",
                    manifest_path.display(),
                    spec.id
                ),
            ));
        }
    }
    if !is_sha256_identity(&manifest.input_digest) {
        return Err(AggregateFailure::new(
            "AGGREGATE_IDENTITY_MISMATCH",
            format!(
                "source manifest '{}' has an invalid input digest",
                manifest_path.display()
            ),
        ));
    }
    let evidence_root = manifest_path.parent().ok_or_else(|| {
        AggregateFailure::new(
            "AGGREGATE_EVIDENCE_MALFORMED",
            "source manifest has no evidence root",
        )
    })?;
    let source_catalog_path = evidence_root.join("artifacts/check-catalog.json");
    verify_uploaded_tree(&source_catalog_path, "AGGREGATE_CATALOG_MISMATCH")?;
    let source_catalog = fs::read(&source_catalog_path).map_err(|error| {
        AggregateFailure::new(
            "AGGREGATE_CATALOG_MISMATCH",
            format!("read uploaded catalog: {error}"),
        )
    })?;
    if source_catalog != catalog {
        return Err(AggregateFailure::new(
            "AGGREGATE_CATALOG_MISMATCH",
            format!(
                "source manifest '{}' uploaded a different catalog",
                manifest_path.display()
            ),
        ));
    }
    verify_source_diagnostics(evidence_root, &manifest)?;
    verify_source_artifacts(evidence_root, &manifest.artifacts)?;
    Ok((
        manifest.fixture.clone(),
        AggregateSource {
            fixture: manifest.fixture,
            manifest: manifest_path.display().to_string(),
            manifest_sha256: sha256_identity(&manifest_bytes),
            executor_digest: manifest.executor_digest,
            input_digest: manifest.input_digest,
            artifacts: manifest.artifacts,
        },
    ))
}

fn verify_source_diagnostics(
    evidence_root: &Path,
    manifest: &CheckManifest,
) -> std::result::Result<(), AggregateFailure> {
    let diagnostics_path = evidence_root.join("diagnostics.jsonl");
    verify_uploaded_tree(&diagnostics_path, "AGGREGATE_DIAGNOSTICS_INVALID")?;
    let bytes = fs::read(&diagnostics_path).map_err(|error| {
        AggregateFailure::new(
            "AGGREGATE_DIAGNOSTICS_INVALID",
            format!("read uploaded JSON Lines diagnostics: {error}"),
        )
    })?;
    let lines = String::from_utf8(bytes)
        .map_err(|error| AggregateFailure::new("AGGREGATE_DIAGNOSTICS_INVALID", error.to_string()))?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if lines.len() != manifest.nodes.len() + 1 {
        return Err(AggregateFailure::new(
            "AGGREGATE_DIAGNOSTICS_INVALID",
            "uploaded diagnostics do not contain every node and one summary",
        ));
    }
    for (line, expected) in lines.iter().zip(&manifest.nodes) {
        let actual: NodeResult = serde_json::from_str(line).map_err(|error| {
            AggregateFailure::new(
                "AGGREGATE_DIAGNOSTICS_INVALID",
                format!("parse uploaded node diagnostic: {error}"),
            )
        })?;
        if &actual != expected {
            return Err(AggregateFailure::new(
                "AGGREGATE_DIAGNOSTICS_INVALID",
                format!(
                    "uploaded diagnostic for {} differs from manifest",
                    expected.node_id
                ),
            ));
        }
    }
    let summary: SummaryEvent = serde_json::from_str(
        lines.last().expect("diagnostic length was checked above"),
    )
    .map_err(|error| {
        AggregateFailure::new(
            "AGGREGATE_DIAGNOSTICS_INVALID",
            format!("parse uploaded summary diagnostic: {error}"),
        )
    })?;
    if summary != summary_event("pass-core", "clean-core-local", true, false) {
        return Err(AggregateFailure::new(
            "AGGREGATE_DIAGNOSTICS_INVALID",
            "uploaded summary is malformed or does not match complete local evidence",
        ));
    }
    Ok(())
}

fn verify_source_artifacts(
    evidence_root: &Path,
    artifacts: &[ArtifactEvidence],
) -> std::result::Result<(), AggregateFailure> {
    let expected_paths = ["logs", "artifacts", "diagnostics.jsonl"];
    if artifacts.len() != expected_paths.len() {
        return Err(AggregateFailure::new(
            "AGGREGATE_ARTIFACT_INVALID",
            "uploaded evidence must identify logs, artifacts, and diagnostics.jsonl exactly once",
        ));
    }
    let mut seen = BTreeSet::new();
    for artifact in artifacts {
        if !expected_paths.contains(&artifact.path.as_str()) || !seen.insert(&artifact.path) {
            return Err(AggregateFailure::new(
                "AGGREGATE_ARTIFACT_INVALID",
                format!("unsupported or duplicate artifact path {:?}", artifact.path),
            ));
        }
        let artifact_path = evidence_root.join(&artifact.path);
        verify_uploaded_tree(&artifact_path, "AGGREGATE_ARTIFACT_INVALID")?;
        let actual = artifact_evidence(evidence_root, &artifact_path).map_err(|error| {
            AggregateFailure::new(
                "AGGREGATE_ARTIFACT_INVALID",
                format!("verify uploaded artifact {:?}: {error:#}", artifact.path),
            )
        })?;
        if actual != *artifact {
            return Err(AggregateFailure::new(
                "AGGREGATE_ARTIFACT_INVALID",
                format!(
                    "uploaded artifact {:?} digest does not match",
                    artifact.path
                ),
            ));
        }
    }
    for node in NODE_SPECS {
        if !evidence_root
            .join(format!("logs/{}.log", node.id))
            .is_file()
        {
            return Err(AggregateFailure::new(
                "AGGREGATE_ARTIFACT_INVALID",
                format!("uploaded raw log for {} is missing", node.id),
            ));
        }
    }
    Ok(())
}

fn verify_uploaded_path_ancestors(
    path: &Path,
    code: &'static str,
) -> std::result::Result<(), AggregateFailure> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| AggregateFailure::new(code, error.to_string()))?
            .join(path)
    };
    for ancestor in absolute.ancestors().collect::<Vec<_>>().into_iter().rev() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|error| {
            AggregateFailure::new(
                code,
                format!(
                    "inspect uploaded evidence path '{}': {error}",
                    ancestor.display()
                ),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(AggregateFailure::new(
                code,
                format!(
                    "uploaded evidence path '{}' must not contain symlinks",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn verify_uploaded_tree(
    path: &Path,
    code: &'static str,
) -> std::result::Result<(), AggregateFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        AggregateFailure::new(
            code,
            format!(
                "inspect uploaded evidence path '{}': {error}",
                path.display()
            ),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(AggregateFailure::new(
            code,
            format!(
                "uploaded evidence path '{}' must not be a symlink",
                path.display()
            ),
        ));
    }
    if metadata.is_file() {
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(AggregateFailure::new(
            code,
            format!(
                "uploaded evidence path '{}' has an unsupported file type",
                path.display()
            ),
        ));
    }
    let mut children = fs::read_dir(path)
        .map_err(|error| AggregateFailure::new(code, error.to_string()))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| AggregateFailure::new(code, error.to_string()))?;
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        verify_uploaded_tree(&child.path(), code)?;
    }
    Ok(())
}

fn is_sha256_identity(value: &str) -> bool {
    value.len() == "sha256:".len() + 64
        && value.starts_with("sha256:")
        && value["sha256:".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
        event: "check-node".to_owned(),
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome: Outcome::Skipped,
        duration_ms: 0,
        attempts: Vec::new(),
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
        event: "check-node".to_owned(),
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome: Outcome::NotRun,
        duration_ms: 0,
        attempts: Vec::new(),
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
    fixture: &'a str,
    catalog_digest: &'a str,
    executor_digest: &'a str,
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
        fixture,
        catalog_digest,
        executor_digest,
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
    let manifest_path = evidence_root.join("manifest.json");
    let summary = summary_event("fail", "clean-core-local", complete, false);
    serde_json::to_writer(&mut *diagnostics, &summary)?;
    diagnostics.write_all(b"\n")?;
    diagnostics.flush()?;
    let artifacts = vec![
        artifact_evidence(evidence_root, &evidence_root.join("logs"))?,
        artifact_evidence(evidence_root, &evidence_root.join("artifacts"))?,
        artifact_evidence(evidence_root, &diagnostics_path)?,
    ];
    let manifest = CheckManifest {
        schema_version: RESULT_SCHEMA_VERSION,
        distribution_version: DISTRIBUTION_VERSION.to_owned(),
        cli_version: DISTRIBUTION_VERSION.to_owned(),
        executor_digest: executor_digest.to_owned(),
        rule_schema_version: RESULT_SCHEMA_VERSION,
        workspace: root.display().to_string(),
        fixture: fixture.to_owned(),
        profile: "clean-core-local".to_owned(),
        scope: "clean-core-local".to_owned(),
        catalog_digest: catalog_digest.to_owned(),
        diagnostic_vocabulary: diagnostic_vocabulary(),
        not_evaluated: not_evaluated(),
        exception_policy: exception_policy(),
        retry_policy: retry_policy(),
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
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    let mut manifest_file = create_private_file(&manifest_path)?;
    manifest_file.write_all(&bytes)?;
    manifest_file.flush()?;
    emit_summary(&summary, &manifest_path, format);
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
        event: "check-node".to_owned(),
        node_id: spec.id.to_owned(),
        prerequisites: Vec::new(),
        outcome: failure.outcome,
        duration_ms: 0,
        attempts: Vec::new(),
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
        event: "check-node".to_owned(),
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome: Outcome::NotRun,
        duration_ms: 0,
        attempts: Vec::new(),
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
    comparison_base: Option<&str>,
    fixture: &str,
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
    let max_attempts = if infrastructure_establishment_node(spec.id) {
        retry_policy().infrastructure_establishment_max_attempts
    } else {
        1
    };
    let mut attempts = Vec::new();
    let execution = loop {
        let attempt = u8::try_from(attempts.len() + 1).unwrap_or(u8::MAX);
        writeln!(context.log, "attempt {attempt}/{max_attempts}")?;
        context.log.flush()?;
        let attempt_started = Instant::now();
        let execution =
            execute_node_attempt(spec, &mut context, baselines, comparison_base, fixture);
        let attempt_duration_ms =
            u64::try_from(attempt_started.elapsed().as_millis()).unwrap_or(u64::MAX);
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
        let (attempt_outcome, attempt_cause) = match &execution {
            Ok(()) => (Outcome::Pass, None),
            Err(failure) => (
                failure.outcome.clone(),
                Some(CheckCause {
                    code: failure.code.to_owned(),
                    message: failure.message.clone(),
                    dependency_node_id: None,
                }),
            ),
        };
        let retry = attempt < max_attempts && attempt_outcome == Outcome::InfrastructureError;
        attempts.push(AttemptResult {
            attempt,
            outcome: attempt_outcome,
            duration_ms: attempt_duration_ms,
            cause: attempt_cause,
        });
        if retry {
            writeln!(context.log, "retrying infrastructure establishment once")?;
            context.log.flush()?;
        } else {
            break execution;
        }
    };
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
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
        event: "check-node".to_owned(),
        node_id: spec.id.to_owned(),
        prerequisites: spec
            .prerequisites
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        outcome,
        duration_ms,
        attempts,
        cause,
        remediation,
        proves: spec.proves.to_owned(),
        does_not_prove: spec.does_not_prove.to_owned(),
        commands: context.commands,
        tool_versions: context.tool_versions,
        log: relative_log(&log_path, evidence_root),
    })
}

fn infrastructure_establishment_node(node_id: &str) -> bool {
    matches!(
        node_id,
        "infrastructure.docker" | "infrastructure.playwright-chromium"
    )
}

fn check_fixture_identity(root: &Path, fixture: &str) -> std::result::Result<(), NodeFailure> {
    if fixture == "unclassified" {
        return Ok(());
    }
    let expected = FIXTURE_SPECS
        .iter()
        .find(|candidate| candidate.id == fixture)
        .ok_or_else(|| {
            NodeFailure::fail(
                "FIXTURE_IDENTITY_MISMATCH",
                format!("fixture {fixture:?} has no exact catalog-owned definition"),
            )
        })?;
    let origin = read_workspace_origin_record(root).map_err(|error| {
        NodeFailure::fail(
            "FIXTURE_IDENTITY_MISMATCH",
            format!("read named fixture Workspace Origin Record: {error:#}"),
        )
    })?;
    if origin.product_name != expected.product_name
        || origin.product_id != expected.product_id
        || origin.product_source_license != expected.product_source_license
    {
        return Err(NodeFailure::fail(
            "FIXTURE_IDENTITY_MISMATCH",
            format!(
                "fixture {fixture:?} requires exact creation inputs product_name={:?}, product_id={:?}, product_source_license={:?}; found product_name={:?}, product_id={:?}, product_source_license={:?}",
                expected.product_name,
                expected.product_id,
                expected.product_source_license,
                origin.product_name,
                origin.product_id,
                origin.product_source_license,
            ),
        ));
    }
    Ok(())
}

fn execute_node_attempt(
    spec: NodeSpec,
    context: &mut NodeContext<'_>,
    baselines: &InputBaselines<'_>,
    comparison_base: Option<&str>,
    fixture: &str,
) -> std::result::Result<(), NodeFailure> {
    let root = context.root;
    match spec.id {
        "policy.exceptions" => check_exception_policy(root),
        "origin.exact-distribution" => verify_origin_authority(root)
            .map_err(|error| NodeFailure::fail("ORIGIN_AUTHORITY_DRIFT", format!("{error:#}")))
            .and_then(|()| check_fixture_identity(root, fixture)),
        "ownership.baseline-skills" => check_baseline_skills(root),
        "ownership.generated-snapshots" => verify_snapshot_authorities(root).map_err(|error| {
            NodeFailure::fail("GENERATED_SNAPSHOT_DRIFT", format!("{error:#}"))
        }),
        "database.migration-history" => {
            check_migration_history(context, baselines.original_root, comparison_base)
        }
        "rust.architecture" => check_rust_architecture(context),
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
        "rust.test" => check_rust_tests(context),
        "rust.doctest" => context.command(
            root,
            "cargo",
            &["test", "--locked", "--workspace", "--all-features", "--doc"],
            &[],
            "RUST_DOCTEST_FAILED",
        ),
        "frontend.lock" => check_frontend_lock(context),
        "api.generated-contract" => check_api_generated_contract(context),
        "api.runtime-conformance" => check_api_runtime_conformance(context),
        "frontend.format" => check_frontend_format(context),
        "frontend.lint" => check_frontend_lint(context),
        "frontend.typecheck" => check_frontend_typecheck(context),
        "frontend.test" => check_frontend_tests(context),
        "native.android-generation" => check_android_generation(context),
        "android.release" => check_android_release(context),
        "server.release" => check_server_release(context),
        "api.client-contract" => check_api_client_contract(context),
        "infrastructure.docker" => check_docker(context),
        "database.runtime-invariants" => check_database_runtime_invariants(context),
        "runtime.post-commit-executor" => check_post_commit_executor(context),
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
        "h5.product-presentation-accessibility" => {
            check_product_presentation_accessibility(context)
        }
        "h5.real-runtime" => check_h5_runtime(context),
        INPUTS_UNCHANGED_NODE => check_and_remove_scratch(root, baselines),
        _ => unreachable!("all node specs have an implementation"),
    }
}

fn check_exception_policy(root: &Path) -> std::result::Result<(), NodeFailure> {
    let path = root.join(".yydra/check-exceptions.toml");
    if path.exists() {
        return Err(NodeFailure::fail(
            "CHECK_EXCEPTION_POLICY_VIOLATION",
            "this exact Distribution has a deny-all exception policy; .yydra/check-exceptions.toml is unsupported",
        ));
    }
    Ok(())
}

impl NodeContext<'_> {
    fn observe_tool_version(
        &mut self,
        directory: &Path,
        name: &str,
        program: &str,
        arguments: &[&str],
    ) -> std::result::Result<String, NodeFailure> {
        self.observe_tool_version_with_environment(directory, name, program, arguments, &[])
    }

    fn observe_tool_version_with_environment(
        &mut self,
        directory: &Path,
        name: &str,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> std::result::Result<String, NodeFailure> {
        let output = self.capture(directory, program, arguments, environment)?;
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
            // Every check subprocess, including API generation and npm hooks,
            // builds inside the isolated scratch Workspace.
            .env("CARGO_TARGET_DIR", self.root.join("target"))
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

fn check_migration_history(
    context: &mut NodeContext<'_>,
    original_root: &Path,
    comparison_base: Option<&str>,
) -> std::result::Result<(), NodeFailure> {
    let migrations = context.root.join("migrations");
    let entries = fs::read_dir(&migrations).map_err(|error| {
        NodeFailure::fail(
            "DB_MIGRATION_HISTORY_MISSING",
            format!("read '{}': {error}", migrations.display()),
        )
    })?;
    let mut versions = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            NodeFailure::fail("DB_MIGRATION_HISTORY_INVALID", error.to_string())
        })?;
        let file_type = entry.file_type().map_err(|error| {
            NodeFailure::fail("DB_MIGRATION_HISTORY_INVALID", error.to_string())
        })?;
        let name = entry.file_name().into_string().map_err(|_| {
            NodeFailure::fail(
                "DB_MIGRATION_HISTORY_INVALID",
                "migration filenames must be UTF-8",
            )
        })?;
        if !file_type.is_file() || !name.ends_with(".sql") {
            return Err(NodeFailure::fail(
                "DB_MIGRATION_HISTORY_INVALID",
                format!("migration authority contains non-SQL entry {name:?}"),
            ));
        }
        let Some((version, description)) = name.split_once('_') else {
            return Err(NodeFailure::fail(
                "DB_MIGRATION_HISTORY_INVALID",
                format!("migration {name:?} must use <version>_<description>.sql"),
            ));
        };
        let version = version.parse::<i64>().map_err(|_| {
            NodeFailure::fail(
                "DB_MIGRATION_HISTORY_INVALID",
                format!("migration {name:?} has an invalid version"),
            )
        })?;
        if version <= 0 || description == ".sql" {
            return Err(NodeFailure::fail(
                "DB_MIGRATION_HISTORY_INVALID",
                format!("migration {name:?} has an invalid version or description"),
            ));
        }
        if let Some(existing) = versions.insert(version, name.clone()) {
            return Err(NodeFailure::fail(
                "DB_MIGRATION_HISTORY_INVALID",
                format!("migrations {existing:?} and {name:?} reuse version {version}"),
            ));
        }
    }
    if versions.is_empty() {
        return Err(NodeFailure::fail(
            "DB_MIGRATION_HISTORY_MISSING",
            "the Product Workspace migration authority is empty",
        ));
    }

    let mut migrator_authorities = Vec::new();
    collect_migrator_authorities(
        &context.root.join("crates"),
        context.root,
        &mut migrator_authorities,
    )?;
    if migrator_authorities != [PathBuf::from("crates/persistence-postgres/src/lib.rs")] {
        return Err(NodeFailure::fail(
            "DB_MIGRATION_AUTHORITY_AMBIGUOUS",
            format!(
                "expected one root SQLx migrator in persistence-postgres, found {migrator_authorities:?}"
            ),
        ));
    }

    for (path, expected) in template_source_files()
        .into_iter()
        .filter(|(path, _)| path.starts_with("migrations/") && path.ends_with(".sql"))
    {
        let actual_path = context.root.join(&path);
        let actual = match fs::read(&actual_path) {
            Ok(actual) => actual,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(NodeFailure::fail(
                    "DB_MIGRATION_DISTRIBUTION_BASE_DELETED",
                    format!("exact-Distribution migration {path:?} was deleted"),
                ));
            }
            Err(error) => {
                return Err(NodeFailure::fail(
                    "DB_MIGRATION_HISTORY_INVALID",
                    format!("read '{}': {error}", actual_path.display()),
                ));
            }
        };
        if actual != expected {
            return Err(NodeFailure::fail(
                "DB_MIGRATION_DISTRIBUTION_BASE_MUTATED",
                format!("exact-Distribution migration {path:?} was edited"),
            ));
        }
    }

    let Some(comparison_base) = comparison_base else {
        return Ok(());
    };
    if comparison_base.is_empty()
        || comparison_base.len() > 256
        || comparison_base.starts_with('-')
        || comparison_base.chars().any(char::is_control)
    {
        return Err(NodeFailure::fail(
            "DB_MIGRATION_COMPARISON_BASE_INVALID",
            "comparison base must be a non-option Git revision of at most 256 characters",
        ));
    }
    let commit = format!("{comparison_base}^{{commit}}");
    let resolved = context.capture(
        original_root,
        "git",
        &["rev-parse", "--verify", "--quiet", &commit],
        &[],
    )?;
    if !resolved.status.success() {
        return Err(NodeFailure::fail(
            "DB_MIGRATION_COMPARISON_BASE_UNAVAILABLE",
            format!("Git revision {comparison_base:?} does not resolve to a commit"),
        ));
    }
    let diff = context.capture(
        original_root,
        "git",
        &[
            "diff",
            "--relative",
            "--name-status",
            "--no-renames",
            comparison_base,
            "--",
            "migrations",
        ],
        &[],
    )?;
    if !diff.status.success() {
        return Err(NodeFailure::fail(
            "DB_MIGRATION_COMPARISON_FAILED",
            format!("could not compare migrations with Git revision {comparison_base:?}"),
        ));
    }
    let changes = String::from_utf8(diff.stdout)
        .map_err(|error| NodeFailure::fail("DB_MIGRATION_COMPARISON_FAILED", error.to_string()))?;
    for line in changes.lines() {
        let Some((status, path)) = line.split_once('\t') else {
            return Err(NodeFailure::fail(
                "DB_MIGRATION_COMPARISON_FAILED",
                format!("Git reported malformed migration change {line:?}"),
            ));
        };
        if status == "A" {
            continue;
        }
        let (code, action) = if status == "D" {
            ("DB_MIGRATION_COMPARISON_BASE_DELETED", "deleted")
        } else {
            ("DB_MIGRATION_COMPARISON_BASE_MUTATED", "edited")
        };
        return Err(NodeFailure::fail(
            code,
            format!("comparison-base migration {path:?} was {action}; add a new migration instead"),
        ));
    }
    Ok(())
}

fn collect_migrator_authorities(
    directory: &Path,
    root: &Path,
    authorities: &mut Vec<PathBuf>,
) -> std::result::Result<(), NodeFailure> {
    for entry in fs::read_dir(directory).map_err(|error| {
        NodeFailure::fail(
            "DB_MIGRATION_AUTHORITY_AMBIGUOUS",
            format!("read '{}': {error}", directory.display()),
        )
    })? {
        let entry = entry.map_err(|error| {
            NodeFailure::fail("DB_MIGRATION_AUTHORITY_AMBIGUOUS", error.to_string())
        })?;
        let file_type = entry.file_type().map_err(|error| {
            NodeFailure::fail("DB_MIGRATION_AUTHORITY_AMBIGUOUS", error.to_string())
        })?;
        if file_type.is_dir() {
            collect_migrator_authorities(&entry.path(), root, authorities)?;
        } else if file_type.is_file()
            && entry.path().extension() == Some(OsStr::new("rs"))
            && fs::read_to_string(entry.path())
                .map_err(|error| {
                    NodeFailure::fail("DB_MIGRATION_AUTHORITY_AMBIGUOUS", error.to_string())
                })?
                .contains("sqlx::migrate!")
        {
            authorities.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .expect("migration source is below the Workspace")
                    .to_path_buf(),
            );
        }
    }
    authorities.sort();
    Ok(())
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
                && !(role == "api-build"
                    && dependency.name == "yydra-build"
                    && edge_kind == "build")
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
                if !allowed_workspace_edge(role, dependency_role)
                    || (role == "api-build" && edge_kind != "build")
                {
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
        "domain"
        | "application"
        | "persistence-postgres"
        | "transport-http"
        | "server"
        | "api-build" => Ok(role),
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
        "api-build" => dependency != "yydra-build" && !dependency.ends_with("-transport-http"),
        _ => true,
    }
}

fn allowed_workspace_edge(from: &str, to: &str) -> bool {
    match from {
        "domain" => false,
        "application" => matches!(to, "domain" | "persistence-postgres"),
        "persistence-postgres" => to == "domain",
        "transport-http" => to == "application",
        "api-build" => to == "transport-http",
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
            "--",
            "--test-threads=1",
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
        &["ci", "--ignore-scripts", "--no-audit"],
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
        ("@react-native-community/netinfo", "12.0.1"),
        ("@playwright/test", "1.62.1"),
        ("@testing-library/dom", "10.4.1"),
        ("@testing-library/react", "16.3.3"),
        ("@types/react-dom", "19.2.5"),
        ("@eslint/js", "10.0.1"),
        ("eslint", "10.9.1"),
        ("jsdom", "30.0.1"),
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
    let output = context.capture(
        &context.root.join("frontend"),
        "node",
        &["scripts/prepare-api.mjs"],
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
        "API_CLIENT_OUTPUT_INVALID",
        "API_CLIENT_TYPECHECK_FAILED",
        "API_CLIENT_GENERATION_FAILED",
        "API_CLIENT_TOOL_VERSION_INVALID",
        "API_CLIENT_LINK_FAILED",
        "API_OUTPUT_MISSING",
        "API_OUTPUT_PREPARE_FAILED",
        "API_WORKSPACE_INVALID",
        "API_OPENAPI_EXPORT_FAILED",
        "API_BUILD_FAILED",
    ]
    .into_iter()
    .find(|code| detail.contains(code))
    .unwrap_or("API_GENERATED_CONTRACT_FAILED");
    Err(NodeFailure::fail(
        code,
        "API build preparation rejected the current contract or generated client",
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
        if relative.starts_with("src/framework/api") {
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
        if !specifier.contains("generated/public-api")
            && !specifier.contains("@yydra/generated-api")
        {
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
        ("dependencies", "@react-native-community/netinfo", "12.0.1"),
        ("devDependencies", "@playwright/test", "1.62.1"),
        ("devDependencies", "@testing-library/dom", "10.4.1"),
        ("devDependencies", "@testing-library/react", "16.3.3"),
        ("devDependencies", "@types/react-dom", "19.2.5"),
        ("devDependencies", "@eslint/js", "10.0.1"),
        ("devDependencies", "eslint", "10.9.1"),
        ("devDependencies", "jsdom", "30.0.1"),
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
    "modules",
    "src",
    "scripts",
    "app.json",
    "eslint.config.mjs",
    "metro.config.mjs",
    "orval.config.mjs",
    "package.json",
    "playwright.config.mts",
    "tsconfig.json",
    "vitest.config.mts",
];

const FRONTEND_LINT_SOURCES: &[&str] = &[
    "app",
    "e2e",
    "modules",
    "src",
    "scripts",
    "eslint.config.mjs",
    "metro.config.mjs",
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
  resolve: {
    preserveSymlinks: true,
    alias: {
      "@": fileURLToPath(new URL("../../src", import.meta.url)),
      "@react-native-community/netinfo": fileURLToPath(
        new URL("../../src/framework/testing/netinfo.ts", import.meta.url),
      ),
      "react-native": "react-native-web",
    },
  },
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeInventoryEntry {
    path: String,
    kind: &'static str,
    mode: String,
    sha256: String,
}

struct AccountFreeAndroidEnvironment {
    home: String,
    xdg_config_home: String,
    xdg_cache_home: String,
    npm_cache: String,
    npm_user_config: String,
    gradle_user_home: String,
    gradle_opts: String,
}

impl AccountFreeAndroidEnvironment {
    fn prepare(evidence_root: &Path) -> std::result::Result<Self, NodeFailure> {
        let root = evidence_root.join("scratch/android-account-free");
        let home = root.join("home");
        let xdg_config_home = root.join("xdg-config");
        let xdg_cache_home = root.join("xdg-cache");
        let npm_cache = root.join("npm-cache");
        let gradle_user_home = root.join("gradle-user-home");
        let maven_local = home.join(".m2/repository");
        for directory in [
            &home,
            &xdg_config_home,
            &xdg_cache_home,
            &npm_cache,
            &gradle_user_home,
            &maven_local,
        ] {
            create_private_dir_all(directory).map_err(evidence_write_failure)?;
        }
        seed_gradle_dependency_cache(&gradle_user_home)?;
        Ok(Self {
            npm_user_config: root.join("empty-npmrc").display().to_string(),
            home: home.display().to_string(),
            xdg_config_home: xdg_config_home.display().to_string(),
            xdg_cache_home: xdg_cache_home.display().to_string(),
            npm_cache: npm_cache.display().to_string(),
            gradle_user_home: gradle_user_home.display().to_string(),
            gradle_opts: account_free_gradle_options(
                std::env::var("HTTPS_PROXY")
                    .or_else(|_| std::env::var("https_proxy"))
                    .ok()
                    .as_deref(),
            ),
        })
    }

    fn expo(&self) -> [(&str, &str); 7] {
        [
            ("CI", "1"),
            ("EXPO_NO_TELEMETRY", "1"),
            ("HOME", &self.home),
            ("XDG_CONFIG_HOME", &self.xdg_config_home),
            ("XDG_CACHE_HOME", &self.xdg_cache_home),
            ("NPM_CONFIG_CACHE", &self.npm_cache),
            ("NPM_CONFIG_USERCONFIG", &self.npm_user_config),
        ]
    }

    fn gradle(&self) -> [(&str, &str); 8] {
        [
            ("CI", "1"),
            ("CMAKE_BUILD_PARALLEL_LEVEL", "1"),
            ("NODE_ENV", "production"),
            ("GRADLE_OPTS", &self.gradle_opts),
            ("HOME", &self.home),
            ("XDG_CONFIG_HOME", &self.xdg_config_home),
            ("XDG_CACHE_HOME", &self.xdg_cache_home),
            ("GRADLE_USER_HOME", &self.gradle_user_home),
        ]
    }
}

fn seed_gradle_dependency_cache(gradle_user_home: &Path) -> std::result::Result<(), NodeFailure> {
    let Some(seed) = std::env::var_os("YYDRA_GRADLE_DEPENDENCY_CACHE_SEED") else {
        return Ok(());
    };
    let seed = PathBuf::from(seed);
    if !seed.is_absolute() {
        return Err(NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "YYDRA_GRADLE_DEPENDENCY_CACHE_SEED must be an absolute path",
        ));
    }
    let source = seed.join("modules-2");
    let destination = gradle_user_home.join("caches/modules-2");
    let completion_marker = gradle_user_home
        .join("caches")
        .join(".yydra-modules-2-seed-complete");
    if destination.exists() {
        return if completion_marker.is_file() {
            Ok(())
        } else {
            Err(NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "isolated Gradle dependency cache '{}' exists without a completed seed copy",
                    destination.display()
                ),
            ))
        };
    }
    copy_gradle_dependency_cache_tree(&source, &destination)?;
    let mut marker = create_private_file(&completion_marker).map_err(evidence_write_failure)?;
    marker
        .write_all(b"complete\n")
        .map_err(evidence_write_failure)?;
    marker.flush().map_err(evidence_write_failure)
}

fn copy_gradle_dependency_cache_tree(
    source: &Path,
    destination: &Path,
) -> std::result::Result<(), NodeFailure> {
    preflight_gradle_dependency_cache_tree(source, GradleCacheSeedLimits::DEFAULT)?;
    let mut usage = GradleCacheSeedUsage::default();
    copy_validated_gradle_dependency_cache_tree(
        source,
        destination,
        GradleCacheSeedLimits::DEFAULT,
        &mut usage,
    )
}

#[derive(Clone, Copy)]
struct GradleCacheSeedLimits {
    max_entries: u64,
    max_files: u64,
    max_file_bytes: u64,
    max_total_bytes: u64,
}

impl GradleCacheSeedLimits {
    const DEFAULT: Self = Self {
        max_entries: 200_000,
        max_files: 100_000,
        max_file_bytes: 512 * 1024 * 1024,
        max_total_bytes: 4 * 1024 * 1024 * 1024,
    };
}

#[derive(Default)]
struct GradleCacheSeedUsage {
    entries: u64,
    files: u64,
    total_bytes: u64,
}

fn preflight_gradle_dependency_cache_tree(
    source: &Path,
    limits: GradleCacheSeedLimits,
) -> std::result::Result<(u64, u64), NodeFailure> {
    let mut pending = vec![source.to_path_buf()];
    let mut entries = 0_u64;
    let mut files = 0_u64;
    let mut total_bytes = 0_u64;
    while let Some(path) = pending.pop() {
        entries = entries.checked_add(1).ok_or_else(|| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed entry count overflow",
            )
        })?;
        if entries > limits.max_entries {
            return Err(NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed exceeds its bounded entry-count limit",
            ));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "inspect Gradle dependency cache seed '{}': {error}",
                    path.display()
                ),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "Gradle dependency cache seed '{}' contains a symlink",
                    path.display()
                ),
            ));
        }
        if metadata.is_dir() {
            let directory = fs::read_dir(&path).map_err(|error| {
                NodeFailure::infrastructure(
                    "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                    format!(
                        "read Gradle dependency cache seed '{}': {error}",
                        path.display()
                    ),
                )
            })?;
            let remaining = limits.max_entries.saturating_sub(entries);
            let mut children = Vec::new();
            for child in directory {
                if u64::try_from(children.len()).unwrap_or(u64::MAX) >= remaining {
                    return Err(NodeFailure::infrastructure(
                        "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                        "Gradle dependency cache seed exceeds its bounded entry-count limit",
                    ));
                }
                children.push(child.map_err(|error| {
                    NodeFailure::infrastructure(
                        "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                        format!(
                            "read Gradle dependency cache seed '{}': {error}",
                            path.display()
                        ),
                    )
                })?);
            }
            children.sort_by_key(std::fs::DirEntry::file_name);
            for child in children.into_iter().rev() {
                let name = child.file_name();
                if name == OsStr::new("gc.properties") || name.to_string_lossy().ends_with(".lock")
                {
                    return Err(NodeFailure::infrastructure(
                        "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                        format!(
                            "Gradle dependency cache seed '{}' must omit locks and gc.properties",
                            child.path().display()
                        ),
                    ));
                }
                pending.push(child.path());
            }
            continue;
        }
        if !metadata.is_file() {
            return Err(NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "Gradle dependency cache seed '{}' contains an unsupported file type",
                    path.display()
                ),
            ));
        }
        files = files.checked_add(1).ok_or_else(|| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed file count overflow",
            )
        })?;
        total_bytes = total_bytes.checked_add(metadata.len()).ok_or_else(|| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed byte count overflow",
            )
        })?;
        if files > limits.max_files
            || metadata.len() > limits.max_file_bytes
            || total_bytes > limits.max_total_bytes
        {
            return Err(NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "Gradle dependency cache seed exceeds its bounded copy budget (files {files}/{}, file bytes {}/{}, total bytes {total_bytes}/{})",
                    limits.max_files,
                    metadata.len(),
                    limits.max_file_bytes,
                    limits.max_total_bytes
                ),
            ));
        }
    }
    Ok((files, total_bytes))
}

fn copy_validated_gradle_dependency_cache_tree(
    source: &Path,
    destination: &Path,
    limits: GradleCacheSeedLimits,
    usage: &mut GradleCacheSeedUsage,
) -> std::result::Result<(), NodeFailure> {
    usage.entries = usage.entries.checked_add(1).ok_or_else(|| {
        NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed entry count overflow during copy",
        )
    })?;
    if usage.entries > limits.max_entries {
        return Err(NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed exceeds its bounded entry-count limit during copy",
        ));
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "inspect Gradle dependency cache seed '{}': {error}",
                source.display()
            ),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "Gradle dependency cache seed '{}' contains a symlink",
                source.display()
            ),
        ));
    }
    if metadata.is_dir() {
        create_private_dir_all(destination).map_err(evidence_write_failure)?;
        let directory = fs::read_dir(source).map_err(|error| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "read Gradle dependency cache seed '{}': {error}",
                    source.display()
                ),
            )
        })?;
        let remaining = limits.max_entries.saturating_sub(usage.entries);
        let mut children = Vec::new();
        for child in directory {
            if u64::try_from(children.len()).unwrap_or(u64::MAX) >= remaining {
                return Err(NodeFailure::infrastructure(
                    "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                    "Gradle dependency cache seed exceeds its bounded entry-count limit during copy",
                ));
            }
            children.push(child.map_err(|error| {
                NodeFailure::infrastructure(
                    "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                    format!(
                        "read Gradle dependency cache seed '{}': {error}",
                        source.display()
                    ),
                )
            })?);
        }
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let name = child.file_name();
            if name == OsStr::new("gc.properties") || name.to_string_lossy().ends_with(".lock") {
                return Err(NodeFailure::infrastructure(
                    "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                    format!(
                        "Gradle dependency cache seed '{}' must omit locks and gc.properties",
                        child.path().display()
                    ),
                ));
            }
            copy_validated_gradle_dependency_cache_tree(
                &child.path(),
                &destination.join(name),
                limits,
                usage,
            )?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "Gradle dependency cache seed '{}' contains an unsupported file type",
                source.display()
            ),
        ));
    }
    usage.files = usage.files.checked_add(1).ok_or_else(|| {
        NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed file count overflow during copy",
        )
    })?;
    if usage.files > limits.max_files || metadata.len() > limits.max_file_bytes {
        return Err(NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed exceeds its bounded file-count or per-file limit during copy",
        ));
    }
    let mut input = File::open(source).map_err(|error| {
        NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "open Gradle dependency cache seed '{}': {error}",
                source.display()
            ),
        )
    })?;
    let mut output = create_private_file(destination).map_err(evidence_write_failure)?;
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "read Gradle dependency cache seed '{}': {error}",
                    source.display()
                ),
            )
        })?;
        if read == 0 {
            break;
        }
        let read = u64::try_from(read).unwrap_or(u64::MAX);
        copied = copied.checked_add(read).ok_or_else(|| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed file byte count overflow during copy",
            )
        })?;
        usage.total_bytes = usage.total_bytes.checked_add(read).ok_or_else(|| {
            NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed total byte count overflow during copy",
            )
        })?;
        if copied > limits.max_file_bytes || usage.total_bytes > limits.max_total_bytes {
            return Err(NodeFailure::infrastructure(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed grew beyond its bounded byte budget during copy",
            ));
        }
        output
            .write_all(&buffer[..usize::try_from(read).unwrap_or(buffer.len())])
            .map_err(evidence_write_failure)?;
    }
    if copied != metadata.len() {
        return Err(NodeFailure::infrastructure(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed changed while it was copied",
        ));
    }
    output.flush().map_err(evidence_write_failure)
}

fn account_free_gradle_options(proxy: Option<&str>) -> String {
    let mut options = "-Dorg.gradle.jvmargs=-Xmx2g -Dorg.gradle.workers.max=1".to_owned();
    let Some(proxy) = proxy else {
        return options;
    };
    let Some(authority) = proxy
        .strip_prefix("http://")
        .or_else(|| proxy.strip_prefix("https://"))
        .and_then(|value| value.split('/').next())
    else {
        return options;
    };
    if authority.is_empty()
        || authority.contains('@')
        || authority.contains('?')
        || authority.contains('#')
    {
        return options;
    }
    let Some((host, port)) = authority.rsplit_once(':') else {
        return options;
    };
    if host.is_empty()
        || !host.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        })
        || port.parse::<u16>().ok().filter(|port| *port > 0).is_none()
    {
        return options;
    }
    options.push_str(&format!(
        " -Dhttps.proxyHost={host} -Dhttps.proxyPort={port} -Dhttp.proxyHost={host} -Dhttp.proxyPort={port}"
    ));
    options
}

fn check_android_generation(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    check_android_generation_inputs(context.root)?;
    let artifact_root = context
        .evidence_root
        .join("artifacts/native.android-generation");
    create_private_dir_all(&artifact_root).map_err(evidence_write_failure)?;
    let first = generate_android_inventory(context)?;
    write_native_inventory(&artifact_root.join("generation-1-inventory.json"), &first)?;
    let second = generate_android_inventory(context)?;
    write_native_inventory(&artifact_root.join("generation-2-inventory.json"), &second)?;
    if first != second {
        return Err(NodeFailure::fail(
            "NATIVE_GENERATION_NONDETERMINISTIC",
            describe_native_inventory_drift(&first, &second),
        ));
    }
    Ok(())
}

fn check_android_generation_inputs(root: &Path) -> std::result::Result<(), NodeFailure> {
    let frontend = root.join("frontend");
    for dynamic_config in [
        "app.config.js",
        "app.config.cjs",
        "app.config.mjs",
        "app.config.ts",
    ] {
        if frontend.join(dynamic_config).exists() {
            return Err(NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!(
                    "dynamic Expo configuration frontend/{dynamic_config} is outside the V0 authored app.json authority"
                ),
            ));
        }
    }
    let package: serde_json::Value =
        serde_json::from_slice(&fs::read(frontend.join("package.json")).map_err(|error| {
            NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!("read frontend/package.json: {error}"),
            )
        })?)
        .map_err(|error| {
            NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!("parse frontend/package.json: {error}"),
            )
        })?;
    let mut dependencies = BTreeMap::new();
    for section in ["dependencies", "devDependencies"] {
        let values = package[section].as_object().ok_or_else(|| {
            NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!("frontend/package.json must define an object-valued {section}"),
            )
        })?;
        for (name, value) in values {
            let authority = value.as_str().ok_or_else(|| {
                NodeFailure::fail(
                    "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                    format!("frontend dependency {name} has a non-string authority"),
                )
            })?;
            if semver::Version::parse(authority).is_err()
                && !valid_local_module_authority(&frontend, authority)
            {
                return Err(NodeFailure::fail(
                    "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                    format!(
                        "frontend dependency {name} must use an exact version or a committed file:./modules path, found {authority:?}"
                    ),
                ));
            }
            dependencies.insert(name.as_str(), authority);
        }
    }
    let generation_command = package["scripts"]["generate:android"]
        .as_str()
        .unwrap_or_default();
    if generation_command != "expo prebuild --platform android --clean --no-install" {
        return Err(NodeFailure::fail(
            "NATIVE_GENERATION_INPUT_POLICY_FAILED",
            format!(
                "frontend package script generate:android must be the reviewed Expo CNG command, found {generation_command:?}"
            ),
        ));
    }

    let app: serde_json::Value =
        serde_json::from_slice(&fs::read(frontend.join("app.json")).map_err(|error| {
            NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!("read frontend/app.json: {error}"),
            )
        })?)
        .map_err(|error| {
            NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!("parse frontend/app.json: {error}"),
            )
        })?;
    let plugins = app["expo"]["plugins"].as_array().ok_or_else(|| {
        NodeFailure::fail(
            "NATIVE_GENERATION_INPUT_POLICY_FAILED",
            "frontend/app.json must declare expo.plugins as an array",
        )
    })?;
    for plugin in plugins {
        let plugin = plugin
            .as_str()
            .or_else(|| plugin.as_array()?.first()?.as_str())
            .ok_or_else(|| {
                NodeFailure::fail(
                    "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                    "each Expo config plugin must be a package string or [package, options] pair",
                )
            })?;
        if plugin.starts_with("./") {
            if !valid_local_module_path(&frontend, plugin.trim_start_matches("./")) {
                return Err(NodeFailure::fail(
                    "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                    format!(
                        "local Expo config plugin {plugin:?} must resolve below committed frontend/modules"
                    ),
                ));
            }
            continue;
        }
        let package_name = plugin_package_name(plugin).ok_or_else(|| {
            NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!("Expo config plugin {plugin:?} is not a valid package reference"),
            )
        })?;
        if !dependencies.contains_key(package_name) {
            return Err(NodeFailure::fail(
                "NATIVE_GENERATION_INPUT_POLICY_FAILED",
                format!(
                    "Expo config plugin {plugin:?} is not backed by an exact declared dependency"
                ),
            ));
        }
    }
    Ok(())
}

fn valid_local_module_authority(frontend: &Path, authority: &str) -> bool {
    authority
        .strip_prefix("file:./")
        .and_then(|path| resolved_local_module_path(frontend, path))
        .is_some_and(|path| path.is_dir())
}

fn valid_local_module_path(frontend: &Path, path: &str) -> bool {
    resolved_local_module_path(frontend, path).is_some()
}

fn resolved_local_module_path(frontend: &Path, path: &str) -> Option<PathBuf> {
    let relative = Path::new(path);
    if !relative.starts_with("modules")
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let modules = frontend.join("modules");
    let candidate = frontend.join(relative);
    if fs::symlink_metadata(&modules)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return None;
    }
    let mut cursor = modules.clone();
    for component in relative.components().skip(1) {
        cursor.push(component);
        if fs::symlink_metadata(&cursor)
            .ok()
            .is_some_and(|metadata| metadata.file_type().is_symlink())
        {
            return None;
        }
    }
    let modules = modules.canonicalize().ok()?;
    let candidate = candidate.canonicalize().ok()?;
    candidate.starts_with(&modules).then_some(candidate)
}

fn plugin_package_name(plugin: &str) -> Option<&str> {
    if plugin.is_empty() || plugin.starts_with('/') || plugin.starts_with('.') {
        return None;
    }
    if plugin.starts_with('@') {
        let second_slash = plugin.match_indices('/').nth(1).map(|(index, _)| index);
        Some(second_slash.map_or(plugin, |index| &plugin[..index]))
    } else {
        Some(plugin.split('/').next().expect("non-empty plugin"))
    }
}

fn generate_android_inventory(
    context: &mut NodeContext<'_>,
) -> std::result::Result<Vec<NativeInventoryEntry>, NodeFailure> {
    let result = generate_android_host(context);
    let cleanup = remove_android_host(context.root);
    cleanup?;
    result
}

fn generate_android_host(
    context: &mut NodeContext<'_>,
) -> std::result::Result<Vec<NativeInventoryEntry>, NodeFailure> {
    let frontend = context.root.join("frontend");
    let android = frontend.join("android");
    if android.exists() {
        return Err(NodeFailure::fail(
            "NATIVE_GENERATION_DIRTY_OUTPUT",
            "generated frontend/android output existed before clean generation",
        ));
    }
    let authored_before = workspace_inputs(context.root)?;
    let account_free = AccountFreeAndroidEnvironment::prepare(context.evidence_root)?;
    let environment = account_free.expo();
    let command_result = context.command(
        &frontend,
        npm_program(),
        &["run", "--ignore-scripts", "generate:android"],
        &environment,
        "NATIVE_GENERATION_FAILED",
    );
    let authored_after = workspace_inputs(context.root)?;
    if authored_before != authored_after {
        Err(NodeFailure::fail(
            "NATIVE_GENERATION_MUTATED_AUTHORED_INPUTS",
            describe_input_drift(&authored_before, &authored_after),
        ))
    } else if let Err(failure) = command_result {
        Err(failure)
    } else if !complete_android_host(&android) {
        Err(NodeFailure::fail(
            "NATIVE_GENERATION_OUTPUT_MISSING",
            "Expo prebuild succeeded without a complete frontend/android host (required: gradlew, settings.gradle or settings.gradle.kts, and app/build.gradle or app/build.gradle.kts)",
        ))
    } else {
        native_inventory(&android)
    }
}

fn complete_android_host(android: &Path) -> bool {
    android.is_dir()
        && android.join("gradlew").is_file()
        && ["settings.gradle", "settings.gradle.kts"]
            .iter()
            .any(|path| android.join(path).is_file())
        && ["app/build.gradle", "app/build.gradle.kts"]
            .iter()
            .any(|path| android.join(path).is_file())
}

fn remove_android_host(root: &Path) -> std::result::Result<(), NodeFailure> {
    let android = root.join("frontend/android");
    if !android.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&android).map_err(|error| {
        NodeFailure::infrastructure(
            "NATIVE_GENERATION_CLEANUP_FAILED",
            format!(
                "remove generated Android host '{}': {error}",
                android.display()
            ),
        )
    })
}

fn native_inventory(root: &Path) -> std::result::Result<Vec<NativeInventoryEntry>, NodeFailure> {
    fn visit(
        root: &Path,
        path: &Path,
        entries: &mut Vec<NativeInventoryEntry>,
    ) -> std::result::Result<(), NodeFailure> {
        let mut children = fs::read_dir(path)
            .map_err(|error| {
                NodeFailure::fail(
                    "NATIVE_GENERATION_INVENTORY_FAILED",
                    format!("read generated path '{}': {error}", path.display()),
                )
            })?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|error| {
                NodeFailure::fail("NATIVE_GENERATION_INVENTORY_FAILED", error.to_string())
            })?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let child_path = child.path();
            let metadata = fs::symlink_metadata(&child_path).map_err(|error| {
                NodeFailure::fail(
                    "NATIVE_GENERATION_INVENTORY_FAILED",
                    format!("inspect generated path '{}': {error}", child_path.display()),
                )
            })?;
            let relative = child_path
                .strip_prefix(root.parent().expect("Android host has frontend parent"))
                .expect("generated path is below frontend")
                .to_string_lossy()
                .replace('\\', "/");
            let (kind, bytes) = if metadata.file_type().is_symlink() {
                let target = fs::read_link(&child_path).map_err(|error| {
                    NodeFailure::fail("NATIVE_GENERATION_INVENTORY_FAILED", error.to_string())
                })?;
                ("symlink", target.as_os_str().as_encoded_bytes().to_vec())
            } else if metadata.is_dir() {
                ("directory", Vec::new())
            } else if metadata.is_file() {
                let bytes = fs::read(&child_path).map_err(|error| {
                    NodeFailure::fail("NATIVE_GENERATION_INVENTORY_FAILED", error.to_string())
                })?;
                ("file", bytes)
            } else {
                return Err(NodeFailure::fail(
                    "NATIVE_GENERATION_INVENTORY_FAILED",
                    format!(
                        "unsupported generated path type at '{}'",
                        child_path.display()
                    ),
                ));
            };
            entries.push(NativeInventoryEntry {
                path: relative,
                kind,
                mode: native_mode(&metadata),
                sha256: format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
            });
            if metadata.is_dir() {
                visit(root, &child_path, entries)?;
            }
        }
        Ok(())
    }

    let root_metadata = fs::symlink_metadata(root).map_err(|error| {
        NodeFailure::fail(
            "NATIVE_GENERATION_INVENTORY_FAILED",
            format!("inspect generated root '{}': {error}", root.display()),
        )
    })?;
    let mut entries = vec![NativeInventoryEntry {
        path: root
            .file_name()
            .expect("Android host has a name")
            .to_string_lossy()
            .into_owned(),
        kind: "directory",
        mode: native_mode(&root_metadata),
        sha256: format!("sha256:{}", hex::encode(Sha256::digest([]))),
    }];
    visit(root, root, &mut entries)?;
    Ok(entries)
}

#[cfg(unix)]
fn native_mode(metadata: &fs::Metadata) -> String {
    format!("{:04o}", metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn native_mode(metadata: &fs::Metadata) -> String {
    if metadata.permissions().readonly() {
        "readonly".to_owned()
    } else {
        "writable".to_owned()
    }
}

fn write_native_inventory(
    path: &Path,
    entries: &[NativeInventoryEntry],
) -> std::result::Result<(), NodeFailure> {
    let mut bytes = serde_json::to_vec_pretty(entries).map_err(evidence_write_failure)?;
    bytes.push(b'\n');
    let mut file = create_private_file(path).map_err(evidence_write_failure)?;
    file.write_all(&bytes).map_err(evidence_write_failure)?;
    file.flush().map_err(evidence_write_failure)
}

fn describe_native_inventory_drift(
    expected: &[NativeInventoryEntry],
    actual: &[NativeInventoryEntry],
) -> String {
    let expected = expected
        .iter()
        .map(|entry| (PathBuf::from(&entry.path), entry))
        .collect::<BTreeMap<_, _>>();
    let actual = actual
        .iter()
        .map(|entry| (PathBuf::from(&entry.path), entry))
        .collect::<BTreeMap<_, _>>();
    describe_input_drift(&expected, &actual)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AndroidArtifactIdentity {
    path: &'static str,
    bytes: u64,
    sha256: String,
    native_inventory_sha256: String,
    runner_os: &'static str,
    runner_arch: &'static str,
    gradle_wrapper: String,
}

fn check_android_release(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    let artifact_root = context.evidence_root.join("artifacts/android.release");
    create_private_dir_all(&artifact_root).map_err(evidence_write_failure)?;
    let execution = (|| {
        let native_inventory = generate_android_host(context)?;
        let native_inventory_path = artifact_root.join("native-inventory.json");
        write_native_inventory(&native_inventory_path, &native_inventory)?;
        let android = context.root.join("frontend/android");
        let account_free = AccountFreeAndroidEnvironment::prepare(context.evidence_root)?;
        let environment = account_free.gradle();
        let gradle_wrapper = context.observe_tool_version_with_environment(
            &android,
            "gradle-wrapper",
            "./gradlew",
            &["--version"],
            &environment,
        )?;
        let source = assemble_android_release(context, &artifact_root, &environment)?;
        let apk_path = artifact_root.join("app-release.apk");
        let (apk_bytes, apk_sha256) = retain_bounded_artifact(
            &source,
            &apk_path,
            512 * 1024 * 1024,
            "ANDROID_RELEASE_OUTPUT_UNREADABLE",
        )?;
        let native_inventory_sha256 = hash_bounded_artifact(
            &native_inventory_path,
            128 * 1024 * 1024,
            "ANDROID_RELEASE_OUTPUT_UNREADABLE",
        )?;
        let identity = AndroidArtifactIdentity {
            path: "app-release.apk",
            bytes: apk_bytes,
            sha256: apk_sha256,
            native_inventory_sha256,
            runner_os: std::env::consts::OS,
            runner_arch: std::env::consts::ARCH,
            gradle_wrapper,
        };
        let mut identity_bytes =
            serde_json::to_vec_pretty(&identity).map_err(evidence_write_failure)?;
        identity_bytes.push(b'\n');
        let mut identity_file = create_private_file(&artifact_root.join("artifact.json"))
            .map_err(evidence_write_failure)?;
        identity_file
            .write_all(&identity_bytes)
            .map_err(evidence_write_failure)?;
        identity_file.flush().map_err(evidence_write_failure)
    })();
    let cleanup = remove_android_host(context.root);
    cleanup?;
    execution
}

/// Build the same account-free APK as the quality contract, retaining the product artifact.
pub(crate) fn build_android_artifact(root: &Path) -> Result<PathBuf> {
    let shutdown = install_shutdown_handler().context("install Android build shutdown handler")?;
    let result = (|| -> std::result::Result<PathBuf, NodeFailure> {
        let artifact_root = root.join("frontend/.expo/yydra-build");
        create_private_dir_all(&artifact_root).map_err(evidence_write_failure)?;
        // These fixed files describe this invocation; keep reusable caches intact.
        for relative in ["android.log", "gradle-concurrency.init.gradle"] {
            match fs::remove_file(artifact_root.join(relative)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(evidence_write_failure(error)),
            }
        }
        let mut context = NodeContext {
            root,
            evidence_root: &artifact_root,
            shutdown: &shutdown,
            log: create_private_file(&artifact_root.join("android.log"))
                .map_err(evidence_write_failure)?,
            commands: Vec::new(),
            tool_versions: BTreeMap::new(),
        };
        remove_android_host(root)?;
        generate_android_host(&mut context)?;
        let account_free = AccountFreeAndroidEnvironment::prepare(&artifact_root)?;
        assemble_android_release(&mut context, &artifact_root, &account_free.gradle())
    })();
    result.map_err(|failure| anyhow::anyhow!("{}: {}", failure.code, failure.message))
}

fn assemble_android_release(
    context: &mut NodeContext<'_>,
    artifact_root: &Path,
    environment: &[(&str, &str)],
) -> std::result::Result<PathBuf, NodeFailure> {
    let android = context.root.join("frontend/android");
    let concurrency_init_path = artifact_root.join("gradle-concurrency.init.gradle");
    write_gradle_concurrency_init_script(&concurrency_init_path)?;
    let concurrency_init_path_text = concurrency_init_path.display().to_string();
    let resolved = context.capture(
        &android,
        "./gradlew",
        &[
            "--no-daemon",
            "assembleRelease",
            "-I",
            concurrency_init_path_text.as_str(),
            "-Pkotlin.compiler.execution.strategy=in-process",
            "--max-workers=1",
        ],
        environment,
    )?;
    if !resolved.status.success() {
        return Err(NodeFailure::fail(
            "ANDROID_RELEASE_BUILD_FAILED",
            format!(
                "the bounded Gradle release invocation exited with {}: {}",
                resolved.status,
                String::from_utf8_lossy(&resolved.stderr).trim()
            ),
        ));
    }
    let source = android.join("app/build/outputs/apk/release/app-release.apk");
    if !source.is_file() {
        return Err(NodeFailure::fail(
            "ANDROID_RELEASE_OUTPUT_MISSING",
            format!(
                "Gradle succeeded without producing the required release APK at '{}'",
                source.display()
            ),
        ));
    }
    Ok(source)
}

fn write_gradle_concurrency_init_script(path: &Path) -> std::result::Result<(), NodeFailure> {
    let mut file = create_private_file(path).map_err(evidence_write_failure)?;
    file.write_all(
        br#"def isolatedHome = System.getenv('HOME')
if (isolatedHome == null || isolatedHome.isEmpty()) {
  throw new GradleException('HOME is required for the account-free Gradle invocation')
}
System.setProperty('user.home', isolatedHome)

gradle.afterProject { candidate, state ->
  def android = candidate.extensions.findByName('android')
  def cmake = android?.defaultConfig?.externalNativeBuild?.cmake
  if (cmake != null) {
    cmake.arguments(
      '-DCMAKE_JOB_POOLS=yydra_compile=1;yydra_link=1',
      '-DCMAKE_JOB_POOL_COMPILE=yydra_compile',
      '-DCMAKE_JOB_POOL_LINK=yydra_link'
    )
  }
}
"#,
    )
    .map_err(evidence_write_failure)?;
    file.flush().map_err(evidence_write_failure)
}

fn check_server_release(context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
    let origin = read_workspace_origin_record(context.root).map_err(|error| {
        NodeFailure::fail(
            "SERVER_RELEASE_BUILD_FAILED",
            format!("read exact Product identity: {error:#}"),
        )
    })?;
    let package = format!("{}-server", origin.product_id);
    context.command(
        context.root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--package",
            &package,
            "--bin",
            "server",
        ],
        &[],
        "SERVER_RELEASE_BUILD_FAILED",
    )?;
    let built_name = if cfg!(windows) {
        "server.exe".to_owned()
    } else {
        "server".to_owned()
    };
    let source = context.root.join("target/release").join(&built_name);
    if !source.is_file() {
        return Err(NodeFailure::fail(
            "SERVER_RELEASE_OUTPUT_MISSING",
            format!(
                "Cargo succeeded without producing the required server binary at '{}'",
                source.display()
            ),
        ));
    }
    let artifact_root = context.evidence_root.join("artifacts/server.release");
    create_private_dir_all(&artifact_root).map_err(evidence_write_failure)?;
    let retained_name = if cfg!(windows) {
        "server.exe"
    } else {
        "server"
    };
    let retained = artifact_root.join(retained_name);
    let (bytes, sha256) = retain_bounded_artifact(
        &source,
        &retained,
        1024 * 1024 * 1024,
        "SERVER_RELEASE_OUTPUT_UNREADABLE",
    )?;
    #[cfg(unix)]
    fs::set_permissions(&retained, fs::Permissions::from_mode(0o700))
        .map_err(evidence_write_failure)?;
    let identity = serde_json::json!({
        "schemaVersion": 1,
        "path": retained_name,
        "package": package,
        "binary": "server",
        "bytes": bytes,
        "sha256": sha256,
        "runnerOs": std::env::consts::OS,
        "runnerArch": std::env::consts::ARCH,
    });
    let mut identity_bytes =
        serde_json::to_vec_pretty(&identity).map_err(evidence_write_failure)?;
    identity_bytes.push(b'\n');
    let mut identity_file = create_private_file(&artifact_root.join("artifact.json"))
        .map_err(evidence_write_failure)?;
    identity_file
        .write_all(&identity_bytes)
        .map_err(evidence_write_failure)?;
    identity_file.flush().map_err(evidence_write_failure)
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

fn check_database_runtime_invariants(
    context: &mut NodeContext<'_>,
) -> std::result::Result<(), NodeFailure> {
    const REQUIRED_TESTS: &[&str] = &[
        "applied_migration_history_rejects_mutation_and_deletion",
        "reading_queue_use_cases_commit_success_and_rollback_failures",
        "cross_domain_orchestration_keeps_progress_synchronous_and_rolls_back_together",
        "read_committed_row_lock_serializes_conflicting_commands_without_retry",
    ];
    require_named_rust_tests(
        context,
        "reading_queue_postgres",
        REQUIRED_TESTS,
        "DATABASE_RUNTIME_INVARIANT_TEST_MISSING",
    )?;
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
        "target/yydra-check-database/compose.yaml",
        compose_source,
    )?;
    let postgres_port = available_port()?;
    let project = format!(
        "yydra-check-database-{}-{postgres_port}",
        std::process::id()
    );
    let compose_arg = compose.to_string_lossy().into_owned();
    let postgres_port_arg = postgres_port.to_string();
    let database_url =
        format!("postgres://postgres:postgres@127.0.0.1:{postgres_port}/yydra_product");
    let mut compose_guard = ComposeGuard::new(
        context.root,
        project.clone(),
        compose_arg.clone(),
        postgres_port_arg.clone(),
        "DATABASE_POSTGRES_CLEANUP_FAILED",
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
        "DATABASE_POSTGRES_UNAVAILABLE",
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
            "DATABASE_MIGRATION_FAILED",
        )?;
        for test in REQUIRED_TESTS {
            context.command(
                context.root,
                "cargo",
                &[
                    "test",
                    "--locked",
                    "--test",
                    "reading_queue_postgres",
                    test,
                    "--",
                    "--exact",
                    "--ignored",
                ],
                &[("DATABASE_URL", &database_url)],
                "DATABASE_RUNTIME_INVARIANTS_FAILED",
            )?;
        }
        Ok(())
    })();
    let down = compose_guard.cleanup(context);
    match (execution, down) {
        (_, Err(cleanup)) => Err(cleanup),
        (Err(failure), Ok(())) => Err(failure),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn check_post_commit_executor(
    context: &mut NodeContext<'_>,
) -> std::result::Result<(), NodeFailure> {
    require_named_rust_tests(
        context,
        "post_commit_executor",
        &[
            "bounded_lossy_executor_reports_admission_deadline_timeout_failure_and_crash_without_retry",
            "shutdown_cancels_tracked_work_and_forces_uncooperative_work_by_its_deadline",
        ],
        "POST_COMMIT_EXECUTOR_TEST_MISSING",
    )?;
    context.command(
        context.root,
        "cargo",
        &[
            "test",
            "--locked",
            "--test",
            "post_commit_executor",
            "--",
            "--test-threads=1",
        ],
        &[],
        "POST_COMMIT_EXECUTOR_FAILED",
    )
}

fn require_named_rust_tests(
    context: &mut NodeContext<'_>,
    target: &str,
    required: &[&str],
    failure_code: &'static str,
) -> std::result::Result<(), NodeFailure> {
    let output = context.capture(
        context.root,
        "cargo",
        &[
            "test", "--locked", "--test", target, "--", "--list", "--format", "terse",
        ],
        &[],
    )?;
    if !output.status.success() {
        return Err(NodeFailure::fail(
            failure_code,
            format!("could not discover required tests in target {target:?}"),
        ));
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    let discovered = listing
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .collect::<BTreeSet<_>>();
    let missing = required
        .iter()
        .copied()
        .filter(|test| !discovered.contains(test))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(NodeFailure::fail(
            failure_code,
            format!(
                "required tests {missing:?} are absent from target {target:?}; zero-test or renamed fixtures cannot prove this node"
            ),
        ));
    }
    Ok(())
}

fn check_product_presentation_accessibility(
    context: &mut NodeContext<'_>,
) -> std::result::Result<(), NodeFailure> {
    const PLAYWRIGHT_CONFIG: &str = r#"import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  outputDir: process.env.YYDRA_PLAYWRIGHT_OUTPUT,
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  reporter: [["json", { outputFile: process.env.YYDRA_PLAYWRIGHT_REPORT }]],
  use: {
    baseURL: `http://127.0.0.1:${process.env.YYDRA_H5_PORT}`,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
});
"#;
    let spec = context
        .root
        .join("frontend/e2e/product-presentation.accessibility.spec.ts");
    if !spec.is_file() {
        return Err(NodeFailure::fail(
            "ACCESSIBILITY_SPEC_MISSING",
            format!(
                "expected authored Product Presentation semantics at '{}'",
                spec.display()
            ),
        ));
    }
    run_h5_playwright(
        context,
        H5PlaywrightPlan {
            artifact_id: "h5.product-presentation-accessibility",
            config: PLAYWRIGHT_CONFIG,
            config_path: "frontend/e2e/.yydra-check-accessibility-playwright.config.mts",
            derived_spec: None,
            failure_code: "ACCESSIBILITY_ASSERTION_FAILED",
            migration_failure_code: "ACCESSIBILITY_MIGRATION_FAILED",
            postgres_cleanup_code: "ACCESSIBILITY_POSTGRES_CLEANUP_FAILED",
            postgres_failure_code: "ACCESSIBILITY_POSTGRES_UNAVAILABLE",
            report_path: Some("artifacts/h5.product-presentation-accessibility/report.json"),
            run_database_fixtures: false,
            spec_path: "e2e/product-presentation.accessibility.spec.ts",
        },
    )
}

fn validate_accessibility_report(report: &[u8]) -> std::result::Result<(), NodeFailure> {
    let parsed: serde_json::Value = serde_json::from_slice(report).map_err(|error| {
        NodeFailure::fail(
            "ACCESSIBILITY_REPORT_INVALID",
            format!("Playwright did not emit valid JSON evidence: {error}"),
        )
    })?;
    let stats = parsed.get("stats").ok_or_else(|| {
        NodeFailure::fail(
            "ACCESSIBILITY_REPORT_INVALID",
            "Playwright JSON evidence has no stats object",
        )
    })?;
    let count = |name: &str| {
        stats
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                NodeFailure::fail(
                    "ACCESSIBILITY_REPORT_INVALID",
                    format!("Playwright JSON evidence has no unsigned stats.{name} count"),
                )
            })
    };
    let expected = count("expected")?;
    let skipped = count("skipped")?;
    let unexpected = count("unexpected")?;
    if skipped > 0 {
        return Err(NodeFailure::fail(
            "ACCESSIBILITY_FOCUSED_OR_SKIPPED",
            format!("the canonical semantic specification skipped {skipped} tests"),
        ));
    }
    if unexpected > 0 {
        return Err(NodeFailure::fail(
            "ACCESSIBILITY_ASSERTION_FAILED",
            format!("{unexpected} Product Presentation semantic tests failed"),
        ));
    }
    if expected == 0 {
        return Err(NodeFailure::fail(
            "ACCESSIBILITY_NO_EXECUTED_TESTS",
            "the canonical semantic specification passed no tests",
        ));
    }
    Ok(())
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

test("production H5 Application Surface reaches Axum and PostgreSQL after refresh", async ({ page }) => {
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
  await expect(page.getByText("The queue is empty.")).toBeVisible();
  const entryTitle = "Transactions without hidden magic";
  const sourceUrl = "https://example.test/transactions";
  await page.getByLabel("Entry title").fill(entryTitle);
  await page.getByLabel("Source URL").fill(sourceUrl);
  await page.getByRole("button", { name: "Add entry" }).click();
  await expect(page.getByText(entryTitle, { exact: true })).toBeVisible();
  await expect(page.getByText(sourceUrl, { exact: true })).toBeVisible();
  await expect(page.getByText("State: queued", { exact: true })).toBeVisible();
  const queue = await page.evaluate(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/api/v1/reading-queue/entries`);
    return { body: await response.json(), status: response.status };
  }, apiUrl);
  expect(queue.status).toBe(200);
  expect(queue.body).toMatchObject({
    entries: [{ title: entryTitle, sourceUrl, state: "queued" }],
  });
  expect(queue.body.entries[0].id).toEqual(expect.any(String));
  const entryId = queue.body.entries[0].id;
  const directComplete = await page.evaluate(async ({ baseUrl, id }) => {
    const response = await fetch(
      `${baseUrl}/api/v1/reading-queue/entries/${encodeURIComponent(id)}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ state: "completed" }),
      },
    );
    return { body: await response.json(), status: response.status };
  }, { baseUrl: apiUrl, id: entryId });
  expect(directComplete).toMatchObject({ body: { state: "completed" }, status: 200 });
  await page.getByRole("button", { name: `Complete ${entryTitle}` }).click();
  await expect(page.getByRole("alert")).toContainText(
    "This entry changed. Refresh the queue and try again.",
  );
  await page.getByRole("button", { name: "Refresh queue" }).click();
  await expect(page.getByText("State: completed", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: `Reopen ${entryTitle}` }).click();
  await expect(page.getByText("State: queued", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: `Complete ${entryTitle}` }).click();
  await expect(page.getByText("State: completed", { exact: true })).toBeVisible();
  const negatives = await page.evaluate(async ({ baseUrl, id }) => {
    const transition = `${baseUrl}/api/v1/reading-queue/entries/${encodeURIComponent(id)}`;
    const unknown = await fetch(transition, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ state: "queued", unknown: true }),
    });
    const malformed = await fetch(transition, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: "{",
    });
    const missingAuth = await fetch(`${baseUrl}/api/v1/framework-auth-contract`);
    const forbidden = await fetch(`${baseUrl}/api/v1/framework-auth-contract`, {
      headers: { authorization: "Bearer local-framework-forbidden" },
    });
    const authorized = await fetch(`${baseUrl}/api/v1/framework-auth-contract`, {
      headers: { authorization: "Bearer local-framework-contract" },
    });
    const validation = await fetch(`${baseUrl}/api/v1/reading-queue/entries`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        title: "   ",
        sourceUrl: "https://example.test/rejected-direct",
      }),
    });
    return {
      authorized: { body: await authorized.json(), status: authorized.status },
      forbidden: { body: await forbidden.json(), status: forbidden.status },
      malformed: { body: await malformed.json(), status: malformed.status },
      missingAuth: {
        body: await missingAuth.json(),
        challenge: missingAuth.headers.get("www-authenticate"),
        status: missingAuth.status,
      },
      unknown: { body: await unknown.json(), status: unknown.status },
      validation: { body: await validation.json(), status: validation.status },
    };
  }, { baseUrl: apiUrl, id: entryId });
  for (const invalid of [negatives.unknown, negatives.malformed]) {
    expect(invalid).toMatchObject({
      body: { type: "https://yydra.dev/problems/invalid-request-body" },
      status: 400,
    });
  }
  expect(negatives.missingAuth).toMatchObject({
    body: { type: "https://yydra.dev/problems/authentication-required" },
    challenge: expect.stringContaining("Bearer"),
    status: 401,
  });
  expect(negatives.forbidden).toMatchObject({
    body: { type: "https://yydra.dev/problems/access-forbidden" },
    status: 403,
  });
  expect(negatives.authorized).toEqual({ body: { access: "granted" }, status: 200 });
  expect(negatives.validation).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-reading-entry" },
    status: 422,
  });
  const pagination = await page.evaluate(async (baseUrl) => {
    for (let index = 1; index <= 11; index += 1) {
      const response = await fetch(`${baseUrl}/api/v1/reading-queue/entries`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          title: `Paging entry ${String(index).padStart(2, "0")}`,
          sourceUrl: `https://example.test/paging-${index}`,
        }),
      });
      if (response.status !== 201) throw new Error(`fixture create returned ${response.status}`);
    }
    const requestPage = async (status, sort, limit, cursor) => {
      const query = new URLSearchParams({ status, sort, limit: String(limit) });
      if (cursor) query.set("cursor", cursor);
      const response = await fetch(`${baseUrl}/api/v1/reading-queue/entries?${query}`);
      return { body: await response.json(), status: response.status };
    };
    const first = await requestPage("queued", "oldest", 3);
    const firstCursor = first.body.nextCursor;
    const second = await requestPage("queued", "oldest", 3, firstCursor);
    const tamperedBytes = firstCursor.split("");
    const payloadIndex = firstCursor.indexOf(".") + 2;
    tamperedBytes[payloadIndex] = tamperedBytes[payloadIndex] === "A" ? "B" : "A";
    const tampered = await requestPage("queued", "oldest", 3, tamperedBytes.join(""));
    const mismatch = await requestPage("completed", "oldest", 3, firstCursor);
    const unknown = await fetch(`${baseUrl}/api/v1/reading-queue/entries?unknown=true`);
    const traversedIds = [];
    let cursor;
    let pages = 0;
    do {
      const page = await requestPage("queued", "oldest", 3, cursor);
      if (page.status !== 200) throw new Error(`pagination returned ${page.status}`);
      traversedIds.push(...page.body.entries.map((entry) => entry.id));
      cursor = page.body.nextCursor ?? undefined;
      pages += 1;
      if (pages > 10) throw new Error("pagination did not terminate");
    } while (cursor);
    return {
      first,
      second,
      tampered,
      mismatch,
      unknown: { body: await unknown.json(), status: unknown.status },
      traversedIds,
      pages,
    };
  }, apiUrl);
  expect(pagination.first).toMatchObject({
    body: { entries: expect.any(Array), nextCursor: expect.any(String) },
    status: 200,
  });
  expect(pagination.first.body.entries).toHaveLength(3);
  expect(pagination.second.status).toBe(200);
  expect(pagination.traversedIds).toHaveLength(11);
  expect(new Set(pagination.traversedIds).size).toBe(11);
  expect(pagination.pages).toBe(4);
  for (const invalid of [pagination.tampered, pagination.mismatch]) {
    expect(invalid).toMatchObject({
      body: { type: "https://yydra.dev/problems/invalid-reading-queue-cursor" },
      status: 400,
    });
  }
  expect(pagination.unknown).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-reading-queue-query" },
    status: 400,
  });
  await page.getByRole("button", { name: "Queued entries" }).click();
  await page.getByRole("button", { name: "Newest first" }).click();
  await expect(page).toHaveURL(/status=queued/);
  await expect(page).toHaveURL(/sort=newest/);
  await expect(page.getByText("Paging entry 11", { exact: true })).toBeVisible();
  await expect(page.getByText("Paging entry 01", { exact: true })).not.toBeVisible();
  await page.getByRole("button", { name: "Load more" }).click();
  await expect(page.getByText("Paging entry 01", { exact: true })).toBeVisible();
  await expect(page.getByText("End of queue.", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Refresh from first page" }).click();
  await expect(page.getByText("Paging entry 01", { exact: true })).not.toBeVisible();
  await expect(page.getByRole("button", { name: "Load more" })).toBeVisible();
  await page.reload();
  await expect(page).toHaveURL(/status=queued/);
  await expect(page).toHaveURL(/sort=newest/);
  await expect(page.getByText("Paging entry 11", { exact: true })).toBeVisible();
  await expect(page.getByText(entryTitle, { exact: true })).not.toBeVisible();
  await page.goto("/?status=completed&sort=oldest");
  await expect(page.getByText(entryTitle, { exact: true })).toBeVisible();
  await expect(page.getByText("Paging entry 11", { exact: true })).not.toBeVisible();
  await page.getByLabel("Entry title").fill("   ");
  await page.getByLabel("Source URL").fill("https://example.test/rejected");
  await page.getByRole("button", { name: "Add entry" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Enter a title and a valid source URL.",
  );
  await page.reload();
  await expect(page).toHaveURL(/status=completed/);
  await expect(page.getByText("Backend ready.")).toBeVisible();
  await expect(page.getByText("PostgreSQL schema: baseline")).toBeVisible();
  await expect(page.getByText(entryTitle, { exact: true })).toBeVisible();
  await expect(page.getByText(sourceUrl, { exact: true })).toBeVisible();
  await expect(page.getByText("State: completed", { exact: true })).toBeVisible();
});
"#;
    run_h5_playwright(
        context,
        H5PlaywrightPlan {
            artifact_id: "h5.real-runtime",
            config: PLAYWRIGHT_CONFIG,
            config_path: "frontend/e2e/.yydra-check-playwright.config.mts",
            derived_spec: Some((
                "frontend/e2e/.yydra-check-clean-workspace.spec.ts",
                PLAYWRIGHT_SPEC,
            )),
            failure_code: "H5_E2E_FAILED",
            migration_failure_code: "H5_MIGRATION_FAILED",
            postgres_cleanup_code: "H5_POSTGRES_CLEANUP_FAILED",
            postgres_failure_code: "H5_POSTGRES_UNAVAILABLE",
            report_path: None,
            run_database_fixtures: true,
            spec_path: "e2e/.yydra-check-clean-workspace.spec.ts",
        },
    )
}

#[derive(Clone, Copy)]
struct H5PlaywrightPlan {
    artifact_id: &'static str,
    config: &'static str,
    config_path: &'static str,
    derived_spec: Option<(&'static str, &'static str)>,
    failure_code: &'static str,
    migration_failure_code: &'static str,
    postgres_cleanup_code: &'static str,
    postgres_failure_code: &'static str,
    report_path: Option<&'static str>,
    run_database_fixtures: bool,
    spec_path: &'static str,
}

fn run_h5_playwright(
    context: &mut NodeContext<'_>,
    plan: H5PlaywrightPlan,
) -> std::result::Result<(), NodeFailure> {
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
    derived.write(context.root, plan.config_path, plan.config.as_bytes())?;
    if let Some((path, contents)) = plan.derived_spec {
        derived.write(context.root, path, contents.as_bytes())?;
    }

    let h5_dist = context
        .evidence_root
        .join(format!("artifacts/{}/dist", plan.artifact_id));
    let playwright = context
        .evidence_root
        .join(format!("artifacts/{}/playwright", plan.artifact_id));
    fs::create_dir_all(&playwright).map_err(|error| {
        NodeFailure::infrastructure("CHECK_EVIDENCE_WRITE_FAILED", error.to_string())
    })?;
    let report = plan
        .report_path
        .map(|path| context.evidence_root.join(path));
    if let Some(parent) = report.as_ref().and_then(|path| path.parent()) {
        fs::create_dir_all(parent).map_err(|error| {
            NodeFailure::infrastructure("CHECK_EVIDENCE_WRITE_FAILED", error.to_string())
        })?;
    }
    let h5_dist_arg = h5_dist.to_string_lossy().into_owned();
    let playwright_arg = playwright.to_string_lossy().into_owned();
    let report_arg = report
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let config_arg = plan
        .config_path
        .strip_prefix("frontend/")
        .expect("H5 Playwright config lives under frontend");

    let mut compose_guard = ComposeGuard::new(
        context.root,
        project.clone(),
        compose_arg.clone(),
        postgres_port_arg.clone(),
        plan.postgres_cleanup_code,
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
        plan.postgres_failure_code,
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
            plan.migration_failure_code,
        )?;
        if plan.run_database_fixtures {
            context.command(
                context.root,
                "cargo",
                &[
                    "test",
                    "--locked",
                    "--test",
                    "reading_queue_postgres",
                    "reading_queue_use_cases_commit_success_and_rollback_failures",
                    "--",
                    "--exact",
                    "--ignored",
                ],
                &[("DATABASE_URL", &database_url)],
                "READING_QUEUE_POSTGRES_FAILED",
            )?;
            context.command(
                context.root,
                "cargo",
                &[
                    "test",
                    "--locked",
                    "--test",
                    "reading_queue_postgres",
                    "reading_queue_keyset_pages_preserve_order_filter_context_and_termination",
                    "--",
                    "--exact",
                    "--ignored",
                ],
                &[("DATABASE_URL", &database_url)],
                "READING_QUEUE_PAGINATION_POSTGRES_FAILED",
            )?;
        }
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
        let mut environment = vec![
            ("CI", "1"),
            ("EXPO_PUBLIC_API_URL", api_url.as_str()),
            ("YYDRA_H5_PORT", h5_port_arg.as_str()),
            ("YYDRA_H5_DIST", h5_dist_arg.as_str()),
            ("YYDRA_PLAYWRIGHT_OUTPUT", playwright_arg.as_str()),
            ("YYDRA_PLAYWRIGHT_CONFIG", config_arg),
            ("YYDRA_PLAYWRIGHT_SPEC", plan.spec_path),
        ];
        if let Some(report) = report_arg.as_deref() {
            environment.push(("YYDRA_PLAYWRIGHT_REPORT", report));
        }
        let result = context.command(
            &context.root.join("frontend"),
            "node",
            &["scripts/.yydra-check-run-h5-e2e.mjs"],
            &environment,
            plan.failure_code,
        );
        let report_result = report.as_ref().map(|path| {
            fs::read(path)
                .map_err(|error| {
                    NodeFailure::fail(
                        "ACCESSIBILITY_REPORT_INVALID",
                        format!(
                            "read Playwright JSON evidence '{}': {error}",
                            path.display()
                        ),
                    )
                })
                .and_then(|contents| validate_accessibility_report(&contents))
        });
        let server_log = server.finish(&mut context.log);
        match (result, report_result, server_log) {
            (_, _, Err(failure)) => Err(failure),
            (Err(failure), _, Ok(())) if failure.outcome == Outcome::InfrastructureError => {
                Err(failure)
            }
            (_, Some(Err(failure)), Ok(())) => Err(failure),
            (Err(failure), _, Ok(())) => Err(failure),
            (Ok(()), Some(Ok(())) | None, Ok(())) => Ok(()),
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
    cleanup_code: &'static str,
    active: bool,
}

impl ComposeGuard {
    fn new(
        root: &Path,
        project: String,
        compose: String,
        port: String,
        cleanup_code: &'static str,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            project,
            compose,
            port,
            cleanup_code,
            active: true,
        }
    }

    fn cleanup(&mut self, context: &mut NodeContext<'_>) -> std::result::Result<(), NodeFailure> {
        let result = compose_down(
            context,
            &self.project,
            &self.compose,
            &self.port,
            self.cleanup_code,
        );
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
    failure_code: &'static str,
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
        failure_code,
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
            (
                "YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY",
                "local-reading-queue-cursor-signing-key",
            ),
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
        .env("CARGO_TARGET_DIR", context.root.join("target"))
        .env("DATABASE_URL", database_url)
        .env("YYDRA_BIND_ADDRESS", server_address)
        .env(
            "YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY",
            "local-reading-queue-cursor-signing-key",
        )
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
        "ANDROID_HOME",
        "ANDROID_SDK_ROOT",
        "CARGO_BUILD_JOBS",
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
        "JAVA_HOME",
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
        ("@react-native-community/netinfo", "12.0.1"),
        ("@playwright/test", "1.62.1"),
        ("@testing-library/dom", "10.4.1"),
        ("@testing-library/react", "16.3.3"),
        ("@types/react-dom", "19.2.5"),
        ("@eslint/js", "10.0.1"),
        ("eslint", "10.9.1"),
        ("jsdom", "30.0.1"),
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

fn emit_aggregate_result(result: &AggregateResult<'_>, format: MessageFormat) {
    match format {
        MessageFormat::Json => println!(
            "{}",
            serde_json::to_string(result).expect("serialize aggregate result")
        ),
        MessageFormat::Human => {
            println!(
                "{} aggregate.clean-and-reading-queue",
                outcome_name(&result.outcome).to_uppercase()
            );
            if let Some(cause) = &result.cause {
                println!("  cause [{}]: {}", cause.code, cause.message);
            }
            if let Some(remediation) = result.remediation {
                println!("  remediation: {remediation}");
            }
            println!("  proves: {}", result.proves);
            println!("  does-not-prove: {}", result.does_not_prove);
        }
    }
}

fn summary_event(
    status: &str,
    scope: &str,
    complete: bool,
    aggregate_conformance: bool,
) -> SummaryEvent {
    SummaryEvent {
        schema_version: RESULT_SCHEMA_VERSION,
        event: "check-summary".to_owned(),
        status: status.to_owned(),
        scope: scope.to_owned(),
        complete,
        aggregate_conformance,
        evidence: "manifest.json".to_owned(),
    }
}

fn emit_summary(summary: &SummaryEvent, manifest_path: &Path, format: MessageFormat) {
    let mut output = summary.clone();
    output.evidence = manifest_path.display().to_string();
    match format {
        MessageFormat::Json => println!(
            "{}",
            serde_json::to_string(&output).expect("serialize check summary")
        ),
        MessageFormat::Human => println!(
            "CHECK {} scope={} complete={} aggregate-conformance={} evidence={}",
            output.status.to_uppercase(),
            output.scope,
            output.complete,
            output.aggregate_conformance,
            output.evidence
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
    fn frontend_authority_commands_cover_local_modules() {
        assert!(FRONTEND_SOURCES.contains(&"modules"));
        assert!(FRONTEND_LINT_SOURCES.contains(&"modules"));
    }

    #[test]
    fn android_gradle_options_are_single_worker_low_memory_and_never_forward_credentials() {
        let direct = account_free_gradle_options(None);
        assert!(direct.contains("-Xmx2g"));
        assert!(direct.contains("workers.max=1"));
        assert!(!direct.contains("proxyHost"));

        let proxied = account_free_gradle_options(Some("http://127.0.0.1:10808"));
        assert!(proxied.contains("-Dhttps.proxyHost=127.0.0.1"));
        assert!(proxied.contains("-Dhttps.proxyPort=10808"));
        let credentialed = account_free_gradle_options(Some("http://secret@proxy:8080"));
        assert!(!credentialed.contains("secret"));
        assert!(!credentialed.contains("proxyHost"));
        let malformed = account_free_gradle_options(Some("http://proxy:not-a-port"));
        assert!(!malformed.contains("proxyHost"));
    }

    #[test]
    fn gradle_concurrency_script_sets_an_isolated_user_home() {
        let sandbox = tempfile::tempdir().expect("create Gradle script sandbox");
        let concurrency_script = sandbox.path().join("concurrency.init.gradle");
        write_gradle_concurrency_init_script(&concurrency_script)
            .expect("write Gradle concurrency script");
        let concurrency_source =
            fs::read_to_string(concurrency_script).expect("read Gradle concurrency script");
        assert!(concurrency_source.contains("System.setProperty('user.home', isolatedHome)"));
    }

    #[test]
    fn gradle_dependency_cache_copy_rejects_runtime_state_and_symlinks() {
        let sandbox = tempfile::tempdir().expect("create cache-copy sandbox");
        let source = sandbox.path().join("source");
        let destination = sandbox.path().join("destination");
        create_private_dir_all(&source).expect("create source");
        fs::write(source.join("modules-2.lock"), "runtime lock").expect("write lock");
        let failure = copy_gradle_dependency_cache_tree(&source, &destination)
            .expect_err("lock files must be rejected");
        assert_eq!(
            failure.code,
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let symlink_source = sandbox.path().join("symlink-source");
            let symlink_destination = sandbox.path().join("symlink-destination");
            create_private_dir_all(&symlink_source).expect("create symlink source");
            fs::write(sandbox.path().join("material"), "material").expect("write material");
            symlink(
                sandbox.path().join("material"),
                symlink_source.join("material-link"),
            )
            .expect("create cache symlink");
            let failure = copy_gradle_dependency_cache_tree(&symlink_source, &symlink_destination)
                .expect_err("symlinks must be rejected");
            assert_eq!(
                failure.code,
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID"
            );
        }
    }

    #[test]
    fn gradle_dependency_cache_preflight_enforces_file_and_byte_budgets() {
        let sandbox = tempfile::tempdir().expect("create cache-budget sandbox");
        let source = sandbox.path().join("source");
        create_private_dir_all(&source).expect("create source");
        fs::write(source.join("one.bin"), [0_u8; 8]).expect("write first fixture");
        fs::write(source.join("two.bin"), [0_u8; 8]).expect("write second fixture");

        let files = preflight_gradle_dependency_cache_tree(
            &source,
            GradleCacheSeedLimits {
                max_entries: 3,
                max_files: 1,
                max_file_bytes: 16,
                max_total_bytes: 32,
            },
        )
        .expect_err("file-count overflow must fail closed");
        assert_eq!(files.code, "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID");

        let bytes = preflight_gradle_dependency_cache_tree(
            &source,
            GradleCacheSeedLimits {
                max_entries: 3,
                max_files: 2,
                max_file_bytes: 7,
                max_total_bytes: 32,
            },
        )
        .expect_err("per-file overflow must fail closed");
        assert_eq!(bytes.code, "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID");

        let mut usage = GradleCacheSeedUsage::default();
        let copied = copy_validated_gradle_dependency_cache_tree(
            &source,
            &sandbox.path().join("bounded-copy"),
            GradleCacheSeedLimits {
                max_entries: 3,
                max_files: 2,
                max_file_bytes: 8,
                max_total_bytes: 15,
            },
            &mut usage,
        )
        .expect_err("the copy itself must enforce the total-byte budget");
        assert_eq!(copied.code, "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID");
    }

    #[test]
    fn accessibility_report_fails_closed() {
        assert!(
            validate_accessibility_report(
                br#"{"stats":{"expected":1,"skipped":0,"unexpected":0}}"#
            )
            .is_ok()
        );
        for (report, expected_code) in [
            (b"not-json".as_slice(), "ACCESSIBILITY_REPORT_INVALID"),
            (
                br#"{"stats":{"expected":0,"skipped":0,"unexpected":0}}"#.as_slice(),
                "ACCESSIBILITY_NO_EXECUTED_TESTS",
            ),
            (
                br#"{"stats":{"expected":0,"skipped":1,"unexpected":0}}"#.as_slice(),
                "ACCESSIBILITY_FOCUSED_OR_SKIPPED",
            ),
            (
                br#"{"stats":{"expected":0,"skipped":0,"unexpected":1}}"#.as_slice(),
                "ACCESSIBILITY_ASSERTION_FAILED",
            ),
        ] {
            assert_eq!(
                validate_accessibility_report(report)
                    .expect_err("negative report must fail")
                    .code,
                expected_code
            );
        }
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
