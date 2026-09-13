// SPDX-License-Identifier: MIT OR Apache-2.0

import { createStore } from "zustand/vanilla";
import { persist, type PersistOptions } from "zustand/middleware";

export { createJSONStorage } from "zustand/middleware";
export type {
  PersistOptions,
  PersistStorage,
  StateStorage,
} from "zustand/middleware";

export function createSettingsStore<T extends object, U = T>(
  initialState: T,
  options: PersistOptions<T, U>,
) {
  return createStore<T>()(persist(() => initialState, options));
}
