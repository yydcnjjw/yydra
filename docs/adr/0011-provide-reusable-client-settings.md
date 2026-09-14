<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Provide reusable client settings

Status: accepted — 2026-09-11.

## Context

Products need ordinary local preferences, such as default sorting, retained
across application restarts. Product Workspaces define the settings and their
meaning; a maintained package supplies the common store and persistence setup.
Settings belong to the application installation or browser storage space.
Account partitioning, cloud synchronization, and credential storage are outside
this capability.

## Decision

Provide `@yydra/client-settings` in `packages/client-settings`, using Zustand
5.0.15 (MIT). The main entry combines the vanilla store with `persist`; the
`/react` entry exports Zustand's `useStore` and a small `useHydrated` hook based
on native hydration subscriptions. Consumers pass their store explicitly.

`createSettingsStore(initialState, options)` accepts product-defined defaults
and Zustand persistence options. It returns the native store, including
`getState`, `getInitialState`, `setState`, `subscribe`, and the `persist`
controls. Products own the storage key, version, migration, and storage adapter.
Use Zustand's existing `createJSONStorage`, `version`, `migrate`, `merge`,
`partialize`, `onRehydrateStorage`, and `skipHydration` options directly.

Reuse the library's state, subscriptions, hydration, and version migration
instead of maintaining those mechanisms in Yydra. Jotai's storage utility would
need additional versioning policy; Redux Persist requires a Redux store model;
Legend-State would introduce a different reactive model. Zustand fits the
selected explicit-store React integration and requires little wrapper code.
See the [fixed release](https://github.com/pmndrs/zustand/releases/tag/v5.0.15)
and [persist implementation](https://github.com/pmndrs/zustand/blob/v5.0.15/src/middleware/persist.ts).

Keep native Zustand behavior for persistence failures and concurrent operations.
The package does not add schema validation, damaged-data recovery, saving or
unsaved status, explicit retry, ordered asynchronous writes, read timeouts, or
protection against late hydration overwriting a new change. These mechanisms
are deliberately excluded from this contract. A successful memory update does
not establish that persistence completed. `useHydrated` reports successful
native hydration; it does not turn an error into application readiness.

In the template, inject AsyncStorage 2.2.0 through `createJSONStorage` for H5
and Android. Start manual hydration from the root effect, and display a loading
view until native hydration succeeds. This can remain loading if hydration
fails; no fallback or recovery policy is supplied. Use ordinary JSON values
and a product-specific storage key. No cross-tab synchronization or conflict
resolution is added.

## Delivery and product integration

Ship TypeScript source entries for TypeScript-aware application toolchains,
including the supported Expo stack. The package is independently packable and
includes both licenses.

Amended on 2026-09-12: follow the authentication capability's local package
release workflow. Publish `@yydra/client-settings` to the existing Verdaccio
registry with `scripts/local-packages.py`; the first development version is
`0.6.0-dev.1`. Each changed publication needs a new `-dev.N` version. The
publisher accepts identical archives and rejects changed bytes under an
existing version. It can publish one selected package or all maintained packages.

Generated products declare an exact npm version and lock its registry source
and integrity. The CLI no longer embeds the settings library's source.
`doctor` checks the package declaration and locked identity, using the same
registry configuration as `@yydra/auth`. Products edit their declarations and
assembly in `frontend/src`; the reusable implementation stays in Yydra.
Existing Workspaces are not rewritten. Local registry services are required
when fetching dependencies, and public npm publication remains a separate action.

The Reading Queue remembers its default sort. A valid explicit URL sort takes
precedence for the current view. Visiting a link does not save that sort.
Explicitly choosing a sort changes both the route and preference; changing a
status filter alone does not change the preference. Theme and language remain
examples of settings a product can define, rather than new presentation features.

## Validation and consequences

- Exercise different product schemas, persistence across store recreation,
  native migration options, and React hydration and update subscriptions.
- Verify exports using an actual npm package artifact.
- Verify packaged CLI creation and rejection of changed package versions,
  registry sources, or locked identities. Check licenses and dependency locks.
- Run affected frontend checks and H5 acceptance for reopening, URL precedence,
  and explicit sort versus filter changes in a fresh Product Workspace.
- Run an Android release build because AsyncStorage adds a native dependency.
  Compilation does not establish persistence on a running Android device.

After ADR 0009 retired the quality graph, run explicit package, consumer, H5,
and Android checks and report the actual candidate, results, and coverage limits.
Doctor verifies Workspace identity and snapshots; it does not run validation.
Historical graph evidence retains its original `pass-selected` / `complete: false`
scope and does not establish validation of a later candidate.

## Subsequent Capability and moon migration

[ADR 0012](0012-aggregate-capabilities-and-adopt-moon.md), accepted on 2026-09-13,
amends the applicable directory, developer-entrypoint, and explicit source-consumer
contracts. The original decision and its dated validation retain their historical scope.
