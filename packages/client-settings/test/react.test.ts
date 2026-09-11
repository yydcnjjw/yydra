// SPDX-License-Identifier: MIT OR Apache-2.0
// @vitest-environment jsdom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { createSettingsStore } from "../src/index.ts";
import { useHydrated, useStore } from "../src/react.ts";

afterEach(cleanup);

it("observes hydration and settings using the supplied store", async () => {
  const store = createSettingsStore(
    { language: "en" },
    {
      name: "react-reader",
      skipHydration: true,
      storage: {
        getItem: async () => ({ state: { language: "zh" }, version: 0 }),
        setItem: async () => {},
        removeItem: async () => {},
      },
    },
  );
  const { result, unmount } = renderHook(() => ({
    hydrated: useHydrated(store),
    language: useStore(store, (state) => state.language),
  }));
  expect(result.current).toEqual({ hydrated: false, language: "en" });
  await act(async () => {
    await store.persist.rehydrate();
  });
  expect(result.current).toEqual({ hydrated: true, language: "zh" });
  await act(async () => {
    await store.setState({ language: "fr" });
  });
  expect(result.current.language).toBe("fr");
  unmount();
});
