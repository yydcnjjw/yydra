<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Client settings

`@yydra/client-settings` creates typed client settings with Zustand's vanilla
store and `persist` middleware. Products define their values and persistence
options; the returned store uses Zustand's ordinary API.

```ts
import AsyncStorage from "@react-native-async-storage/async-storage";
import { createJSONStorage, createSettingsStore } from "@yydra/client-settings";

interface Settings {
  defaultSort: "oldest" | "newest";
}

export const settings = createSettingsStore<Settings>(
  { defaultSort: "oldest" },
  {
    name: "my-product:settings",
    storage: createJSONStorage(() => AsyncStorage),
    version: 1,
    skipHydration: true,
  },
);

await settings.persist.rehydrate();
await settings.setState({ defaultSort: "newest" });
settings.getState().defaultSort;
const unsubscribe = settings.subscribe((value) => console.log(value));
unsubscribe();
```

Storage is injected. For H5 and Android the product uses AsyncStorage; another
consumer can supply any Zustand-compatible storage. Ordinary JSON preferences
belong here; credentials are outside this module's contract. Use a distinct
`name` for each product/settings collection.

## React

The `/react` entry exports Zustand's `useStore` and a `useHydrated` hook based on
its hydration subscriptions. Pass the store explicitly; no global Provider is
required. Start manual hydration from the product's root effect when using
`skipHydration: true`.

```tsx
import { useHydrated, useStore } from "@yydra/client-settings/react";

function SortPreference() {
  const hydrated = useHydrated(settings);
  const sort = useStore(settings, (value) => value.defaultSort);
  return hydrated ? <span>{sort}</span> : <span>Loading settings…</span>;
}
```

`useHydrated` reports Zustand's successful hydration flag, not a general-purpose
readiness or saved-state model. Persistence errors retain Zustand's behavior.
The module adds no schema validation, damaged-data recovery, save-status state,
retry API, write queue, timeout, or late-read guard.

## Native persistence options

Pass `version`, `migrate`, `merge`, `partialize`, `onRehydrateStorage`, and
`skipHydration` directly as Zustand options. `getState`, `getInitialState`,
`setState`, `subscribe`, and the `persist` controls are unchanged. For example,
reset memory using `settings.setState(settings.getInitialState(), true)`;
`settings.persist.clearStorage()` removes stored data and is not a memory reset.

The library adds neither account isolation nor cross-tab synchronization.
Products needing either can select the corresponding storage and store setup.
See [Zustand persistence](https://zustand.docs.pmnd.rs/reference/middlewares/persist)
for the underlying contract.

## Delivery and development

The package ships TypeScript source entries for TypeScript-aware application
toolchains such as the supported Expo stack. It is not a precompiled browser
script. The CLI includes the same source as a protected Distribution snapshot
under `frontend/modules/yydra-client-settings`; product-specific declarations
remain editable in `frontend/src`.
The package's `node_modules` directory, when created by npm, is disposable
dependency installation output rather than part of that source snapshot.

Run `npm ci`, `npm test`, and `npm run typecheck` in this package. `npm pack`
produces an independently consumable package containing the source, documentation,
and both licenses.
