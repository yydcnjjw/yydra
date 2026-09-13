// SPDX-License-Identifier: MIT OR Apache-2.0

import { useCallback, useSyncExternalStore } from "react";

export { useStore } from "zustand";

interface HydratingStore {
  persist: {
    hasHydrated(): boolean;
    onHydrate(listener: () => void): () => void;
    onFinishHydration(listener: () => void): () => void;
  };
}

export function useHydrated(store: HydratingStore): boolean {
  const subscribe = useCallback(
    (listener: () => void) => {
      const stopStarting = store.persist.onHydrate(listener);
      const stopFinishing = store.persist.onFinishHydration(listener);
      return () => {
        stopStarting();
        stopFinishing();
      };
    },
    [store],
  );
  return useSyncExternalStore(
    subscribe,
    store.persist.hasHydrated,
    () => false,
  );
}
