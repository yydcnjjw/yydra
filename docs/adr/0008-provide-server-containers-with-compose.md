<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Provide product-owned server containers with Compose

Status: accepted — 2026-09-11.

New Product Workspaces include a Product Server Image recipe and a default
Compose deployment for the backend API and persistent PostgreSQL. One
`docker compose up --build --wait` builds the image, initializes persistent
credentials, waits for PostgreSQL, applies migrations, and waits for backend
health. Users confirmed this experience instead of introducing a remote
provisioning or SSH deployment command. The deployment host needs Docker and
Compose; the Rust compiler lives in the image builder.

The image contains the release server and migration executables from the same
product inputs. Compose owns their startup order; server startup retains its
existing migration verification and never applies migrations itself. Image
construction neither accesses a live database nor generates credentials.
The persistence package tracks the migrations directory so cached builds also
embed newly added migrations. These files are Product-owned source after creation. `yydra build` retains the
artifact-only contract of ADR 0004, and this change does not introduce template
synchronization for existing products.

Use separate named volumes for database data and initial random credentials,
reusing both across container recreation. Keep the API on host loopback by
default, with a configurable host address and port; PostgreSQL is only reachable
within the Compose network. H5, registry publication, host provisioning, and
product authentication are outside this server deployment path. Updates can
cause downtime; neither failed migrations nor application updates promise an
atomic rollback. A failed update can leave the prior server running or unhealthy.

The previous temporary PostgreSQL configuration moves to `compose.dev.yaml`.
Development uses its own default Compose project name; existing checks use
explicit isolated project names and this file, preserving their ephemeral
state and cleanup behavior. This extra file avoids letting a routine check or
its volume cleanup operate on persistent deployment data.
