#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Create an explicitly disposable framework source consumer outside this checkout."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def run(args, cwd):
    subprocess.run(args, cwd=cwd, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    destination = args.destination.resolve()
    if destination.exists() or destination.is_relative_to(ROOT):
        parser.error("destination must be absent and outside the framework checkout")
    cargo = os.environ.get("CARGO", "cargo")
    run([cargo, "build", "--locked", "--package", "yydra-cli"], ROOT)
    metadata = json.loads(subprocess.check_output(
        [cargo, "metadata", "--locked", "--no-deps", "--format-version=1"], cwd=ROOT))
    executable = Path(metadata["target_directory"]) / "debug" / ("yydra.exe" if os.name == "nt" else "yydra")
    run([str(executable), "new", str(destination), "--product-name", "Source Reader",
         "--product-id", "source-reader", "--product-source-license", "MIT"], ROOT)
    manifest = destination / "Cargo.toml"
    lines = manifest.read_text().splitlines()
    rust = ROOT / "capabilities/auth/rust"
    manifest.write_text("\n".join(
        "yydra-auth = { path = " + json.dumps(str(rust), ensure_ascii=False) + " }"
        if line.startswith("yydra-auth = ") else line for line in lines) + "\n")
    cargo_lock = destination / "Cargo.lock"
    entries = cargo_lock.read_text().split("[[package]]")
    for index, entry in enumerate(entries):
        if '\nname = "yydra-auth"\n' in entry:
            entries[index] = "\n".join(line for line in entry.split("\n")
                                        if not line.startswith(("source = ", "checksum = ")))
    cargo_lock.write_text("[[package]]".join(entries))
    frontend = destination / "frontend"
    package_path = frontend / "package.json"
    package = json.loads(package_path.read_text())
    for name, relative in [("@yydra/auth", "capabilities/auth/expo"),
                           ("@yydra/client-settings", "capabilities/client-settings/typescript")]:
        package["dependencies"][name] = "file:" + str(ROOT / relative)
        library = json.loads((ROOT / relative / "package.json").read_text())
        # npm file links do not hoist the linked package's runtime dependencies.
        # Materialize them in this disposable application's graph so Metro can
        # resolve all runtimes from its installation, including singleton peers.
        for dependency, version in library.get("dependencies", {}).items():
            existing = package["dependencies"].setdefault(dependency, version)
            if existing != version:
                raise RuntimeError(f"source dependency conflict for {dependency}: {existing} versus {version}")
    package_path.write_text(json.dumps(package, indent=2) + "\n")
    origin = tomllib.loads((destination / ".yydra/origin.toml").read_text())
    record = {"schema_version": 1, "distribution_version": origin["distribution_version"],
              "framework_root": str(ROOT)}
    (destination / ".yydra/source-workspace.json").write_text(json.dumps(record, indent=2) + "\n")
    # The source consumer must use this candidate's executor, never an unrelated installed CLI.
    moon = destination / "moon.yml"
    lines = []
    for line in moon.read_text().splitlines():
        if line.lstrip().startswith("command: ["):
            prefix, encoded = line.split("command: ", 1)
            command = json.loads(encoded)
            if command[0] == "yydra":
                command[0] = str(executable)
            line = prefix + "command: " + json.dumps(command, ensure_ascii=False)
        lines.append(line)
    moon.write_text("\n".join(lines) + "\n")
    # Re-resolve only once when creating the explicitly different source graph.
    # Later setup uses these generated locks without rewriting them.
    run([cargo, "update", "--package", "yydra-auth"], destination)
    run(["npm", "install", "--package-lock-only", "--ignore-scripts", "--no-audit"], frontend)
    print(f"Source Workspace: {destination}")
    print("Run moon run product:setup there, then product:db-up and product:dev.")
    print("This is disposable development state. Registry consumer validation is separate.")


if __name__ == "__main__":
    main()
