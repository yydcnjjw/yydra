const fs = require('fs');
const { chromium } = require('playwright');

const apiBase = process.env.API_BASE_URL;
const webBase = process.env.WEB_BASE_URL;
const evidenceDir = process.env.EVIDENCE_DIR;

if (!apiBase || !webBase || !evidenceDir) {
  throw new Error('API_BASE_URL, WEB_BASE_URL, and EVIDENCE_DIR are required');
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const title = `Accessible hidden ${Date.now()}`;
  const sourceUrl = `https://example.com/h5/${Date.now()}`;
  try {
    await page.goto(webBase, { waitUntil: 'networkidle' });
    await page.getByRole('heading', { name: 'Reading Queue', exact: true }).waitFor();
    await page.getByLabel('Title', { exact: true }).fill(title);
    await page.getByLabel('Source URL', { exact: true }).fill(sourceUrl);
    await page.getByRole('button', { name: 'Add to queue', exact: true }).click();
    await page.getByRole('heading', { name: title, exact: true }).waitFor();
    await page.getByRole('link', { name: sourceUrl, exact: true }).waitFor();
    await page.getByRole('button', { name: `Complete ${title}`, exact: true }).click();
    await page.getByRole('button', { name: `Reopen ${title}`, exact: true }).waitFor();
    await page.getByRole('button', { name: 'Completed', exact: true }).click();
    await page.getByRole('heading', { name: title, exact: true }).waitFor();
    await page.getByRole('button', { name: `Reopen ${title}`, exact: true }).click();
    await page.getByRole('button', { name: 'Queued', exact: true }).click();
    await page.getByRole('heading', { name: title, exact: true }).waitFor();
    await page.screenshot({ path: `${evidenceDir}/hidden-h5.png`, fullPage: true });
    process.stdout.write(JSON.stringify({ schemaVersion: 1, status: 'pass', checks: 10 }) + '\n');
  } catch (error) {
    await page.screenshot({ path: `${evidenceDir}/hidden-h5-failure.png`, fullPage: true });
    throw error;
  } finally {
    await browser.close();
  }
})().catch((error) => {
  process.stderr.write(`${error.stack || error}\n`);
  process.exit(1);
});
