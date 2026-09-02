// SPDX-License-Identifier: MIT OR Apache-2.0

import { expect, test } from '@playwright/test';

test('production H5 reaches Axum and PostgreSQL through Framework Runtime after refresh', async ({
  page,
}) => {
  page.on('console', (message) => console.log(`browser console: ${message.type()}: ${message.text()}`));
  page.on('requestfailed', (request) =>
    console.log(`browser request failed: ${request.url()}: ${request.failure()?.errorText}`),
  );
  page.on('response', (response) => {
    if (response.url().endsWith('/health')) {
      console.log(`browser health response: ${response.status()} ${response.url()}`);
    }
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: __PRODUCT_NAME_JSON__ })).toBeVisible();
  const apiUrl = process.env.EXPO_PUBLIC_API_URL;
  expect(apiUrl).toBeTruthy();
  const directBrowserHealth = await page.evaluate(async (baseUrl) => {
    try {
      const response = await fetch(`${baseUrl}/health`);
      return { body: await response.text(), status: response.status };
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error) };
    }
  }, apiUrl);
  console.log(`browser direct health: ${JSON.stringify(directBrowserHealth)}`);
  console.log(`browser body: ${JSON.stringify(await page.locator('body').innerText())}`);
  await expect(page.getByText('Backend ready.')).toBeVisible();
  await expect(page.getByText('PostgreSQL schema: baseline')).toBeVisible();

  await page.reload();
  await expect(page.getByText('Backend ready.')).toBeVisible();
  await expect(page.getByText('PostgreSQL schema: baseline')).toBeVisible();
});
