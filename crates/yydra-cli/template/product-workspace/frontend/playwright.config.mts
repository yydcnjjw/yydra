// SPDX-License-Identifier: MIT OR Apache-2.0

import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  outputDir: process.env.YYDRA_PLAYWRIGHT_OUTPUT ?? 'test-results',
  fullyParallel: false,
  retries: 0,
  reporter: 'line',
  use: {
    baseURL: `http://127.0.0.1:${process.env.YYDRA_H5_PORT ?? '8081'}`,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
});
