// SPDX-License-Identifier: MIT OR Apache-2.0

import path from "node:path";
import fs from "node:fs";
import metro from "expo/metro-config.js";
import { generatedApiDirectory } from "./scripts/generated-api.mjs";

const config = metro.getDefaultConfig(import.meta.dirname);
config.watchFolders = [...config.watchFolders, generatedApiDirectory()];
config.resolver.nodeModulesPaths = [
  path.join(import.meta.dirname, "node_modules"),
];

const sourceRecord = path.join(
  import.meta.dirname,
  "../.yydra/source-workspace.json",
);
if (fs.existsSync(sourceRecord)) {
  const { framework_root: framework } = JSON.parse(
    fs.readFileSync(sourceRecord, "utf8"),
  );
  config.watchFolders.push(
    path.join(framework, "capabilities/auth/expo"),
    path.join(framework, "capabilities/client-settings/typescript"),
  );
  // Resolve peers from this application even when the linked libraries have
  // their own test dependencies installed in the framework checkout.
  config.resolver.disableHierarchicalLookup = true;
}

export default config;
