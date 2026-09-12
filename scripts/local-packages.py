#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Manage Yydra's loopback-only Cargo/npm development registries."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
CARGO_URL = "http://127.0.0.1:18081"
NPM_URL = "http://127.0.0.1:4873"


def run(args, cwd=ROOT, env=None, capture=False):
    result = subprocess.run(args, cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE if capture else None,
                            stderr=subprocess.PIPE if capture else None)
    if result.returncode:
        if capture:
            print(result.stderr, file=sys.stderr)
        raise RuntimeError(f"command failed: {args[0]} {args[1]}")
    return result.stdout if capture else None


def request(url, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method="GET" if body is None else "PUT",
                                 headers={"Content-Type": "application/json"})
    # Local registry traffic must not pass through the user's HTTP proxy.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        with opener.open(req, timeout=20) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        if error.code == 404 and body is None:
            return None
        raise RuntimeError(f"local registry returned HTTP {error.code}") from None


def write_private(path, text):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "w") as file:
        file.write(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    default_data = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    parser.add_argument("--state-dir", type=Path, default=default_data / "yydra/local-packages")
    parser.add_argument("--project", default="yydra-local-packages")
    parser.add_argument("command", choices=["up", "status", "publish", "down"])
    args = parser.parse_args()
    state = args.state_dir.resolve()
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    credentials_path = state / "credentials.json"
    if not credentials_path.exists():
        if args.command != "up":
            raise RuntimeError("run up with this same state directory and Compose project first")
        credentials = {"project": args.project, "cargo_password": secrets.token_urlsafe(32),
                       "cargo_token": secrets.token_urlsafe(32),
                       "npm_password": secrets.token_urlsafe(32)}
        write_private(credentials_path, json.dumps(credentials))
    credentials = json.loads(credentials_path.read_text())
    if credentials.get("project") != args.project:
        raise RuntimeError("this state directory belongs to a different Compose project")
    env = os.environ.copy()
    env.update(YYDRA_REGISTRY_PASSWORD=credentials["cargo_password"],
               YYDRA_REGISTRY_TOKEN=credentials["cargo_token"])
    compose = ["docker", "compose", "--project-name", args.project, "--file",
               str(ROOT / "dev/local-packages/compose.yaml")]
    if args.command == "down":
        run(compose + ["down"], env=env)
        print("Stopped containers; package volumes and local credentials retained.")
        return
    if args.command == "status":
        run(compose + ["ps"], env=env)
        return
    if args.command == "up":
        run(compose + ["up", "--detach", "--wait", "--wait-timeout", "120"], env=env)
        if "npm_token" not in credentials:
            response = json.loads(request(NPM_URL + "/-/user/org.couchdb.user:yydra", {
                "name": "yydra", "password": credentials["npm_password"],
                "email": "local-packages@localhost.invalid", "type": "user", "roles": []}))
            credentials["npm_token"] = response["token"]
            write_private(credentials_path, json.dumps(credentials))
        print(f"Cargo: {CARGO_URL}; npm: {NPM_URL}; state: {state}")
        return

    if "npm_token" not in credentials:
        raise RuntimeError("registry bootstrap incomplete; run up first")
    # Stage outside the source checkout so VCS metadata does not make package
    # checksums depend on unrelated commits or on the absolute checkout path.
    git = subprocess.run(["git", "-C", str(state), "rev-parse", "--show-toplevel"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if git.returncode == 0:
        raise RuntimeError("state-dir must be outside a Git checkout for reproducible packages")
    staging = state / "staging"
    staging.mkdir(exist_ok=True)
    cargo = staging / "yydra-auth"
    npm = staging / "auth-client"
    for source, destination in [(ROOT / "crates/yydra-auth", cargo), (ROOT / "packages/auth-client", npm)]:
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(source, destination, ignore=shutil.ignore_patterns("target", "node_modules", ".git"))
    shutil.copyfile(ROOT / "Cargo.lock", cargo / "Cargo.lock")
    rust_version = tomllib.loads((cargo / "Cargo.toml").read_text())["package"]["version"]
    npm_version = json.loads((npm / "package.json").read_text())["version"]
    if "-dev." not in rust_version or "-dev." not in npm_version:
        raise RuntimeError("local publishing requires explicit -dev.N package versions")
    # Check the two independent package versions against this template's pins.
    template = ROOT / "crates/yydra-cli/template/product-workspace"
    rust_pin = tomllib.loads((template / "Cargo.toml.tmpl").read_text())["workspace"]["dependencies"]["yydra-auth"]["version"]
    npm_pin = json.loads((template / "frontend/package.json").read_text().replace("__PRODUCT_SOURCE_LICENSE_TOML__", '"MIT"'))["dependencies"]["@yydra/auth-client"]
    if rust_pin != "=" + rust_version or npm_pin != npm_version:
        raise RuntimeError("package versions and Product Workspace template pins disagree")
    archives = state / "archives"
    archives.mkdir(exist_ok=True)
    env["CARGO_TARGET_DIR"] = str(state / "cargo-target")
    env["CARGO_REGISTRIES_YYDRA_LOCAL_TOKEN"] = credentials["cargo_token"]
    cargo_args = ["--config", str(ROOT / ".cargo/config.toml")]
    # Prune other workspace members while retaining the root lock's dependency versions.
    run(["cargo", "+nightly", *cargo_args, "update", "--workspace"], cwd=cargo, env=env)
    run(["cargo", "+nightly", *cargo_args, "package", "--locked", "--registry", "yydra-local"], cwd=cargo, env=env)
    crate = state / "cargo-target/package" / f"yydra-auth-{rust_version}.crate"
    crate_sha = hashlib.sha256(crate.read_bytes()).hexdigest()
    index = request(CARGO_URL + "/api/v1/crates/yy/dr/yydra-auth")
    existing = next((json.loads(line) for line in (index or b"").splitlines()
                     if json.loads(line)["vers"] == rust_version), None)
    if existing and existing["cksum"] != crate_sha:
        raise RuntimeError("yydra-auth version already has different bytes; choose a new dev version")
    if not existing:
        run(["cargo", "+nightly", *cargo_args, "publish", "--locked", "--registry", "yydra-local"], cwd=cargo, env=env)
    shutil.copyfile(crate, archives / crate.name)
    npmrc = state / "publish.npmrc"
    write_private(npmrc, f"registry={NPM_URL}/\n//127.0.0.1:4873/:_authToken={credentials['npm_token']}\n")
    env["NPM_CONFIG_USERCONFIG"] = str(npmrc)
    packed = json.loads(run(["npm", "pack", "--json", "--pack-destination", str(archives)], cwd=npm, env=env, capture=True))
    package_info = packed[0] if isinstance(packed, list) else packed["@yydra/auth-client"]
    tarball = archives / package_info["filename"]
    integrity = "sha512-" + base64.b64encode(hashlib.sha512(tarball.read_bytes()).digest()).decode()
    metadata_bytes = request(NPM_URL + "/@yydra%2fauth-client")
    metadata = json.loads(metadata_bytes) if metadata_bytes else {}
    published = metadata.get("versions", {}).get(npm_version)
    if published and published["dist"]["integrity"] != integrity:
        raise RuntimeError("auth-client version already has different bytes; choose a new dev version")
    if not published:
        run(["npm", "publish", str(tarball), "--registry", NPM_URL, "--tag", "dev", "--access", "public"], cwd=npm, env=env)
    result = {"yydra-auth": {"version": rust_version, "sha256": crate_sha},
              "@yydra/auth-client": {"version": npm_version, "integrity": integrity}}
    (archives / "packages.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError) as error:
        print(f"local-packages: {error}", file=sys.stderr)
        sys.exit(1)
