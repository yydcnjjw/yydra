// SPDX-License-Identifier: MIT OR Apache-2.0

import AsyncStorage from "@react-native-async-storage/async-storage";
import { createJSONStorage, createSettingsStore } from "@yydra/client-settings";
import type { ReadingQueueSort } from "./reading-queue/queries";

interface ClientSettings {
  defaultSort: ReadingQueueSort;
}

export const clientSettings = createSettingsStore<ClientSettings>(
  { defaultSort: "oldest" },
  {
    name: "__PRODUCT_ID__:client-settings",
    storage: createJSONStorage(() => AsyncStorage),
    version: 1,
    skipHydration: true,
  },
);
