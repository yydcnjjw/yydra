// SPDX-License-Identifier: MIT OR Apache-2.0

import { spawn } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import { createServer } from 'node:http';
import { extname, isAbsolute, join, normalize, relative, resolve, sep } from 'node:path';

import { isContainedRelativePath, parseH5Port } from '../src/framework/path-containment.mjs';

const frontendRoot = process.cwd();
const distributionRoot = resolve(frontendRoot, 'dist');
const h5Port = parseH5Port(process.env.YYDRA_H5_PORT ?? '8081');
process.env.YYDRA_H5_PORT = String(h5Port);
let activeChild;
let server;
let shutdownSignal;

for (const signal of ['SIGINT', 'SIGTERM']) {
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
  process.exitCode = 128 + (shutdownSignal === 'SIGINT' ? 2 : 15);
}

async function main() {
  const npmCli = process.env.npm_execpath;
  if (npmCli === undefined) {
    throw new Error('run the production H5 acceptance through `npm run test:e2e`');
  }
  await run(process.execPath, [npmCli, 'run', 'export:h5']);
  if (shutdownSignal !== undefined) {
    return;
  }

  server = createServer(async (request, response) => {
    try {
      const requestUrl = new URL(request.url ?? '/', 'http://127.0.0.1');
      const requestPath = requestUrl.pathname === '/' ? 'index.html' : requestUrl.pathname.slice(1);
      const candidate = normalize(join(distributionRoot, requestPath));
      const candidateRelative = relative(distributionRoot, candidate);
      if (!isContainedRelativePath(candidateRelative, sep, isAbsolute(candidateRelative))) {
        response.writeHead(400).end('invalid path');
        return;
      }
      const metadata = await stat(candidate);
      const file = metadata.isDirectory() ? join(candidate, 'index.html') : candidate;
      response.writeHead(200, { 'content-type': contentType(file) });
      createReadStream(file).pipe(response);
    } catch {
      response.writeHead(404).end('not found');
    }
  });

  await new Promise((resolveListen, rejectListen) => {
    server.once('error', rejectListen);
    server.listen(h5Port, '127.0.0.1', resolveListen);
  });

  try {
    const playwrightCli = join(
      frontendRoot,
      'node_modules',
      '@playwright',
      'test',
      'cli.js',
    );
    await run(process.execPath, [
      playwrightCli,
      'test',
      'e2e/clean-workspace.spec.ts',
      '--forbid-only',
      '--workers=1',
      '--retries=0',
    ]);
  } finally {
    await closeServer(server);
  }
}

function run(program, args) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(program, args, {
      cwd: frontendRoot,
      detached: process.platform !== 'win32',
      stdio: 'inherit',
    });
    activeChild = child;
    child.once('error', (error) => {
      if (activeChild === child) {
        activeChild = undefined;
      }
      rejectRun(error);
    });
    child.once('exit', async (code, signal) => {
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
  if (process.platform === 'win32') {
    await new Promise((resolveTaskkill) => {
      const taskkill = spawn('taskkill.exe', ['/pid', String(child.pid), '/t', '/f'], {
        stdio: 'ignore',
      });
      taskkill.once('error', resolveTaskkill);
      taskkill.once('exit', resolveTaskkill);
    });
    return;
  }
  try {
    process.kill(-child.pid, 'SIGTERM');
  } catch (error) {
    if (error.code !== 'ESRCH') {
      throw error;
    }
  }
  await Promise.race([
    new Promise((resolveExit) => child.once('exit', resolveExit)),
    new Promise((resolveTimeout) => setTimeout(resolveTimeout, 1000)),
  ]);
  try {
    process.kill(-child.pid, 'SIGKILL');
  } catch (error) {
    if (error.code !== 'ESRCH') {
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
      '.css': 'text/css; charset=utf-8',
      '.html': 'text/html; charset=utf-8',
      '.js': 'text/javascript; charset=utf-8',
      '.json': 'application/json; charset=utf-8',
      '.map': 'application/json; charset=utf-8',
      '.png': 'image/png',
      '.svg': 'image/svg+xml',
    }[extname(path)] ?? 'application/octet-stream'
  );
}
