import { expect, test } from '@playwright/test';

test('create, filter, complete, and reopen a reading entry', async ({ page }) => {
  const title = `Golden Stack ${Date.now()}`;
  const sourceUrl = `https://example.com/golden-stack/${Date.now()}`;

  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Reading Queue' })).toBeVisible();

  await page.getByLabel('Title').fill(title);
  await page.getByLabel('Source URL').fill(sourceUrl);
  await page.getByRole('button', { name: 'Add to queue' }).click();

  const entry = page.getByLabel(`Reading entry ${title}`);
  await expect(entry).toBeVisible();
  await expect(entry.getByRole('link', { name: sourceUrl })).toBeVisible();

  await entry.getByRole('button', { name: `Complete ${title}` }).click();
  await page.getByRole('button', { name: 'Completed', exact: true }).click();
  await expect(entry).toContainText('Completed');
  await page.screenshot({
    fullPage: true,
    path: '../evidence/h5-reading-queue-completed.png',
  });

  await entry.getByRole('button', { name: `Reopen ${title}` }).click();
  await page.getByRole('button', { name: 'Queued', exact: true }).click();
  await expect(entry).toContainText('Queued');
});
