#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outputPath = resolve(
  repository,
  "crates/yydra-cli/supply-chain/cli-graph.json",
);
// Preserve already bundled text as unreviewed metadata. Generation does not
// scan dependency sources or certify notice completeness under Issues #26/#39.
const retainedPackages = new Map(
  (existsSync(outputPath) ? JSON.parse(readFileSync(outputPath, "utf8")).packages : [])
    .map((pkg) => [pkg.purl, pkg]),
);
const compareText = (left, right) => (left < right ? -1 : left > right ? 1 : 0);
const arguments_ = process.argv.slice(2);
if (arguments_.some((argument) => argument !== "--check")) {
  throw new Error("usage: generate-cli-supply-chain.mjs [--check]");
}
const host = execFileSync("rustc", ["-vV"], {
  cwd: repository,
  encoding: "utf8",
})
  .split("\n")
  .find((line) => line.startsWith("host: "))
  ?.slice("host: ".length);
if (!host) throw new Error("rustc did not report its host target");
const metadata = JSON.parse(
  execFileSync(
    "cargo",
    [
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--filter-platform",
      host,
      "--all-features",
    ],
    { cwd: repository, encoding: "utf8" },
  ),
);

const lock = readFileSync(resolve(repository, "Cargo.lock"), "utf8");
const checksums = new Map();
for (const block of lock.split("[[package]]").slice(1)) {
  const name = block.match(/^\s*name = "([^"]+)"/m)?.[1];
  const version = block.match(/^\s*version = "([^"]+)"/m)?.[1];
  const source = block.match(/^\s*source = "([^"]+)"/m)?.[1] ?? "workspace-path";
  const checksum = block.match(/^\s*checksum = "([^"]+)"/m)?.[1] ?? null;
  if (name && version) checksums.set(`${name}\0${version}\0${source}`, checksum);
}

const resolved = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
const packageById = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
const root = metadata.workspace_members.find(
  (id) => packageById.get(id)?.name === "yydra-cli",
);
if (!root) throw new Error("could not find the yydra-cli workspace package");

const reachable = new Set();
const visit = (id) => {
  if (reachable.has(id)) return;
  reachable.add(id);
  for (const dependency of resolved.get(id)?.dependencies ?? []) visit(dependency);
};
visit(root);

const normalizeSource = (pkg) =>
  pkg.source ??
  (pkg.manifest_path.startsWith(repository) ? "workspace-path" : "unknown-path");
const purl = (pkg) =>
  `pkg:cargo/${encodeURIComponent(pkg.name)}@${encodeURIComponent(pkg.version)}`;
const stablePackageRoot = (pkg) =>
  pkg.manifest_path.startsWith(repository)
    ? relative(repository, dirname(pkg.manifest_path)) || "."
    : `cargo-registry/${pkg.name}-${pkg.version}`;
const stablePackagePath = (pkg, path) => {
  if (!path) return null;
  const packageRoot = dirname(pkg.manifest_path);
  const absolute = isAbsolute(path) ? path : resolve(packageRoot, path);
  const inside = relative(packageRoot, absolute);
  if (inside.startsWith("..")) return `${stablePackageRoot(pkg)}/${basename(path)}`;
  return `${stablePackageRoot(pkg)}/${inside}`;
};
const packages = [...reachable]
  .map((id) => {
    const pkg = packageById.get(id);
    const node = resolved.get(id);
    const source = normalizeSource(pkg);
    const checksum = checksums.get(`${pkg.name}\0${pkg.version}\0${source}`) ?? null;
    const retained = retainedPackages.get(purl(pkg));
    return {
      id: purl(pkg),
      purl: purl(pkg),
      name: pkg.name,
      version: pkg.version,
      source,
      checksum,
      declaredLicense: pkg.license,
      licenseFile: stablePackagePath(pkg, pkg.license_file),
      manifestPath: stablePackagePath(pkg, pkg.manifest_path),
      features: [...(node?.features ?? [])].sort(),
      targetKinds: [...new Set(pkg.targets.flatMap((target) => target.kind))].sort(
        compareText,
      ),
      notices: retained?.source === source && retained?.checksum === checksum
        ? retained.notices ?? [] : [],
    };
  })
  .sort((left, right) => compareText(left.purl, right.purl));

const dependencies = [...reachable]
  .map((id) => ({
    from: purl(packageById.get(id)),
    to: (resolved.get(id)?.deps ?? [])
      .filter((dependency) => reachable.has(dependency.pkg))
      .map((dependency) => ({
        purl: purl(packageById.get(dependency.pkg)),
        kinds: dependency.dep_kinds
          .map((kind) => ({
            kind: kind.kind ?? "normal",
            target: kind.target,
          }))
          .sort((left, right) =>
            compareText(
              `${left.kind}:${left.target ?? ""}`,
              `${right.kind}:${right.target ?? ""}`,
            ),
          ),
      }))
      .sort((left, right) => compareText(left.purl, right.purl)),
  }))
  .sort((left, right) => compareText(left.from, right.from));

const output = {
  schemaVersion: 2,
  generatedBy: "scripts/generate-cli-supply-chain.mjs",
  target: host,
  root: purl(packageById.get(root)),
  packages,
  dependencies,
};
const serialized = `${JSON.stringify(output, null, 2)}\n`;
if (arguments_.includes("--check")) {
  if (readFileSync(outputPath, "utf8") !== serialized) {
    throw new Error(
      "embedded CLI supply-chain graph is stale; run scripts/generate-cli-supply-chain.mjs",
    );
  }
} else {
  writeFileSync(outputPath, serialized);
}
