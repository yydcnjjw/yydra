<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# __PRODUCT_NAME__

This Product Workspace was created once from Yydra Distribution
`__YYDRA_DISTRIBUTION_VERSION__`. Its product-owned source evolves independently
after creation.

The Workspace Origin Record and `.yydra/distribution-inventory.json` record the
creation inputs, five lifecycle classes, source digests, modes, license notices,
and editing authority. The selected product-source license applies only to new
bytes authored by the Product team. Template bytes copied from Yydra remain
`MIT OR Apache-2.0`; third-party bytes retain their original terms and notices.
Exact-Distribution snapshots and committed generated output are not second
hand-edited authorities.

Creation is a one-shot boundary: there is no template rerun, no template sync,
no upgrade, no compatibility-range selection, and no Distribution-version override contract.

## Supported local path

Install exact dependency graphs and start the pinned PostgreSQL service:

```console
yydra setup .
docker compose up -d --wait postgres
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product
```

Database source and database state have separate explicit commands:

```console
yydra db migration add add_example .
yydra db migrate .
```

Server startup never applies migrations. It fails before listening unless the
database has exactly the compiled versions and checksums. After migration,
`yydra dev .` visibly starts the migration, backend, and H5 frontend phases and
terminates their process groups together on failure or shutdown.

The focused production H5 acceptance command exports static web assets, serves
them locally, and runs Playwright against the real service URL. Start the
diagnostic-only server leaf in one terminal:

```console
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product \
  cargo run --locked --bin server
```

Then run the H5 acceptance in a second terminal; do not run it alongside the
Expo development server because both use port 8081:

```console
EXPO_PUBLIC_API_URL=http://127.0.0.1:4000 npm --prefix frontend run test:e2e
```

Add `--message-format=json` before any supported Yydra subcommand for versioned
JSON Lines diagnostics. Cargo, npm, Expo, and Playwright output is forwarded as
diagnostic detail rather than becoming a second supported interface.
