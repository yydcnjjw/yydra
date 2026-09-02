<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# yydra-cli

`yydra-cli` installs the exact-version `yydra` executable for creating and
diagnosing Product Workspaces initialized from a Yydra Distribution and owned
independently by their product teams.

Install an exact release while preserving its packaged lockfile:

```console
cargo install yydra-cli --version 0.1.0 --locked
```

Create once with one normalized product-source license choice:

```console
yydra new ./reader \
  --product-name "Reader" \
  --product-id reader \
  --product-source-license Apache-2.0
```

The choice governs only source newly authored by the Product team. Copied Yydra
bytes remain `MIT OR Apache-2.0`, third-party bytes retain their original terms
and notices, and both complete Yydra license texts ship with the package and the
new Workspace. `.yydra/distribution-inventory.json` records lifecycle,
provenance, mode, digest, license-notice authority, and edit authority.

Continue only through supported commands:

```console
yydra setup ./reader
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
  yydra db migrate ./reader
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
  yydra dev ./reader
```

`setup` consumes both committed locks. Migration creation (`yydra db migration
add`) and application are explicit; the server only verifies that PostgreSQL
matches its compiled history. Add `--message-format=json` before the subcommand
for versioned JSON Lines diagnostics.

Creation does not establish a template rerun, synchronization, upgrade,
compatibility-range, or Distribution-version override contract.
