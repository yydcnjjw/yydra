// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::TEMPLATE;

fn template_text(path: &str) -> &str {
    TEMPLATE
        .get_file(path)
        .expect("embedded package input")
        .contents_utf8()
        .expect("UTF-8 package input")
}

/// Check the actual package declarations and locked identities, without a network request.
/// Other product dependencies remain product-owned.
pub(crate) fn verify(root: &Path) -> Result<()> {
    let expected: toml::Value = toml::from_str(template_text("Cargo.toml.tmpl"))?;
    let actual: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    if actual
        .get("workspace")
        .and_then(|v| v.get("dependencies"))
        .and_then(|v| v.get("yydra-auth"))
        != expected
            .get("workspace")
            .and_then(|v| v.get("dependencies"))
            .and_then(|v| v.get("yydra-auth"))
    {
        bail!(
            "authentication package drift: restore the Distribution's exact yydra-auth version and registry"
        );
    }
    let config: toml::Value = toml::from_str(
        &fs::read_to_string(root.join(".cargo/config.toml"))
            .context("read authentication registry configuration")?,
    )?;
    let expected_config: toml::Value = toml::from_str(template_text(".cargo/config.toml"))?;
    if config.get("registries").and_then(|v| v.get("yydra-local"))
        != expected_config
            .get("registries")
            .and_then(|v| v.get("yydra-local"))
    {
        bail!("authentication package drift: restore the yydra-local registry configuration");
    }
    for manifest in [&actual, &config] {
        if manifest
            .get("replace")
            .and_then(toml::Value::as_table)
            .is_some_and(|packages| {
                packages.iter().any(|(name, spec)| {
                    name.rsplit('#').next().unwrap_or(name).split(':').next() == Some("yydra-auth")
                        || spec.get("package").and_then(toml::Value::as_str) == Some("yydra-auth")
                })
            })
            || manifest
                .get("patch")
                .and_then(toml::Value::as_table)
                .is_some_and(|sources| {
                    sources
                        .values()
                        .filter_map(toml::Value::as_table)
                        .any(|packages| {
                            packages.iter().any(|(name, spec)| {
                                name == "yydra-auth"
                                    || spec.get("package").and_then(toml::Value::as_str)
                                        == Some("yydra-auth")
                            })
                        })
                })
        {
            bail!(
                "authentication package drift: yydra-auth cannot be replaced by a local or Git override"
            );
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.lock"))?)?;
    let expected_lock: toml::Value = toml::from_str(template_text("Cargo.lock"))?;
    let auth_entries = |lock: &toml::Value| {
        lock.get("package")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|entry| entry.get("name").and_then(toml::Value::as_str) == Some("yydra-auth"))
            .map(|entry| {
                ["name", "version", "source", "checksum"].map(|key| entry.get(key).cloned())
            })
            .collect::<Vec<_>>()
    };
    if auth_entries(&lock) != auth_entries(&expected_lock) || auth_entries(&lock).len() != 1 {
        bail!(
            "authentication package drift: restore the locked yydra-auth version, source and checksum"
        );
    }

    let package: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("frontend/package.json"))?)?;
    let expected_package: serde_json::Value = serde_json::from_str(
        &template_text("frontend/package.json")
            .replace("__PRODUCT_SOURCE_LICENSE_TOML__", "\"MIT\""),
    )?;
    let npmrc = fs::read_to_string(root.join("frontend/.npmrc"))?;
    let scopes = |text: &str| {
        text.lines()
            .map(str::trim)
            .filter(|line| line.starts_with("@yydra:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    if scopes(&npmrc) != scopes(template_text("frontend/.npmrc")) {
        bail!("authentication package drift: restore the @yydra npm registry");
    }
    let npm_lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("frontend/package-lock.json"))?)?;
    let expected_npm_lock: serde_json::Value = serde_json::from_str(
        &template_text("frontend/package-lock.json")
            .replace("__PRODUCT_SOURCE_LICENSE_TOML__", "\"MIT\""),
    )?;
    for (name, label) in [
        ("@yydra/auth", "authentication"),
        ("@yydra/client-settings", "client settings"),
    ] {
        if package["dependencies"][name] != expected_package["dependencies"][name] {
            bail!("{label} package drift: restore the exact {name} version");
        }
        let key = format!("node_modules/{name}");
        let entry = &npm_lock["packages"][&key];
        let expected_entry = &expected_npm_lock["packages"][&key];
        if entry.is_null()
            || ["version", "resolved", "integrity"]
                .iter()
                .any(|key| entry[*key] != expected_entry[*key])
            || entry["link"] == true
            || npm_lock["packages"][""]["dependencies"][name]
                != expected_package["dependencies"][name]
        {
            bail!("{label} package drift: restore the locked {name} package and integrity");
        }
    }
    Ok(())
}
