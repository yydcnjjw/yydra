// SPDX-License-Identifier: MIT OR Apache-2.0

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { linkGeneratedApi } from "./generated-api.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const manifest = path.join(root, "crates/api-build/Cargo.toml");

function cargo(args) {
  const result = spawnSync(process.env.CARGO || "cargo", args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.stderr) process.stderr.write(result.stderr);
  if (result.error) throw result.error;
  if (result.status !== 0) {
    for (const line of (result.stdout || "").split("\n")) {
      if (!line.startsWith("{")) continue;
      try {
        const diagnostic = JSON.parse(line);
        if (diagnostic.message?.rendered) {
          process.stderr.write(diagnostic.message.rendered);
        }
      } catch {
        /* Cargo may also forward non-JSON tool output. */
      }
    }
    throw new Error(`API_BUILD_FAILED: cargo exited with ${result.status}`);
  }
  return result.stdout;
}

const metadata = JSON.parse(
  cargo([
    "metadata",
    "--locked",
    "--no-deps",
    "--format-version=1",
    "--manifest-path",
    manifest,
  ]),
);
const packageId = metadata.packages.find(
  (item) => path.resolve(item.manifest_path) === path.resolve(manifest),
)?.id;
if (!packageId)
  throw new Error("API_WORKSPACE_INVALID: api-build package missing");

function build() {
  const output = cargo([
    "build",
    "--locked",
    "--package",
    packageId,
    "--message-format=json",
  ]);
  let outDir;
  let apiOutput;
  for (const line of output.split("\n")) {
    if (!line.startsWith("{")) continue;
    const message = JSON.parse(line);
    if (
      message.reason === "build-script-executed" &&
      message.package_id === packageId
    ) {
      outDir = message.out_dir;
      apiOutput = message.env?.find(([key]) => key === "YYDRA_API_OUTPUT")?.[1];
    }
  }
  if (
    !outDir ||
    !path.isAbsolute(outDir) ||
    !apiOutput ||
    !path.isAbsolute(apiOutput) ||
    path.relative(outDir, apiOutput).startsWith("..")
  ) {
    throw new Error("API_OUTPUT_MISSING: Cargo did not report the API OUT_DIR");
  }
  return apiOutput;
}

function complete(output) {
  try {
    const client = path.join(output, "public-api");
    const pkg = JSON.parse(fs.readFileSync(path.join(client, "package.json")));
    if (
      pkg.name !== "@yydra/generated-api" ||
      !Array.isArray(pkg.files) ||
      !pkg.files.includes("fetch/client.ts")
    ) {
      return false;
    }
    const regularFile = (file) => fs.lstatSync(file).isFile();
    return (
      regularFile(path.join(output, "openapi.json")) &&
      regularFile(path.join(output, "generated-client-tsconfig.json")) &&
      pkg.files.every((relative) => {
        if (typeof relative !== "string" || path.isAbsolute(relative))
          return false;
        const file = path.resolve(client, relative);
        return (
          !path.relative(client, file).startsWith("..") && regularFile(file)
        );
      })
    );
  } catch {
    return false;
  }
}

let output = build();
if (!complete(output)) {
  cargo(["clean", "--locked", "--package", packageId]);
  output = build();
  if (!complete(output)) {
    throw new Error(
      "API_OUTPUT_MISSING: API outputs are incomplete after rebuild",
    );
  }
}
linkGeneratedApi(path.join(output, "public-api"));
