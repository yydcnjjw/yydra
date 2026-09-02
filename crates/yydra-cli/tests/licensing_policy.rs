// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn distribution_ships_both_complete_license_texts_and_exact_spdx_markers() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root_mit = fs::read(repository.join("LICENSE-MIT")).expect("read root MIT license");
    let root_apache =
        fs::read(repository.join("LICENSE-APACHE")).expect("read root Apache license");
    assert_eq!(
        root_mit,
        fs::read(repository.join("crates/yydra-cli/LICENSE-MIT"))
            .expect("read packaged MIT license")
    );
    assert_eq!(
        root_apache,
        fs::read(repository.join("crates/yydra-cli/LICENSE-APACHE"))
            .expect("read packaged Apache license")
    );
    let mit = String::from_utf8(root_mit).expect("UTF-8 MIT license");
    assert!(mit.starts_with("MIT License\n"));
    assert!(mit.contains("Permission is hereby granted, free of charge"));
    assert!(mit.contains("THE SOFTWARE IS PROVIDED \"AS IS\""));
    let apache = String::from_utf8(root_apache).expect("UTF-8 Apache license");
    assert!(apache.contains("Apache License\n                           Version 2.0"));
    assert!(apache.contains("1. Definitions."));
    assert!(apache.contains("9. Accepting Warranty or Additional Liability."));
    assert!(apache.contains("END OF TERMS AND CONDITIONS"));

    for relative in yydra_authored_sources(&repository) {
        let contents = fs::read_to_string(repository.join(&relative))
            .unwrap_or_else(|error| panic!("read {}: {error}", relative.display()));
        let prologue = contents.lines().take(3).collect::<Vec<_>>().join("\n");
        assert!(
            prologue.contains("SPDX-License-Identifier: MIT OR Apache-2.0"),
            "{} must use the exact SPDX expression in its prologue",
            relative.display()
        );
    }
}

fn yydra_authored_sources(repository: &Path) -> BTreeSet<PathBuf> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(repository)
        .output()
        .expect("list tracked sources");
    assert!(output.status.success());
    let mut sources = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(String::from_utf8(path.to_vec()).expect("UTF-8 source path")))
        .filter(|path| requires_spdx_prologue(path))
        .collect::<BTreeSet<_>>();
    collect_template_sources(
        repository,
        Path::new("crates/yydra-cli/template/product-workspace"),
        &mut sources,
    );
    for relative in [
        ".github/workflows/dco.yml",
        "CONTRIBUTING.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "scripts/check-dco",
        "crates/yydra-cli/LICENSE-MIT",
        "crates/yydra-cli/LICENSE-APACHE",
        "crates/yydra-cli/template/product-workspace/.yydra/product-source-license.toml",
        "crates/yydra-cli/tests/dco_policy.rs",
        "crates/yydra-cli/tests/licensing_policy.rs",
    ] {
        sources.insert(relative.into());
    }
    sources.retain(|path| requires_spdx_prologue(path));
    sources
}

fn collect_template_sources(repository: &Path, relative: &Path, sources: &mut BTreeSet<PathBuf>) {
    let absolute = repository.join(relative);
    for entry in fs::read_dir(&absolute)
        .unwrap_or_else(|error| panic!("read template directory {}: {error}", absolute.display()))
    {
        let entry = entry.expect("read template entry");
        let path = relative.join(entry.file_name());
        if entry.file_type().expect("read template file type").is_dir() {
            collect_template_sources(repository, &path, sources);
        } else if requires_spdx_prologue(&path) {
            sources.insert(path);
        }
    }
}

fn requires_spdx_prologue(path: &Path) -> bool {
    if is_complete_license_text(path) {
        return false;
    }
    if matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("Cargo.lock" | "package-lock.json")
    ) {
        return false;
    }
    path.extension().and_then(|extension| extension.to_str()) != Some("json")
}

fn is_complete_license_text(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("LICENSE-MIT" | "LICENSE-APACHE")
    )
}
