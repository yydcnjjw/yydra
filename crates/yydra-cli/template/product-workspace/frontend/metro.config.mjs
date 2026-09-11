// SPDX-License-Identifier: MIT OR Apache-2.0

import path from "node:path";
import metro from "expo/metro-config.js";
import { generatedApiDirectory } from "./scripts/generated-api.mjs";

const config = metro.getDefaultConfig(import.meta.dirname);
config.watchFolders = [
  ...config.watchFolders,
  generatedApiDirectory(),
  path.resolve(import.meta.dirname, "../.yydra/auth-client"),
];
config.resolver.nodeModulesPaths = [
  path.join(import.meta.dirname, "node_modules"),
];

export default config;
