// SPDX-License-Identifier: MIT OR Apache-2.0

import fs from "node:fs";
import path from "node:path";

const frontend = path.resolve(import.meta.dirname, "..");
const packagePath = path.join(frontend, "node_modules/@yydra/generated-api");

export function generatedApiDirectory() {
  return fs.realpathSync(packagePath);
}

export function linkGeneratedApi(output) {
  if (!output || !path.isAbsolute(output)) {
    throw new Error("Expected the absolute generated API build directory");
  }
  fs.mkdirSync(path.dirname(packagePath), { recursive: true });
  const existing = fs.lstatSync(packagePath, { throwIfNoEntry: false });
  if (existing) {
    if (!existing.isSymbolicLink()) {
      throw new Error(`Generated API package path is occupied: ${packagePath}`);
    }
    fs.unlinkSync(packagePath);
  }
  fs.symlinkSync(
    output,
    packagePath,
    process.platform === "win32" ? "junction" : "dir",
  );
}
