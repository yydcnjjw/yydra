// SPDX-License-Identifier: MIT OR Apache-2.0

process.env.YYDRA_PLAYWRIGHT_SPEC ??=
  "e2e/product-presentation.accessibility.spec.ts";

await import("./run-h5-e2e.mjs");
