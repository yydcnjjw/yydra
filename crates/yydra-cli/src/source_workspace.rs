// SPDX-License-Identifier: MIT OR Apache-2.0

//! Explicit, disposable framework source consumers. Ordinary products use registries.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema_version: u32,
    distribution_version: String,
    framework_root: PathBuf,
}

pub(crate) struct Sources {
    pub rust: PathBuf,
    pub auth: PathBuf,
    pub settings: PathBuf,
}

pub(crate) fn read(root: &Path) -> Result<Option<Sources>> {
    let record = root.join(".yydra/source-workspace.json");
    let bytes = match fs::read(&record) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read source Workspace record"),
    };
    let record: Record = serde_json::from_slice(&bytes).context("parse source Workspace record")?;
    if record.schema_version != 1 || record.distribution_version != crate::DISTRIBUTION_VERSION {
        bail!("source Workspace version mismatch; regenerate with the current framework checkout");
    }
    let framework = record
        .framework_root
        .canonicalize()
        .context("locate source framework checkout")?;
    if !record.framework_root.is_absolute() || root.canonicalize()?.starts_with(&framework) {
        bail!("source Workspace must be outside its framework checkout");
    }
    let sources = Sources {
        rust: framework.join("capabilities/auth/rust").canonicalize()?,
        auth: framework.join("capabilities/auth/expo").canonicalize()?,
        settings: framework
            .join("capabilities/client-settings/typescript")
            .canonicalize()?,
    };
    for path in [&sources.rust, &sources.auth, &sources.settings] {
        if !path.starts_with(&framework) {
            bail!("source Capability path escapes the selected framework checkout");
        }
    }
    Ok(Some(sources))
}
