// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use tempfile::tempdir;

const POSTGRES_IMAGE: &str = "postgres:18.6-alpine3.24@sha256:d3e1620b530c944afa6e887d22eb899824da68e19c52024bf98f5220c88a65b2";
const CURSOR_SIGNING_KEY: &str = "yydra-live-workspace-cursor-signing-key";

#[test]
#[ignore = "requires Docker, npm, Playwright Chromium, and the pinned PostgreSQL image"]
fn packaged_clean_workspace_reaches_real_postgres_axum_and_production_h5() {
    let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sandbox = tempdir().expect("create live acceptance sandbox");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let package_target = sandbox.path().join("package-target");
    let package = command_output(
        Command::new(&cargo).args([
            "package",
            "--manifest-path",
            package_root
                .join("Cargo.toml")
                .to_str()
                .expect("UTF-8 manifest path"),
            "--locked",
            "--offline",
            "--allow-dirty",
            "--no-verify",
            "--target-dir",
            package_target.to_str().expect("UTF-8 package target"),
        ]),
        "package exact CLI",
    );
    assert_success(&package, "package exact CLI");
    let unpack = command_output(
        Command::new("tar").args([
            "--extract",
            "--file",
            package_target
                .join("package/yydra-cli-0.6.0.crate")
                .to_str()
                .expect("UTF-8 package archive"),
            "--directory",
            package_target
                .join("package")
                .to_str()
                .expect("UTF-8 package directory"),
        ]),
        "unpack exact CLI package",
    );
    assert_success(&unpack, "unpack exact CLI package");
    let extracted = package_target.join("package/yydra-cli-0.6.0");
    let install_root = sandbox.path().join("install");
    let install = command_output(
        Command::new(&cargo).args([
            "install",
            "yydra-cli@0.6.0",
            "--path",
            extracted.to_str().expect("UTF-8 extracted package"),
            "--locked",
            "--offline",
            "--root",
            install_root.to_str().expect("UTF-8 install root"),
            "--target-dir",
            sandbox
                .path()
                .join("install-target")
                .to_str()
                .expect("UTF-8 install target"),
        ]),
        "install exact packaged CLI",
    );
    assert_success(&install, "install exact packaged CLI");
    let yydra = install_root.join("bin/yydra");
    let workspace = sandbox.path().join("live-reader");
    let create = command_output(
        Command::new(&yydra).args([
            "new",
            workspace.to_str().expect("UTF-8 workspace"),
            "--product-name",
            "Live Reader",
            "--product-id",
            "live-reader",
            "--product-source-license",
            "Apache-2.0",
        ]),
        "create clean packaged Workspace",
    );
    assert_success(&create, "create clean packaged Workspace");

    let cargo_lock = workspace.join("Cargo.lock");
    let npm_lock = workspace.join("frontend/package-lock.json");
    let locks_before = [
        fs::read(&cargo_lock).expect("read Cargo lock before setup"),
        fs::read(&npm_lock).expect("read npm lock before setup"),
    ];
    let setup = command_output(
        Command::new(&yydra)
            .args([
                "--message-format=json",
                "internal",
                "setup",
                workspace.to_str().expect("UTF-8 workspace"),
            ])
            .current_dir(&workspace),
        "run locked setup",
    );
    assert_success(&setup, "run locked setup");
    assert_eq!(
        locks_before[0],
        fs::read(&cargo_lock).expect("reread Cargo lock")
    );
    assert_eq!(
        locks_before[1],
        fs::read(&npm_lock).expect("reread npm lock")
    );
    let build = command_output(
        Command::new(&cargo)
            .args([
                "build",
                "--locked",
                "--offline",
                "--workspace",
                "--bins",
                "--features",
                "live-reader-server/auth-fixture",
            ])
            .current_dir(&workspace),
        "build clean Workspace binaries",
    );
    assert_success(&build, "build clean Workspace binaries");
    let server_binary = workspace.join("target/debug/server");

    let postgres_port = reserve_port();
    let compose_project = format!("yydra29live{}", std::process::id());
    let compose = ComposeGuard {
        workspace: workspace.clone(),
        project: compose_project,
        postgres_port,
    };
    let up = compose.output(&["up", "-d", "--wait", "postgres"]);
    assert_success(&up, "start pinned PostgreSQL");
    let database_url =
        format!("postgres://postgres:postgres@127.0.0.1:{postgres_port}/yydra_product");

    let missing_port = reserve_port();
    let missing = run_server_failure(&server_binary, &workspace, &database_url, missing_port);
    let missing_stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        missing_stderr.contains("_sqlx_migrations"),
        "missing-history stderr: {missing_stderr}"
    );
    assert_port_closed(missing_port);
    let metadata_table =
        compose.psql("SELECT COALESCE(to_regclass('_sqlx_migrations')::text, '');");
    assert_success(&metadata_table, "inspect pre-migration database");
    assert!(
        String::from_utf8_lossy(&metadata_table.stdout)
            .trim()
            .is_empty()
    );

    let migrate = command_output(
        Command::new(&yydra)
            .args([
                "--message-format=json",
                "db",
                "migrate",
                workspace.to_str().expect("UTF-8 workspace"),
            ])
            .env("DATABASE_URL", &database_url)
            .current_dir(&workspace),
        "apply explicit migration",
    );
    assert_success(&migrate, "apply explicit migration");

    let checksum =
        compose.psql("SELECT encode(checksum, 'hex') FROM _sqlx_migrations WHERE version = 1;");
    assert_success(&checksum, "read exact migration checksum");
    let checksum = String::from_utf8(checksum.stdout)
        .expect("UTF-8 checksum")
        .trim()
        .to_owned();
    assert!(!checksum.is_empty());

    let mutate_checksum = compose
        .psql("UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = 1;");
    assert_success(&mutate_checksum, "mutate migration checksum fixture");
    let mutated_port = reserve_port();
    let mutated = run_server_failure(&server_binary, &workspace, &database_url, mutated_port);
    assert!(String::from_utf8_lossy(&mutated.stderr).contains("checksum differs"));
    assert_port_closed(mutated_port);
    let restore_checksum = compose.psql(&format!(
        "UPDATE _sqlx_migrations SET checksum = decode('{checksum}', 'hex') WHERE version = 1;"
    ));
    assert_success(&restore_checksum, "restore migration checksum fixture");

    let next_version = compose.psql("SELECT MAX(version) + 1 FROM _sqlx_migrations;");
    assert_success(&next_version, "select an unused migration version");
    let unused_version: i64 = String::from_utf8(next_version.stdout)
        .expect("UTF-8 migration version")
        .trim()
        .parse()
        .expect("integer migration version");
    let add_unknown = compose.psql(&format!(
        "INSERT INTO _sqlx_migrations \
         (version, description, installed_on, success, checksum, execution_time) \
         VALUES ({unused_version}, 'unknown', now(), true, decode('00', 'hex'), 0);",
    ));
    assert_success(&add_unknown, "add unknown migration fixture");
    let unknown_port = reserve_port();
    let unknown = run_server_failure(&server_binary, &workspace, &database_url, unknown_port);
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown migration"));
    assert_port_closed(unknown_port);
    let remove_unknown = compose.psql(&format!(
        "DELETE FROM _sqlx_migrations WHERE version = {unused_version};"
    ));
    assert_success(&remove_unknown, "remove unknown migration fixture");

    let make_incompatible = compose.psql(&format!(
        "UPDATE _sqlx_migrations SET version = {unused_version} WHERE version = 1;"
    ));
    assert_success(&make_incompatible, "make migration version incompatible");
    let incompatible_port = reserve_port();
    let incompatible =
        run_server_failure(&server_binary, &workspace, &database_url, incompatible_port);
    assert!(String::from_utf8_lossy(&incompatible.stderr).contains("incompatible"));
    assert_port_closed(incompatible_port);
    let restore_version = compose.psql(&format!(
        "UPDATE _sqlx_migrations SET version = 1 WHERE version = {unused_version};"
    ));
    assert_success(&restore_version, "restore migration version fixture");

    let server_port = reserve_port();
    let h5_port = reserve_port();
    let mut server = ServerGuard::spawn(
        &server_binary,
        &workspace,
        &database_url,
        server_port,
        h5_port,
    );
    let health = wait_for_health(&mut server.child, server_port);
    assert!(
        health.contains("HTTP/1.1 200 OK"),
        "health response: {health}"
    );
    assert!(
        health.contains(r#"{"status":"ready","database":"baseline"}"#),
        "health response: {health}"
    );

    let frontend = workspace.join("frontend");
    for (label, arguments) in [
        ("typecheck generated frontend", &["run", "typecheck"][..]),
        ("unit test generated frontend", &["test"][..]),
    ] {
        let output = command_output(
            Command::new("npm").args(arguments).current_dir(&frontend),
            label,
        );
        assert_success(&output, label);
    }
    let h5 = command_output(
        Command::new("npm")
            .args(["run", "test:e2e"])
            .current_dir(&frontend)
            .env(
                "EXPO_PUBLIC_API_URL",
                format!("http://127.0.0.1:{server_port}"),
            )
            .env("YYDRA_H5_PORT", h5_port.to_string())
            .env(
                "YYDRA_PLAYWRIGHT_OUTPUT",
                sandbox.path().join("playwright-output"),
            ),
        "run production H5 Application Surface acceptance after refresh",
    );
    if !h5.status.success() {
        server.shutdown();
        drop(compose);
        let retained = sandbox.keep();
        panic!(
            "production H5 Application Surface acceptance failed; retained diagnostics at {}\nstdout:\n{}\nstderr:\n{}",
            retained.display(),
            String::from_utf8_lossy(&h5.stdout),
            String::from_utf8_lossy(&h5.stderr)
        );
    }
    server.shutdown();

    drop(compose);
}

struct ComposeGuard {
    workspace: PathBuf,
    project: String,
    postgres_port: u16,
}

impl ComposeGuard {
    fn command(&self) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["compose", "-p", &self.project, "-f", "compose.dev.yaml"])
            .current_dir(&self.workspace)
            .env("YYDRA_POSTGRES_PORT", self.postgres_port.to_string());
        command
    }

    fn output(&self, arguments: &[&str]) -> Output {
        command_output(self.command().args(arguments), "run Docker Compose")
    }

    fn psql(&self, sql: &str) -> Output {
        self.output(&[
            "exec",
            "-T",
            "postgres",
            "psql",
            "--set=ON_ERROR_STOP=1",
            "--username=postgres",
            "--dbname=yydra_product",
            "--tuples-only",
            "--no-align",
            "--command",
            sql,
        ])
    }
}

impl Drop for ComposeGuard {
    fn drop(&mut self) {
        let _ = self
            .command()
            .args(["down", "--volumes", "--remove-orphans"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct ServerGuard {
    child: Child,
    active: bool,
}

impl ServerGuard {
    fn spawn(binary: &Path, workspace: &Path, database_url: &str, port: u16, h5_port: u16) -> Self {
        let mut command = Command::new(binary);
        command
            .current_dir(workspace)
            .env("DATABASE_URL", database_url)
            .env("YYDRA_BIND_ADDRESS", format!("127.0.0.1:{port}"))
            .env("YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY", CURSOR_SIGNING_KEY)
            .env("YYDRA_AUTH_DEVELOPMENT", "true")
            .env("YYDRA_PUBLIC_API_URL", format!("http://127.0.0.1:{port}"))
            .env(
                "YYDRA_AUTH_WEB_RETURN",
                format!("http://127.0.0.1:{h5_port}"),
            )
            .env(
                "YYDRA_AUTH_FIXTURE_PROVIDER",
                format!("http://127.0.0.1:{port}"),
            )
            .env("GITHUB_CLIENT_ID", "fixture-client")
            .env("GITHUB_CLIENT_SECRET", "fixture-secret")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        Self {
            child: command.spawn().expect("start live Axum server"),
            active: true,
        }
    }

    fn shutdown(&mut self) {
        if self.active {
            terminate_process_group(&mut self.child);
            self.active = false;
        }
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if self.active {
            terminate_process_group(&mut self.child);
        }
    }
}

fn run_server_failure(binary: &Path, workspace: &Path, database_url: &str, port: u16) -> Output {
    let mut command = Command::new(binary);
    command
        .current_dir(workspace)
        .env("DATABASE_URL", database_url)
        .env("YYDRA_BIND_ADDRESS", format!("127.0.0.1:{port}"))
        .env("YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY", CURSOR_SIGNING_KEY)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = ProcessGroupGuard {
        child: Some(command.spawn().expect("start failing server fixture")),
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child
            .child_mut()
            .try_wait()
            .expect("poll failing server fixture")
            .is_some()
        {
            let output = child
                .take()
                .wait_with_output()
                .expect("collect failing server output");
            assert!(
                !output.status.success(),
                "invalid migration history served traffic"
            );
            return output;
        }
        assert!(
            Instant::now() < deadline,
            "invalid migration history did not fail promptly"
        );
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "server listened before migration verification"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

struct ProcessGroupGuard {
    child: Option<Child>,
}

impl ProcessGroupGuard {
    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("live process child")
    }

    fn take(&mut self) -> Child {
        self.child.take().expect("live process child")
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            terminate_process_group(child);
        }
    }
}

fn wait_for_health(child: &mut Child, port: u16) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().expect("poll live server") {
            let mut stderr = String::new();
            child
                .stderr
                .take()
                .expect("piped server stderr")
                .read_to_string(&mut stderr)
                .expect("read failed server stderr");
            panic!("live server exited before health was ready: {status}\n{stderr}");
        }
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .expect("set health read timeout");
            stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .expect("write health request");
            let mut response = String::new();
            stream
                .read_to_string(&mut response)
                .expect("read health response");
            if response.contains("HTTP/1.1 200 OK") {
                return response;
            }
        }
        assert!(
            Instant::now() < deadline,
            "live health endpoint did not become ready"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn terminate_process_group(child: &mut Child) {
    let mut reaped = child.try_wait().ok().flatten().is_some();
    let group = Pid::from_raw(i32::try_from(child.id()).expect("child pid fits pid_t"));
    if let Err(error) = killpg(group, Signal::SIGTERM)
        && error != Errno::ESRCH
    {
        panic!("terminate server process group: {error}");
    }
    for _ in 0..40 {
        if !reaped && child.try_wait().ok().flatten().is_some() {
            reaped = true;
        }
        match killpg(group, None) {
            Err(Errno::ESRCH) => {
                if !reaped {
                    let _ = child.wait();
                }
                return;
            }
            Ok(()) => {}
            Err(error) => panic!("probe server process group: {error}"),
        }
        thread::sleep(Duration::from_millis(25));
    }
    if let Err(error) = killpg(group, Signal::SIGKILL)
        && error != Errno::ESRCH
    {
        panic!("force-terminate server process group: {error}");
    }
    if !reaped {
        let _ = child.wait();
    }
}

fn reserve_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("reserve TCP port")
        .local_addr()
        .expect("read reserved TCP port")
        .port()
}

fn assert_port_closed(port: u16) {
    assert!(
        TcpStream::connect(("127.0.0.1", port)).is_err(),
        "port {port} unexpectedly accepted a connection"
    );
}

fn command_output(command: &mut Command, label: &str) -> Output {
    command
        .output()
        .unwrap_or_else(|error| panic!("{label}: could not start {command:?}: {error}"))
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn compose_uses_the_reviewed_pinned_postgres_image() {
    let compose = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("template/product-workspace/compose.dev.yaml"),
    )
    .expect("read embedded Compose file");
    assert!(compose.contains(POSTGRES_IMAGE));
}
