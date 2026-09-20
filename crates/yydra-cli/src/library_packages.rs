// SPDX-License-Identifier: MIT OR Apache-2.0

module_errors!("LIBRARY", [IdentityDrift => "LIBRARY_IDENTITY_DRIFT", SourceIdentity => "LIBRARY_SOURCE_IDENTITY_INVALID"], [SourceWorkspace => crate::source_workspace::Error]);

use std::{fs, path::Path};

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
    let sources = crate::source_workspace::read(root)?;
    let mut expected: toml::Value = toml::from_str(template_text("Cargo.toml.tmpl"))?;
    let actual: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    for name in ["yydra-auth", "yydra-http"] {
        if let Some(sources) = &sources {
            let path = if name == "yydra-auth" {
                &sources.rust
            } else {
                &sources.http
            };
            let path = path.to_str().context("UTF-8 library source path")?;
            expected["workspace"]["dependencies"][name] = toml::Value::Table(
                [("path".to_owned(), toml::Value::String(path.to_owned()))]
                    .into_iter()
                    .collect(),
            );
        }
        if actual
            .get("workspace")
            .and_then(|v| v.get("dependencies"))
            .and_then(|v| v.get(name))
            != expected
                .get("workspace")
                .and_then(|v| v.get("dependencies"))
                .and_then(|v| v.get(name))
        {
            fail!(
                IdentityDrift,
                "library package drift: restore the Distribution's exact {name} version and registry"
            );
        }
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
        fail!(
            IdentityDrift,
            "authentication package drift: restore the yydra-local registry configuration"
        );
    }
    for manifest in [&actual, &config] {
        if manifest
            .get("replace")
            .and_then(toml::Value::as_table)
            .is_some_and(|packages| {
                packages.iter().any(|(name, spec)| {
                    ["yydra-auth", "yydra-http"].contains(
                        &name
                            .rsplit('#')
                            .next()
                            .unwrap_or(name)
                            .split(':')
                            .next()
                            .unwrap_or(""),
                    ) || spec
                        .get("package")
                        .and_then(toml::Value::as_str)
                        .is_some_and(|name| ["yydra-auth", "yydra-http"].contains(&name))
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
                                ["yydra-auth", "yydra-http"].contains(&name.as_str())
                                    || spec
                                        .get("package")
                                        .and_then(toml::Value::as_str)
                                        .is_some_and(|name| {
                                            ["yydra-auth", "yydra-http"].contains(&name)
                                        })
                            })
                        })
                })
        {
            fail!(
                IdentityDrift,
                "authentication package drift: yydra-auth cannot be replaced by a local or Git override"
            );
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.lock"))?)?;
    let mut expected_lock: toml::Value = toml::from_str(template_text("Cargo.lock"))?;
    for name in ["yydra-auth", "yydra-http"] {
        if let Some(sources) = &sources {
            let path = if name == "yydra-auth" {
                &sources.rust
            } else {
                &sources.http
            };
            let source: toml::Value =
                toml::from_str(&fs::read_to_string(path.join("Cargo.toml"))?)?;
            if source["package"]["name"].as_str() != Some(name) {
                fail!(SourceIdentity, "source {name} package identity mismatch");
            }
            for entry in expected_lock["package"]
                .as_array_mut()
                .context("template Cargo packages")?
            {
                if entry["name"].as_str() == Some(name) {
                    entry["version"] = source["package"]["version"].clone();
                    let entry = entry.as_table_mut().context("Cargo package table")?;
                    entry.remove("source");
                    entry.remove("checksum");
                }
            }
        }
        let entries = |lock: &toml::Value| {
            lock.get("package")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .filter(|entry| entry.get("name").and_then(toml::Value::as_str) == Some(name))
                .map(|entry| {
                    ["name", "version", "source", "checksum"].map(|key| entry.get(key).cloned())
                })
                .collect::<Vec<_>>()
        };
        if entries(&lock) != entries(&expected_lock) || entries(&lock).len() != 1 {
            fail!(
                IdentityDrift,
                "library package drift: restore the locked {name} version, source and checksum"
            );
        }
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
        fail!(
            IdentityDrift,
            "authentication package drift: restore the @yydra npm registry"
        );
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
        if let Some(sources) = &sources {
            let source = if name == "@yydra/auth" {
                &sources.auth
            } else {
                &sources.settings
            };
            let declaration = format!("file:{}", source.display());
            let entry = &npm_lock["packages"][format!("node_modules/{name}")];
            let resolved = entry["resolved"]
                .as_str()
                .context("source npm link target missing")?;
            let metadata: serde_json::Value =
                serde_json::from_slice(&fs::read(source.join("package.json"))?)?;
            if package["dependencies"][name] != declaration
                || npm_lock["packages"][""]["dependencies"][name] != declaration
                || entry["link"] != true
                || root.join("frontend").join(resolved).canonicalize()? != *source
                || metadata["name"] != name
                || npm_lock["packages"][resolved]["version"] != metadata["version"]
            {
                fail!(
                    IdentityDrift,
                    "{label} source dependency drift; regenerate the source Workspace"
                );
            }
            continue;
        }
        if package["dependencies"][name] != expected_package["dependencies"][name] {
            fail!(
                IdentityDrift,
                "{label} package drift: restore the exact {name} version"
            );
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
            fail!(
                IdentityDrift,
                "{label} package drift: restore the locked {name} package and integrity"
            );
        }
    }
    Ok(())
}
