import { expect, test } from '@playwright/test';

test('Product Workspace shell exposes its title as a heading', async ({ page }) => {
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: '__PRODUCT_NAME__', exact: true }),
  ).toBeVisible();
});
