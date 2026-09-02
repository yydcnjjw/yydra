// SPDX-License-Identifier: MIT OR Apache-2.0

import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [".expo/**", "dist/**", "node_modules/**", "test-results/**"],
  },
  {
    files: ["**/*.{js,mjs,ts,tsx,mts}"],
    extends: [eslint.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      globals: {
        __dirname: "readonly",
        console: "readonly",
        process: "readonly",
      },
    },
  },
);
