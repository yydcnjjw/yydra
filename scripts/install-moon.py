#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Install the pinned Linux CI moon binary into an explicit private directory."""

import argparse
import hashlib
from pathlib import Path
import platform
import tarfile
import urllib.request

VERSION = "2.5.4"
ARCHIVE = "moon_cli-x86_64-unknown-linux-gnu.tar.xz"
SHA256 = "baa6f0cda8fe9d7513ffebb2908cfd952888c66659b795da02a14ef2156eb5c7"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "AMD64"):
        parser.error("this CI installer supports Linux x86_64; use moon's versioned release for other hosts")
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    archive = destination / ARCHIVE
    url = f"https://github.com/moonrepo/moon/releases/download/v{VERSION}/{ARCHIVE}"
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != SHA256:
        raise RuntimeError("moon release archive checksum mismatch")
    archive.write_bytes(data)
    with tarfile.open(archive) as package:
        package.extractall(destination, filter="data")
    executable = destination / ARCHIVE.removesuffix(".tar.xz") / "moon"
    executable.rename(destination / "moon")
    print(destination / "moon")


if __name__ == "__main__":
    main()
