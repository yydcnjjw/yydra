import { expect, test } from '@playwright/test';

test('clean Product Workspace survives responsive refresh and reaches its live backend', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await expect(page.getByText('Clean Yydra Product Workspace ready.')).toBeVisible();
  await page.reload();
  await expect(page.getByText('Clean Yydra Product Workspace ready.')).toBeVisible();

  const apiUrl = process.env.EXPO_PUBLIC_API_URL ?? 'http://127.0.0.1:4000';
  const response = await page.request.get(`${apiUrl}/health`);
  expect(response.status()).toBe(200);
  await expect(response.json()).resolves.toEqual({ status: 'ready' });
});
