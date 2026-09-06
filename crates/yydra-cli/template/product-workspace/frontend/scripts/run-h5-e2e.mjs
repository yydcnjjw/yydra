// SPDX-License-Identifier: MIT OR Apache-2.0

import { spawn } from "node:child_process";
import { createReadStream } from "node:fs";
import {
  lstat,
  opendir,
  readFile,
  realpath,
  stat,
  writeFile,
} from "node:fs/promises";
import { createServer } from "node:http";
import {
  extname,
  isAbsolute,
  join,
  normalize,
  relative,
  resolve,
  sep,
} from "node:path";

const frontendRoot = process.cwd();
const distributionRoot = resolve(
  frontendRoot,
  process.env.YYDRA_H5_DIST ?? "dist",
);
const h5Port = parseH5Port(process.env.YYDRA_H5_PORT ?? "8081");
process.env.YYDRA_H5_PORT = String(h5Port);
let activeChild;
let canonicalDistributionRoot;
let server;
let shutdownSignal;

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    shutdownSignal ??= signal;
    void terminateActiveChild();
    server?.close();
  });
}

try {
  await main();
} catch (error) {
  if (shutdownSignal === undefined) {
    throw error;
  }
} finally {
  await terminateActiveChild();
  if (server?.listening) {
    await closeServer(server);
  }
}

if (shutdownSignal !== undefined) {
  process.exitCode = 128 + (shutdownSignal === "SIGINT" ? 2 : 15);
}

async function main() {
  const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm";
  await run(npmCommand, [
    "exec",
    "--offline",
    "--",
    "expo",
    "export",
    "--clear",
    "--platform",
    "web",
    "--output-dir",
    distributionRoot,
    "--source-maps",
  ]);
  if (shutdownSignal !== undefined) {
    return;
  }
  canonicalDistributionRoot = await realpath(distributionRoot);
  await bindExportedSourceMaps(canonicalDistributionRoot);

  server = createServer(async (request, response) => {
    try {
      const requestUrl = new URL(request.url ?? "/", "http://127.0.0.1");
      const requestPath =
        requestUrl.pathname === "/"
          ? "index.html"
          : requestUrl.pathname.slice(1);
      const candidate = normalize(join(distributionRoot, requestPath));
      const candidateRelative = relative(distributionRoot, candidate);
      if (
        !isContainedRelativePath(
          candidateRelative,
          sep,
          isAbsolute(candidateRelative),
        )
      ) {
        response.writeHead(400).end("invalid path");
        return;
      }
      const canonicalCandidate = await realpath(candidate);
      const canonicalRelative = relative(
        canonicalDistributionRoot,
        canonicalCandidate,
      );
      if (
        !isContainedRelativePath(
          canonicalRelative,
          sep,
          isAbsolute(canonicalRelative),
        )
      ) {
        response.writeHead(400).end("invalid path");
        return;
      }
      const metadata = await stat(canonicalCandidate);
      const requestedFile = metadata.isDirectory()
        ? join(canonicalCandidate, "index.html")
        : canonicalCandidate;
      const file = await realpath(requestedFile);
      const fileRelative = relative(canonicalDistributionRoot, file);
      if (
        !isContainedRelativePath(fileRelative, sep, isAbsolute(fileRelative))
      ) {
        response.writeHead(400).end("invalid path");
        return;
      }
      response.writeHead(200, { "content-type": contentType(file) });
      createReadStream(file).pipe(response);
    } catch {
      response.writeHead(404).end("not found");
    }
  });

  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(h5Port, "127.0.0.1", resolveListen);
  });

  try {
    const playwrightCli = join(
      frontendRoot,
      "node_modules",
      "@playwright",
      "test",
      "cli.js",
    );
    await run(process.execPath, [
      playwrightCli,
      "test",
      process.env.YYDRA_PLAYWRIGHT_SPEC ?? "e2e/clean-workspace.spec.ts",
      "--config",
      process.env.YYDRA_PLAYWRIGHT_CONFIG ?? "playwright.config.mts",
      "--forbid-only",
      "--workers=1",
      "--retries=0",
    ]);
  } finally {
    await closeServer(server);
  }
}

// Metro identifies matching exports with debugId but omits the optional map.file.
// Complete that declaration only after checking the unmodified pair, before the
// browser tests and retained checksums. Never repair contradictory metadata.
async function bindExportedSourceMaps(root) {
  const maps = [];
  let entries = 0;
  async function visit(directory, depth = 0) {
    if (depth > 64) {
      throw new Error(
        "H5_SOURCE_MAP_BINDING_INVALID: export exceeds depth limit",
      );
    }
    for await (const entry of await opendir(directory)) {
      entries += 1;
      if (entries > 100_000 || (!entry.isDirectory() && !entry.isFile())) {
        throw new Error(
          "H5_SOURCE_MAP_BINDING_INVALID: unsupported export entry or count",
        );
      }
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await visit(path, depth + 1);
      else if (entry.name.endsWith(".map")) {
        maps.push(path);
        if (maps.length > 100) {
          throw new Error(
            "H5_SOURCE_MAP_BINDING_INVALID: map count exceeds 100",
          );
        }
      }
    }
  }
  await visit(root);
  if (maps.length === 0 || maps.length > 100) {
    throw new Error("H5_SOURCE_MAP_BINDING_INVALID: expected 1 to 100 maps");
  }
  let totalBytes = 0;
  for (const path of maps.sort()) {
    const fail = () => {
      throw new Error(`H5_SOURCE_MAP_BINDING_INVALID: ${relative(root, path)}`);
    };
    const bundlePath = path.slice(0, -4);
    const mapMetadata = await lstat(path).catch(fail);
    const bundleMetadata = await lstat(bundlePath).catch(fail);
    if (!mapMetadata.isFile() || !bundleMetadata.isFile()) fail();
    const mapSize = mapMetadata.size;
    const bundleSize = bundleMetadata.size;
    totalBytes += mapSize;
    if (
      mapSize > 128 * 1024 * 1024 ||
      totalBytes > 256 * 1024 * 1024 ||
      bundleSize > 256 * 1024 * 1024
    )
      fail();
    const map = JSON.parse(await readFile(path, "utf8"));
    const bundle = await readFile(bundlePath, "utf8");
    const mapName = relative(root, path).split(sep).join("/");
    const bundleName = mapName.split("/").at(-1).slice(0, -4);
    const urls = [
      ...bundle.matchAll(/^\/\/# sourceMappingURL=([^\r\n]+)$/gm),
    ].map((match) => match[1]);
    if (
      urls.length !== 1 ||
      (urls[0] !== `/${mapName}` && urls[0] !== `${bundleName}.map`)
    )
      fail();
    if (
      map.version !== 3 ||
      (map.file !== undefined && map.file !== bundleName)
    )
      fail();
    const debugIds = [...bundle.matchAll(/^\/\/# debugId=([^\r\n]+)$/gm)].map(
      (match) => match[1],
    );
    if (
      map.file === undefined ||
      map.debugId !== undefined ||
      debugIds.length !== 0
    ) {
      if (
        typeof map.debugId !== "string" ||
        !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
          map.debugId,
        ) ||
        debugIds.length !== 1 ||
        debugIds[0] !== map.debugId
      )
        fail();
    }
    if (map.file === undefined) {
      map.file = bundleName;
      await writeFile(path, `${JSON.stringify(map)}\n`);
    }
  }
}

function run(program, args) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(program, args, {
      cwd: frontendRoot,
      detached: process.platform !== "win32",
      stdio: "inherit",
    });
    activeChild = child;
    child.once("error", (error) => {
      if (activeChild === child) {
        activeChild = undefined;
      }
      rejectRun(error);
    });
    child.once("exit", async (code, signal) => {
      if (code === 0) {
        if (activeChild === child) {
          activeChild = undefined;
        }
        resolveRun();
      } else {
        await terminateProcessTree(child);
        if (activeChild === child) {
          activeChild = undefined;
        }
        rejectRun(new Error(`${program} exited with ${code ?? signal}`));
      }
    });
  });
}

async function terminateActiveChild() {
  const child = activeChild;
  if (child === undefined) {
    return;
  }
  await terminateProcessTree(child);
  if (activeChild === child) {
    activeChild = undefined;
  }
}

async function terminateProcessTree(child) {
  if (child.pid === undefined) {
    return;
  }
  if (process.platform === "win32") {
    await new Promise((resolveTaskkill) => {
      const taskkill = spawn(
        "taskkill.exe",
        ["/pid", String(child.pid), "/t", "/f"],
        {
          stdio: "ignore",
        },
      );
      taskkill.once("error", resolveTaskkill);
      taskkill.once("exit", resolveTaskkill);
    });
    return;
  }
  try {
    process.kill(-child.pid, "SIGTERM");
  } catch (error) {
    if (error.code !== "ESRCH") {
      throw error;
    }
  }
  await Promise.race([
    new Promise((resolveExit) => child.once("exit", resolveExit)),
    new Promise((resolveTimeout) => setTimeout(resolveTimeout, 1000)),
  ]);
  try {
    process.kill(-child.pid, "SIGKILL");
  } catch (error) {
    if (error.code !== "ESRCH") {
      throw error;
    }
  }
}

function closeServer(httpServer) {
  return new Promise((resolveClose, rejectClose) => {
    httpServer.close((error) => (error ? rejectClose(error) : resolveClose()));
  });
}

function contentType(path) {
  return (
    {
      ".css": "text/css; charset=utf-8",
      ".html": "text/html; charset=utf-8",
      ".js": "text/javascript; charset=utf-8",
      ".json": "application/json; charset=utf-8",
      ".map": "application/json; charset=utf-8",
      ".png": "image/png",
      ".svg": "image/svg+xml",
    }[extname(path)] ?? "application/octet-stream"
  );
}

function isContainedRelativePath(relativePath, separator, absolute) {
  return (
    !absolute &&
    relativePath !== ".." &&
    !relativePath.startsWith(`..${separator}`)
  );
}

function parseH5Port(raw) {
  if (!/^[1-9][0-9]*$/.test(raw)) {
    throw new Error("YYDRA_H5_PORT must be an integer from 1 through 65535");
  }
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("YYDRA_H5_PORT must be an integer from 1 through 65535");
  }
  return port;
}
