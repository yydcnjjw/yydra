<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Yydra

Yydra is an agent-native, opinionated, full-stack application framework for
Coding Agents and human developers. It brings reusable application behavior,
development tools, task-oriented agent knowledge, and quality checks together
in a versioned **Yydra Distribution**.

Each Distribution creates an independently maintained **Product Workspace**.
The product team owns its codebase and expresses its distinguishing business
rules in ordinary **Product Domain** code. See the [domain glossary](CONTEXT.md)
for these terms and the framework's ownership boundaries.

## Current status

This checkout contains Distribution **0.6.0**, an unpublished development
candidate. Start with the [CLI guide](crates/yydra-cli/README.md) for its
exact-version package installation, prerequisites, and supported commands.
Rust support uses the rolling nightly channel.

Development products obtain authentication packages from [local Cargo/npm registries](dev/local-packages/README.md). Start and publish these packages before running product dependency setup.

The latest published Distribution is
[0.1.0](https://github.com/yydcnjjw/yydra/releases/tag/distribution-v0.1.0).
Its [release notes](docs/releases/0.1.0.md) describe that version's contract.
An existing Product Workspace requires the CLI for its originating Distribution;
creation does not provide a template synchronization or upgrade contract.

The current stack combines Rust, Axum, SQLx, and PostgreSQL with TypeScript,
React Native, and Expo. The build targets are the backend, an H5 (browser)
application, and an Android release APK. Default builds produce backend and H5
artifacts; Android is an explicit target. Android validation covers native
generation and release assembly; it does not establish device runtime behavior.
iOS is outside the current validation scope.

## What is included

| Component | Purpose |
| --- | --- |
| [yydra-cli](crates/yydra-cli/README.md) | Installs the `yydra` command to create, set up, diagnose, develop, and build Product Workspaces. |
| [yydra-build](crates/yydra-build/README.md) | Validates Rust-derived OpenAPI and generates and validates the TypeScript client through the product's API build package. |
| [yydra-auth](crates/yydra-auth/README.md) and [@yydra/auth](packages/auth/README.md) | Provide GitHub sign-in and independent product sessions for H5 and Android; product code owns resource authorization. |
| [client-settings](packages/client-settings/README.md) | Provides reusable typed local preferences using Zustand stores, persistence, and React subscriptions. |
| [Product Workspace template](crates/yydra-cli/template/product-workspace/README.md) | Supplies the product-owned Rust backend, Expo frontend, database migrations, and build configuration copied by `yydra new`. |

New Workspaces also receive exact-Distribution Baseline Skills for product
changes and diagnosis. `doctor` diagnoses Workspace identity and dependency
environments; quality validation uses the project's Cargo/npm tests and builds.
The [CLI guide](crates/yydra-cli/README.md) explains the commands and their scope.

## Documentation

| I want to… | Start here |
| --- | --- |
| Install Yydra and create a product | [CLI guide](crates/yydra-cli/README.md) |
| Build and run the server with Docker Compose | [Server container tutorial](docs/tutorials/server-compose.md) |
| Work inside a created product | Its generated `README.md`; preview the [Workspace guide](crates/yydra-cli/template/product-workspace/README.md) |
| Integrate API generation | [Build support guide](crates/yydra-build/README.md) |
| Understand concepts and design decisions | [Context map](CONTEXT-MAP.md), with links to the glossary and ADRs |
| Read historical research and dated validation records | [Yydra Wiki](https://github.com/yydcnjjw/yydra/wiki/Home) |
| Contribute to the framework | [Contributing guide](CONTRIBUTING.md) |
| Report a bug or request a feature | [GitHub Issues](https://github.com/yydcnjjw/yydra/issues) |

Current context definitions, ADRs, and contributor workflows live with the
code. The Wiki preserves supporting research and historical validation; use
the current decisions and implementation to establish the applicable contract.

## Contributing

Follow [CONTRIBUTING.md](CONTRIBUTING.md) for task worktrees, Conventional
Commits, cryptographic signatures, DCO sign-offs, and the PR workflow.
The [local development workflow](docs/agents/development-workflow.md) explains
which checks to run for a change. Coding Agents should also read
[AGENTS.md](AGENTS.md).

## License

Yydra-authored material is dual-licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE). Copied third-party material retains its original
terms and notices. A product's source-license choice applies to newly authored
product code; copied Yydra material keeps its original dual license.
