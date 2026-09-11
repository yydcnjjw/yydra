// SPDX-License-Identifier: MIT OR Apache-2.0

//! Local, account-free Android generation and APK assembly for `yydra build`.

use crate::{install_shutdown_handler, npm_program};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

#[derive(Debug)]
struct BuildError {
    code: &'static str,
    message: String,
}
impl BuildError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
struct BuildContext<'a> {
    root: &'a Path,
    run_dir: &'a Path,
    shutdown: &'a AtomicBool,
    log: File,
}
impl BuildContext<'_> {
    fn command(
        &mut self,
        directory: &Path,
        program: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
        failure_code: &'static str,
    ) -> std::result::Result<(), BuildError> {
        let output = self.capture(directory, program, arguments, environment)?;
        if !output.status.success() {
            return Err(BuildError::new(
                failure_code,
                format!(
                    "{program} {} exited with {}",
                    arguments.join(" "),
                    output.status
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
    ) -> std::result::Result<std::process::Output, BuildError> {
        let display = display_command(
            self.root,
            self.run_dir,
            directory,
            program,
            arguments,
            environment,
        );
        writeln!(self.log, "$ {display}").map_err(build_io_failure)?;
        self.log.flush().map_err(build_io_failure)?;
        let mut command = sanitized_command(program);
        command
            .args(arguments)
            .envs(environment.iter().copied())
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .current_dir(directory);
        let output = crate::process::capture(command, self.shutdown, None).map_err(|error| {
            BuildError::new(
                "ANDROID_BUILD_PROCESS_FAILED",
                format!("{program}: {error:#}"),
            )
        })?;
        self.log
            .write_all(&output.stdout)
            .map_err(build_io_failure)?;
        self.log
            .write_all(&output.stderr)
            .map_err(build_io_failure)?;
        self.log.flush().map_err(build_io_failure)?;
        Ok(output)
    }
}
fn build_io_failure(error: impl std::fmt::Display) -> BuildError {
    BuildError::new("ANDROID_BUILD_IO_FAILED", error.to_string())
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
    fn prepare(run_dir: &Path) -> std::result::Result<Self, BuildError> {
        let root = run_dir.join("scratch/android-account-free");
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
            create_private_dir_all(directory).map_err(build_io_failure)?;
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

fn seed_gradle_dependency_cache(gradle_user_home: &Path) -> std::result::Result<(), BuildError> {
    let Some(seed) = std::env::var_os("YYDRA_GRADLE_DEPENDENCY_CACHE_SEED") else {
        return Ok(());
    };
    let seed = PathBuf::from(seed);
    if !seed.is_absolute() {
        return Err(BuildError::new(
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
            Err(BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "isolated Gradle dependency cache '{}' exists without a completed seed copy",
                    destination.display()
                ),
            ))
        };
    }
    copy_gradle_dependency_cache_tree(&source, &destination)?;
    let mut marker = create_private_file(&completion_marker).map_err(build_io_failure)?;
    marker.write_all(b"complete\n").map_err(build_io_failure)?;
    marker.flush().map_err(build_io_failure)
}

fn copy_gradle_dependency_cache_tree(
    source: &Path,
    destination: &Path,
) -> std::result::Result<(), BuildError> {
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
) -> std::result::Result<(u64, u64), BuildError> {
    let mut pending = vec![source.to_path_buf()];
    let mut entries = 0_u64;
    let mut files = 0_u64;
    let mut total_bytes = 0_u64;
    while let Some(path) = pending.pop() {
        entries = entries.checked_add(1).ok_or_else(|| {
            BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed entry count overflow",
            )
        })?;
        if entries > limits.max_entries {
            return Err(BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed exceeds its bounded entry-count limit",
            ));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "inspect Gradle dependency cache seed '{}': {error}",
                    path.display()
                ),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "Gradle dependency cache seed '{}' contains a symlink",
                    path.display()
                ),
            ));
        }
        if metadata.is_dir() {
            let directory = fs::read_dir(&path).map_err(|error| {
                BuildError::new(
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
                    return Err(BuildError::new(
                        "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                        "Gradle dependency cache seed exceeds its bounded entry-count limit",
                    ));
                }
                children.push(child.map_err(|error| {
                    BuildError::new(
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
                    return Err(BuildError::new(
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
            return Err(BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                format!(
                    "Gradle dependency cache seed '{}' contains an unsupported file type",
                    path.display()
                ),
            ));
        }
        files = files.checked_add(1).ok_or_else(|| {
            BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed file count overflow",
            )
        })?;
        total_bytes = total_bytes.checked_add(metadata.len()).ok_or_else(|| {
            BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed byte count overflow",
            )
        })?;
        if files > limits.max_files
            || metadata.len() > limits.max_file_bytes
            || total_bytes > limits.max_total_bytes
        {
            return Err(BuildError::new(
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
) -> std::result::Result<(), BuildError> {
    usage.entries = usage.entries.checked_add(1).ok_or_else(|| {
        BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed entry count overflow during copy",
        )
    })?;
    if usage.entries > limits.max_entries {
        return Err(BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed exceeds its bounded entry-count limit during copy",
        ));
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "inspect Gradle dependency cache seed '{}': {error}",
                source.display()
            ),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "Gradle dependency cache seed '{}' contains a symlink",
                source.display()
            ),
        ));
    }
    if metadata.is_dir() {
        create_private_dir_all(destination).map_err(build_io_failure)?;
        let directory = fs::read_dir(source).map_err(|error| {
            BuildError::new(
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
                return Err(BuildError::new(
                    "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                    "Gradle dependency cache seed exceeds its bounded entry-count limit during copy",
                ));
            }
            children.push(child.map_err(|error| {
                BuildError::new(
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
                return Err(BuildError::new(
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
        return Err(BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "Gradle dependency cache seed '{}' contains an unsupported file type",
                source.display()
            ),
        ));
    }
    usage.files = usage.files.checked_add(1).ok_or_else(|| {
        BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed file count overflow during copy",
        )
    })?;
    if usage.files > limits.max_files || metadata.len() > limits.max_file_bytes {
        return Err(BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed exceeds its bounded file-count or per-file limit during copy",
        ));
    }
    let mut input = File::open(source).map_err(|error| {
        BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            format!(
                "open Gradle dependency cache seed '{}': {error}",
                source.display()
            ),
        )
    })?;
    let mut output = create_private_file(destination).map_err(build_io_failure)?;
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| {
            BuildError::new(
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
            BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed file byte count overflow during copy",
            )
        })?;
        usage.total_bytes = usage.total_bytes.checked_add(read).ok_or_else(|| {
            BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed total byte count overflow during copy",
            )
        })?;
        if copied > limits.max_file_bytes || usage.total_bytes > limits.max_total_bytes {
            return Err(BuildError::new(
                "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
                "Gradle dependency cache seed grew beyond its bounded byte budget during copy",
            ));
        }
        output
            .write_all(&buffer[..usize::try_from(read).unwrap_or(buffer.len())])
            .map_err(build_io_failure)?;
    }
    if copied != metadata.len() {
        return Err(BuildError::new(
            "ANDROID_RELEASE_DEPENDENCY_CACHE_SEED_INVALID",
            "Gradle dependency cache seed changed while it was copied",
        ));
    }
    output.flush().map_err(build_io_failure)
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

fn generate_android_host(context: &mut BuildContext<'_>) -> std::result::Result<(), BuildError> {
    let frontend = context.root.join("frontend");
    let android = frontend.join("android");
    if android.exists() {
        return Err(BuildError::new(
            "NATIVE_GENERATION_DIRTY_OUTPUT",
            "generated frontend/android output existed before clean generation",
        ));
    }
    let authored_before = workspace_inputs(context.root)?;
    let account_free = AccountFreeAndroidEnvironment::prepare(context.run_dir)?;
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
        Err(BuildError::new(
            "NATIVE_GENERATION_MUTATED_AUTHORED_INPUTS",
            describe_input_drift(&authored_before, &authored_after),
        ))
    } else if let Err(failure) = command_result {
        Err(failure)
    } else if !complete_android_host(&android) {
        Err(BuildError::new(
            "NATIVE_GENERATION_OUTPUT_MISSING",
            "Expo prebuild succeeded without a complete frontend/android host (required: gradlew, settings.gradle or settings.gradle.kts, and app/build.gradle or app/build.gradle.kts)",
        ))
    } else {
        Ok(())
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

fn remove_android_host(root: &Path) -> std::result::Result<(), BuildError> {
    let android = root.join("frontend/android");
    if !android.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&android).map_err(|error| {
        BuildError::new(
            "NATIVE_GENERATION_CLEANUP_FAILED",
            format!(
                "remove generated Android host '{}': {error}",
                android.display()
            ),
        )
    })
}

/// Generate a fresh native host and retain its release APK in the Product Workspace.
pub(crate) fn build_android_artifact(root: &Path) -> Result<PathBuf> {
    let shutdown = install_shutdown_handler().context("install Android build shutdown handler")?;
    let result = (|| -> std::result::Result<PathBuf, BuildError> {
        let artifact_root = root.join("frontend/.expo/yydra-build");
        create_private_dir_all(&artifact_root).map_err(build_io_failure)?;
        // These fixed files describe this invocation; keep reusable caches intact.
        for relative in ["android.log", "gradle-concurrency.init.gradle"] {
            match fs::remove_file(artifact_root.join(relative)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(build_io_failure(error)),
            }
        }
        let mut context = BuildContext {
            root,
            run_dir: &artifact_root,
            shutdown: &shutdown,
            log: create_private_file(&artifact_root.join("android.log"))
                .map_err(build_io_failure)?,
        };
        remove_android_host(root)?;
        generate_android_host(&mut context)?;
        let account_free = AccountFreeAndroidEnvironment::prepare(&artifact_root)?;
        assemble_android_release(&mut context, &artifact_root, &account_free.gradle())
    })();
    result.map_err(|failure| anyhow::anyhow!("{}: {}", failure.code, failure.message))
}

fn assemble_android_release(
    context: &mut BuildContext<'_>,
    artifact_root: &Path,
    environment: &[(&str, &str)],
) -> std::result::Result<PathBuf, BuildError> {
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
        return Err(BuildError::new(
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
        return Err(BuildError::new(
            "ANDROID_RELEASE_OUTPUT_MISSING",
            format!(
                "Gradle succeeded without producing the required release APK at '{}'",
                source.display()
            ),
        ));
    }
    Ok(source)
}

fn write_gradle_concurrency_init_script(path: &Path) -> std::result::Result<(), BuildError> {
    let mut file = create_private_file(path).map_err(build_io_failure)?;
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
    .map_err(build_io_failure)?;
    file.flush().map_err(build_io_failure)
}

fn workspace_inputs(root: &Path) -> std::result::Result<InputInventory, BuildError> {
    let mut inputs = BTreeMap::new();
    collect_workspace_inputs(root, root, &mut inputs)?;
    Ok(inputs)
}

fn collect_workspace_inputs(
    root: &Path,
    directory: &Path,
    inputs: &mut InputInventory,
) -> std::result::Result<(), BuildError> {
    let entries = fs::read_dir(directory).map_err(|error| {
        BuildError::new(
            "ANDROID_BUILD_INPUT_INVENTORY_FAILED",
            format!("read '{}': {error}", directory.display()),
        )
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| {
                BuildError::new("ANDROID_BUILD_INPUT_INVENTORY_FAILED", error.to_string())
            })?
            .path();
        let relative = path
            .strip_prefix(root)
            .expect("walk starts at workspace root");
        if excluded_input(relative) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            BuildError::new(
                "ANDROID_BUILD_INPUT_INVENTORY_FAILED",
                format!("inspect '{}': {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(|error| {
                BuildError::new("ANDROID_BUILD_INPUT_INVENTORY_FAILED", error.to_string())
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
                        BuildError::new(
                            "ANDROID_BUILD_INPUT_INVENTORY_FAILED",
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
        || relative.starts_with("frontend/modules/yydra-client-settings/node_modules")
        || relative.starts_with("frontend/.expo")
        || relative.starts_with("frontend/dist")
        || relative.starts_with("frontend/test-results")
        || relative.starts_with("frontend/android")
        || relative.starts_with("frontend/ios")
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
    run_dir: &Path,
    directory: &Path,
    program: &str,
    arguments: &[&str],
    environment: &[(&str, &str)],
) -> String {
    let normalize = |value: &str| {
        value
            .replace(&run_dir.display().to_string(), "$BUILD")
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
