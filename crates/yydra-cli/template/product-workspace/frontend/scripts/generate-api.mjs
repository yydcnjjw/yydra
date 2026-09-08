// SPDX-License-Identifier: MIT OR Apache-2.0

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const result = spawnSync(
  process.env.YYDRA_EXECUTABLE || "yydra",
  ["generate", "api", root],
  {
    cwd: root,
    stdio: "inherit",
  },
);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
