import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  retries: 0,
  reporter: 'line',
  use: {
    baseURL: 'http://127.0.0.1:8081',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'npm run web -- --port 8081',
    env: {
      CI: '1',
      EXPO_PUBLIC_API_URL: 'http://127.0.0.1:4000',
    },
    reuseExistingServer: true,
    timeout: 120_000,
    url: 'http://127.0.0.1:8081',
  },
});
