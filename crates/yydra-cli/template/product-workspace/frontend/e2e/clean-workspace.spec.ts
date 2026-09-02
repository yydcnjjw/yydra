// SPDX-License-Identifier: MIT OR Apache-2.0

import { expect, test } from "@playwright/test";

test("production H5 reaches Axum and PostgreSQL through Framework Runtime after refresh", async ({
  page,
}) => {
  page.on("console", (message) =>
    console.log(`browser console: ${message.type()}: ${message.text()}`),
  );
  page.on("requestfailed", (request) =>
    console.log(
      `browser request failed: ${request.url()}: ${request.failure()?.errorText}`,
    ),
  );
  page.on("response", (response) => {
    if (response.url().endsWith("/health")) {
      console.log(
        `browser health response: ${response.status()} ${response.url()}`,
      );
    }
  });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: __PRODUCT_NAME_JSON__ }),
  ).toBeVisible();
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
  console.log(
    `browser body: ${JSON.stringify(await page.locator("body").innerText())}`,
  );
  await expect(page.getByText("Backend ready.")).toBeVisible();
  await expect(page.getByText("PostgreSQL schema: baseline")).toBeVisible();
  await expect(page.getByText("The queue is empty.")).toBeVisible();

  const entryTitle = "Transactions without hidden magic";
  const sourceUrl = "https://example.test/transactions";
  await page.getByLabel("Entry title").fill(entryTitle);
  await page.getByLabel("Source URL").fill(sourceUrl);
  await page.getByRole("button", { name: "Add entry" }).click();
  await expect(page.getByText(entryTitle, { exact: true })).toBeVisible();
  await expect(page.getByText(sourceUrl, { exact: true })).toBeVisible();
  await expect(page.getByText("State: queued", { exact: true })).toBeVisible();

  const persistedQueue = await page.evaluate(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/api/v1/reading-queue/entries`);
    return { body: await response.json(), status: response.status };
  }, apiUrl);
  expect(persistedQueue.status).toBe(200);
  expect(persistedQueue.body).toMatchObject({
    entries: [
      {
        title: entryTitle,
        sourceUrl,
        state: "queued",
      },
    ],
  });
  expect(persistedQueue.body.entries[0].id).toEqual(expect.any(String));

  await page.getByLabel("Entry title").fill("   ");
  await page.getByLabel("Source URL").fill("https://example.test/rejected");
  await page.getByRole("button", { name: "Add entry" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Could not add this entry.",
  );

  await page.reload();
  await expect(page.getByText("Backend ready.")).toBeVisible();
  await expect(page.getByText("PostgreSQL schema: baseline")).toBeVisible();
  await expect(page.getByText(entryTitle, { exact: true })).toBeVisible();
  await expect(page.getByText(sourceUrl, { exact: true })).toBeVisible();
});
