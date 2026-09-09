// SPDX-License-Identifier: MIT OR Apache-2.0

import { fileURLToPath, URL } from "node:url";

import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    preserveSymlinks: true,
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      "@react-native-community/netinfo": fileURLToPath(
        new URL("./src/framework/testing/netinfo.ts", import.meta.url),
      ),
      "react-native": "react-native-web",
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx,mjs}"],
  },
});
