// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

#[test]
#[ignore = "consumer integration: requires Docker Compose, curl, image downloads and a real Rust container build"]
fn compose_builds_a_persistent_server_and_blocks_failed_migrations() {
    let sandbox = tempfile::tempdir().expect("create container test sandbox");
    let workspace = sandbox.path().join("reader");
    success(
        Command::new(env!("CARGO_BIN_EXE_yydra"))
            .arg("new")
            .arg(&workspace)
            .args([
                "--product-name",
                "Container Reader",
                "--product-id",
                "container-reader",
                "--product-source-license",
                "Apache-2.0",
            ]),
    );
    let compose_name = |file: &str| {
        let config = success(
            Command::new("docker")
                .args(["compose", "-f", file, "config", "--format", "json"])
                .current_dir(&workspace)
                .env_remove("COMPOSE_PROJECT_NAME"),
        );
        let config: serde_json::Value = serde_json::from_str(&config).unwrap();
        config["name"].clone()
    };
    assert_ne!(
        compose_name("compose.yaml"),
        compose_name("compose.dev.yaml")
    );
    let project = format!(
        "yydra-server-compose-{}-{}",
        std::process::id(),
        sandbox
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase()
            .replace('.', "")
    );
    let deployment = Deployment {
        image: format!("{project}:local"),
        project,
        workspace,
        sandbox: Some(sandbox),
    };
    eprintln!("Container acceptance project: {}", deployment.project);
    eprintln!("Building and starting the fresh Product Workspace");
    deployment.run(&["up", "--build", "--wait", "--wait-timeout", "90"]);

    let address = deployment.run(&["port", "server", "4000"]);
    let base = format!("http://{}", address.trim());
    let created = success(Command::new("curl").args([
        "--fail",
        "--silent",
        "--show-error",
        "--header",
        "Content-Type: application/json",
        "--data",
        r#"{"title":"Survives recreation","sourceUrl":"https://example.com/retained"}"#,
        &format!("{base}/api/v1/reading-queue/entries"),
    ]));
    assert!(created.contains("Survives recreation"));
    let credentials = deployment.run(&[
        "exec",
        "-T",
        "server",
        "sha256sum",
        "/var/lib/yydra/postgres-password",
        "/var/lib/yydra/cursor-signing-key",
    ]);
    deployment.run(&[
        "exec",
        "-T",
        "server",
        "sh",
        "-ec",
        "test \"$(id -u)\" != 0; ! command -v cargo; ! command -v node",
    ]);
    let postgres_id = deployment.run(&["ps", "--quiet", "postgres"]);
    let ports = success(Command::new("docker").args([
        "inspect",
        "--format",
        "{{json .NetworkSettings.Ports}}",
        postgres_id.trim(),
    ]));
    let ports: serde_json::Value =
        serde_json::from_str(&ports).expect("published PostgreSQL ports");
    assert!(
        ports
            .as_object()
            .unwrap()
            .values()
            .all(serde_json::Value::is_null)
    );

    eprintln!("Adding only a migration and rebuilding with the existing Cargo cache");
    fs::write(
        deployment
            .workspace
            .join("migrations/0006_container_acceptance.sql"),
        "CREATE TABLE container_acceptance_marker (id INTEGER PRIMARY KEY);\n",
    )
    .expect("add a forward migration");
    deployment.run(&["up", "--build", "--wait", "--wait-timeout", "90"]);
    let applied = deployment.run(&[
        "exec",
        "-T",
        "postgres",
        "psql",
        "-U",
        "postgres",
        "-d",
        "yydra_product",
        "-tAc",
        "SELECT to_regclass('container_acceptance_marker')",
    ]);
    assert_eq!(applied.trim(), "container_acceptance_marker");
    deployment.run(&["stop"]);
    deployment.run(&["start", "--wait", "--wait-timeout", "90"]);

    eprintln!("Recreating containers and checking retained records and credentials");
    deployment.run(&["down"]);
    deployment.run(&["up", "--wait", "--wait-timeout", "90"]);
    let address = deployment.run(&["port", "server", "4000"]);
    let listed = success(Command::new("curl").args([
        "--fail",
        "--silent",
        "--show-error",
        &format!("http://{}/api/v1/reading-queue/entries", address.trim()),
    ]));
    assert!(listed.contains("Survives recreation"));
    assert_eq!(
        credentials,
        deployment.run(&[
            "exec",
            "-T",
            "server",
            "sha256sum",
            "/var/lib/yydra/postgres-password",
            "/var/lib/yydra/cursor-signing-key",
        ])
    );

    eprintln!("Checking that a mismatched migration prevents backend startup");
    deployment.run(&[
        "exec",
        "-T",
        "postgres",
        "psql",
        "-U",
        "postgres",
        "-d",
        "yydra_product",
        "-v",
        "ON_ERROR_STOP=1",
        "-c",
        "UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = 1",
    ]);
    deployment.run(&["down"]);
    let failed = deployment
        .command(&["up", "--wait", "--wait-timeout", "90"])
        .output()
        .expect("run failing migration deployment");
    assert!(
        !failed.status.success(),
        "migration drift must fail deployment"
    );
    let running = deployment.run(&["ps", "--status", "running", "--services"]);
    assert!(!running.lines().any(|service| service == "server"));
    let migrate = deployment.run(&["ps", "--all", "--format", "json", "migrate"]);
    let state: serde_json::Value = serde_json::from_str(migrate.trim()).expect("migration state");
    assert_ne!(state["ExitCode"], 0);
    eprintln!("Container acceptance passed");
}

struct Deployment {
    project: String,
    image: String,
    workspace: PathBuf,
    sandbox: Option<TempDir>,
}

impl Deployment {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["compose", "-p", &self.project])
            .args(args)
            .current_dir(&self.workspace)
            .env("YYDRA_SERVER_IMAGE", &self.image)
            .env("YYDRA_SERVER_PORT", "0")
            .env("YYDRA_SERVER_HOST", "127.0.0.1");
        command
    }

    fn run(&self, args: &[&str]) -> String {
        let output = self.command(args).output().expect("run Docker Compose");
        if !output.status.success() {
            let logs = self
                .command(&["logs", "--tail", "100"])
                .output()
                .expect("read failed deployment logs");
            eprintln!("{}", String::from_utf8_lossy(&logs.stdout));
        }
        checked(output)
    }
}

impl Drop for Deployment {
    fn drop(&mut self) {
        // Only this test's uniquely named project, volumes and image are removed.
        let cleanup = self
            .command(&["down", "--volumes", "--remove-orphans"])
            .output()
            .expect("clean container acceptance project");
        if !cleanup.status.success() {
            let recovery = self.sandbox.take().unwrap().keep();
            eprintln!("Retained failed cleanup workspace: {}", recovery.display());
            eprintln!(
                "Container cleanup failed for {}: {}",
                self.project,
                String::from_utf8_lossy(&cleanup.stderr)
            );
        }
        if cleanup.status.success() {
            let image = Command::new("docker")
                .args(["image", "rm", &self.image])
                .output()
                .expect("remove task image");
            if !image.status.success() {
                eprintln!(
                    "Image cleanup needs review for {}: {}",
                    self.image,
                    String::from_utf8_lossy(&image.stderr)
                );
            }
        }
    }
}

fn success(command: &mut Command) -> String {
    checked(command.output().expect("run acceptance command"))
}

fn checked(output: Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 command output")
}
