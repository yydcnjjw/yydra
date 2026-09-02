import { expect, test } from '@playwright/test';

test('Reading Queue exposes product semantics through the accessibility tree', async ({
  page,
}) => {
  const title = `Visible semantics ${Date.now()}`;
  const sourceUrl = `https://example.com/visible-semantics/${Date.now()}`;

  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Reading Queue', exact: true }),
  ).toBeVisible();
  await page.getByLabel('Title', { exact: true }).fill(title);
  await page.getByLabel('Source URL', { exact: true }).fill(sourceUrl);
  await page.getByRole('button', { name: 'Add to queue', exact: true }).click();

  const entry = page.getByLabel(`Reading entry ${title}`, { exact: true });
  await expect(
    entry.getByRole('heading', { name: title, exact: true }),
  ).toBeVisible();
  await expect(entry.getByRole('link', { name: sourceUrl, exact: true })).toBeVisible();
  await entry.getByRole('button', { name: `Complete ${title}`, exact: true }).click();
  await page.getByRole('button', { name: 'Completed', exact: true }).click();
  await expect(
    entry.getByRole('button', { name: `Reopen ${title}`, exact: true }),
  ).toBeVisible();
});
