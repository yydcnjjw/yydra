<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Local development packages

This development Distribution consumes `yydra-auth`, `@yydra/auth`, and
`@yydra/client-settings` through local package registries. Source remains in
`capabilities/auth/rust`, `capabilities/auth/expo`, and `capabilities/client-settings/typescript`.
`yydra-build` retains its bundled source snapshot.

Prerequisites: Linux, Docker Engine with Compose, Python 3.11+, the repository's
Rust toolchain and Node/npm. Run from the Yydra checkout:

```sh
python3 scripts/local-packages.py up
python3 scripts/local-packages.py publish
python3 scripts/local-packages.py status
```

The script starts pinned Kellnr and Verdaccio images, initializes a local
publisher, and publishes the packages with the standard Cargo/npm clients.
To publish only one package, select it explicitly, for example:

```sh
python3 scripts/local-packages.py --package @yydra/client-settings publish
```

The default `publish` command handles all three packages. A selected publication
preserves the other packages' recorded identities and staging directories.
Cargo is at `http://127.0.0.1:18081`; npm is at `http://127.0.0.1:4873`.
It does not proxy upstream dependencies or publish to public registries.
Ports are fixed because product lockfiles record these development sources.
Only one instance may bind these ports on a host.

State defaults to `$XDG_DATA_HOME/yydra/local-packages`, or
`~/.local/share/yydra/local-packages`. It holds private publisher credentials,
staging directories and archives. Registry data lives in the Compose project's
named volumes. Keep the state directory and project together; changing a
bootstrap password does not change users already stored in a registry volume.
Use `--state-dir /absolute/path --project name` before the command for an isolated
instance. The state directory must be outside a Git checkout so package archives
do not depend on unrelated repository commits through Cargo VCS metadata.

The publisher keeps each version immutable: an already-published identical
archive is accepted; different bytes under the same version fail. This also
allows resuming after only one registry accepted a publication. For a library
change, choose a new `0.6.0-dev.N` version in its package manifest and the matching
Product Workspace template dependency. Update the root Cargo lock, publish, and
refresh the template's Cargo/npm locks from an isolated generated product before
building the final CLI candidate. The libraries may have different versions.
Keep tested package archives when retaining evidence or reproducing a candidate.

After publishing, create a product with the candidate CLI and run its normal
`moon run product:setup` command. Setup uses locked Cargo/npm installs and does not start
registry servers or rewrite lockfiles. `doctor` verifies package versions,
registry declarations and the expected locked package identities.

For a product server container build on Linux:

```sh
docker compose -f compose.yaml -f compose.local-registry.yaml up --build
```

The override gives the build stage access to the host loopback registry; runtime
containers keep their normal Compose network. The package registries are needed
when fetching/building dependencies, not when running an already built product.
Docker Desktop and remote builders need an explicit network/source arrangement
and are not covered by this local Linux workflow.

```sh
python3 scripts/local-packages.py down
```

Stopping retains both package volumes and credentials. There is deliberately no
automatic package-data deletion command. Public release publication and migration
to crates.io/npm sources are separate from this development workflow.

Upstream references: [Kellnr](https://github.com/kellnr/kellnr/tree/v6.8.0),
[Verdaccio](https://www.verdaccio.org/docs/configuration/),
[Cargo alternate registries](https://doc.rust-lang.org/cargo/reference/registries.html).
