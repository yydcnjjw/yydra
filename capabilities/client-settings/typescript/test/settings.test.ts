// SPDX-License-Identifier: MIT OR Apache-2.0

import { describe, expect, it } from "vitest";
import { createJSONStorage, createSettingsStore } from "../src/index.ts";

describe("settings package", () => {
  it("keeps product schemas isolated and reopens a stored preference", async () => {
    const values = new Map<string, string>();
    const driver = {
      getItem: async (name: string) => values.get(name) ?? null,
      setItem: async (name: string, value: string) => {
        values.set(name, value);
      },
      removeItem: async (name: string) => {
        values.delete(name);
      },
    };
    const createSort = () =>
      createSettingsStore<{ sort: "oldest" | "newest" }>(
        { sort: "oldest" },
        {
          name: "reader",
          storage: createJSONStorage(() => driver),
          version: 1,
          skipHydration: true,
        },
      );
    const reader = createSort();
    const other = createSettingsStore(
      { language: "en" },
      {
        name: "other-product",
        storage: createJSONStorage(() => driver),
        skipHydration: true,
      },
    );
    await reader.persist.rehydrate();
    await reader.setState({ sort: "newest" });
    const reopened = createSort();
    await reopened.persist.rehydrate();
    expect(reopened.getState()).toEqual({ sort: "newest" });
    expect(other.getState()).toEqual({ language: "en" });
    expect(values.has("other-product")).toBe(false);
  });

  it("passes native migration and manual hydration options through", async () => {
    let stored = JSON.stringify({ state: { oldSort: "newest" }, version: 0 });
    const store = createSettingsStore(
      { sort: "oldest" },
      {
        name: "versioned-reader",
        version: 1,
        skipHydration: true,
        storage: createJSONStorage(() => ({
          getItem: async () => stored,
          setItem: async (_name, value) => {
            stored = value;
          },
          removeItem: async () => {},
        })),
        migrate: (value, version) => {
          expect(version).toBe(0);
          return { sort: (value as { oldSort: string }).oldSort };
        },
      },
    );
    expect(store.persist.hasHydrated()).toBe(false);
    expect(store.getState()).toEqual({ sort: "oldest" });
    await store.persist.rehydrate();
    expect(store.persist.hasHydrated()).toBe(true);
    expect(store.getState()).toEqual({ sort: "newest" });
    expect(JSON.parse(stored)).toEqual({
      state: { sort: "newest" },
      version: 1,
    });
  });
});
