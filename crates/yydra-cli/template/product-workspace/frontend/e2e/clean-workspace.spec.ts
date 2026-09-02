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
  const entryId = persistedQueue.body.entries[0].id as string;

  const directComplete = await page.evaluate(
    async ({ baseUrl, id }) => {
      const response = await fetch(
        `${baseUrl}/api/v1/reading-queue/entries/${encodeURIComponent(id)}`,
        {
          method: "PATCH",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ state: "completed" }),
        },
      );
      return { body: await response.json(), status: response.status };
    },
    { baseUrl: apiUrl, id: entryId },
  );
  expect(directComplete).toMatchObject({
    body: { id: entryId, state: "completed" },
    status: 200,
  });

  await page.getByRole("button", { name: `Complete ${entryTitle}` }).click();
  await expect(page.getByRole("alert")).toContainText(
    "This entry changed. Refresh the queue and try again.",
  );
  await page.getByRole("button", { name: "Refresh queue" }).click();
  await expect(
    page.getByText("State: completed", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: `Reopen ${entryTitle}` }).click();
  await expect(page.getByText("State: queued", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: `Complete ${entryTitle}` }).click();
  await expect(
    page.getByText("State: completed", { exact: true }),
  ).toBeVisible();

  const requestProblems = await page.evaluate(
    async ({ baseUrl, id }) => {
      const transitionUrl = `${baseUrl}/api/v1/reading-queue/entries/${encodeURIComponent(id)}`;
      const unknown = await fetch(transitionUrl, {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ state: "queued", unknown: true }),
      });
      const malformed = await fetch(transitionUrl, {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: "{",
      });
      const missing = await fetch(
        `${baseUrl}/api/v1/reading-queue/entries/missing-opaque-entry`,
        {
          method: "PATCH",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ state: "completed" }),
        },
      );
      const validation = await fetch(
        `${baseUrl}/api/v1/reading-queue/entries`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            title: "   ",
            sourceUrl: "https://example.test/rejected-direct",
          }),
        },
      );
      return {
        malformed: { body: await malformed.json(), status: malformed.status },
        missing: { body: await missing.json(), status: missing.status },
        unknown: { body: await unknown.json(), status: unknown.status },
        validation: {
          body: await validation.json(),
          status: validation.status,
        },
      };
    },
    { baseUrl: apiUrl, id: entryId },
  );
  expect(requestProblems.unknown).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-request-body" },
    status: 400,
  });
  expect(requestProblems.malformed).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-request-body" },
    status: 400,
  });
  expect(requestProblems.missing).toMatchObject({
    body: { type: "https://yydra.dev/problems/reading-entry-not-found" },
    status: 404,
  });
  expect(requestProblems.validation).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-reading-entry" },
    status: 422,
  });

  const authentication = await page.evaluate(async (baseUrl) => {
    const missing = await fetch(`${baseUrl}/api/v1/framework-auth-contract`);
    const forbidden = await fetch(`${baseUrl}/api/v1/framework-auth-contract`, {
      headers: { authorization: "Bearer local-framework-forbidden" },
    });
    const authorized = await fetch(
      `${baseUrl}/api/v1/framework-auth-contract`,
      { headers: { authorization: "Bearer local-framework-contract" } },
    );
    return {
      authorized: {
        body: await authorized.json(),
        status: authorized.status,
      },
      forbidden: {
        body: await forbidden.json(),
        status: forbidden.status,
      },
      missing: {
        body: await missing.json(),
        challenge: missing.headers.get("www-authenticate"),
        status: missing.status,
      },
    };
  }, apiUrl);
  expect(authentication.missing).toMatchObject({
    body: { type: "https://yydra.dev/problems/authentication-required" },
    challenge: expect.stringContaining("Bearer"),
    status: 401,
  });
  expect(authentication.forbidden).toMatchObject({
    body: { type: "https://yydra.dev/problems/access-forbidden" },
    status: 403,
  });
  expect(authentication.authorized).toEqual({
    body: { access: "granted" },
    status: 200,
  });

  const pagination = await page.evaluate(async (baseUrl) => {
    for (let index = 1; index <= 11; index += 1) {
      const response = await fetch(`${baseUrl}/api/v1/reading-queue/entries`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          title: `Paging entry ${String(index).padStart(2, "0")}`,
          sourceUrl: `https://example.test/paging-${index}`,
        }),
      });
      if (response.status !== 201) {
        throw new Error(
          `pagination fixture create returned ${response.status}`,
        );
      }
    }

    const requestPage = async (
      status: string,
      sort: string,
      limit: number,
      cursor?: string,
    ) => {
      const query = new URLSearchParams({ status, sort, limit: String(limit) });
      if (cursor) query.set("cursor", cursor);
      const response = await fetch(
        `${baseUrl}/api/v1/reading-queue/entries?${query}`,
      );
      return { body: await response.json(), status: response.status };
    };
    const first = await requestPage("queued", "oldest", 3);
    const firstCursor = first.body.nextCursor as string;
    const second = await requestPage("queued", "oldest", 3, firstCursor);
    const tamperedBytes = firstCursor.split("");
    const payloadIndex = firstCursor.indexOf(".") + 2;
    tamperedBytes[payloadIndex] =
      tamperedBytes[payloadIndex] === "A" ? "B" : "A";
    const tampered = await requestPage(
      "queued",
      "oldest",
      3,
      tamperedBytes.join(""),
    );
    const mismatch = await requestPage("completed", "oldest", 3, firstCursor);
    const unknown = await fetch(
      `${baseUrl}/api/v1/reading-queue/entries?unknown=true`,
    );

    const traversedIds: string[] = [];
    let cursor: string | undefined;
    let pages = 0;
    do {
      const page = await requestPage("queued", "oldest", 3, cursor);
      if (page.status !== 200) {
        throw new Error(`pagination traversal returned ${page.status}`);
      }
      traversedIds.push(
        ...page.body.entries.map((entry: { id: string }) => entry.id),
      );
      cursor = page.body.nextCursor ?? undefined;
      pages += 1;
      if (pages > 10) throw new Error("pagination did not terminate");
    } while (cursor);

    return {
      first,
      second,
      tampered,
      mismatch,
      unknown: { body: await unknown.json(), status: unknown.status },
      traversedIds,
      pages,
    };
  }, apiUrl);
  expect(pagination.first).toMatchObject({
    body: { entries: expect.any(Array), nextCursor: expect.any(String) },
    status: 200,
  });
  expect(pagination.first.body.entries).toHaveLength(3);
  expect(pagination.second.status).toBe(200);
  expect(pagination.traversedIds).toHaveLength(11);
  expect(new Set(pagination.traversedIds).size).toBe(11);
  expect(pagination.pages).toBe(4);
  expect(pagination.tampered).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-reading-queue-cursor" },
    status: 400,
  });
  expect(pagination.mismatch).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-reading-queue-cursor" },
    status: 400,
  });
  expect(pagination.unknown).toMatchObject({
    body: { type: "https://yydra.dev/problems/invalid-reading-queue-query" },
    status: 400,
  });

  await page.getByRole("button", { name: "Queued entries" }).click();
  await page.getByRole("button", { name: "Newest first" }).click();
  await expect(page).toHaveURL(/status=queued/);
  await expect(page).toHaveURL(/sort=newest/);
  await expect(
    page.getByText("Paging entry 11", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Paging entry 01", { exact: true }),
  ).not.toBeVisible();
  await page.getByRole("button", { name: "Load more" }).click();
  await expect(
    page.getByText("Paging entry 01", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("End of queue.", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Refresh from first page" }).click();
  await expect(
    page.getByText("Paging entry 01", { exact: true }),
  ).not.toBeVisible();
  await expect(page.getByRole("button", { name: "Load more" })).toBeVisible();

  await page.reload();
  await expect(page).toHaveURL(/status=queued/);
  await expect(page).toHaveURL(/sort=newest/);
  await expect(
    page.getByText("Paging entry 11", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText(entryTitle, { exact: true })).not.toBeVisible();
  await page.goto("/?status=completed&sort=oldest");
  await expect(page.getByText(entryTitle, { exact: true })).toBeVisible();
  await expect(
    page.getByText("Paging entry 11", { exact: true }),
  ).not.toBeVisible();

  await page.getByLabel("Entry title").fill("   ");
  await page.getByLabel("Source URL").fill("https://example.test/rejected");
  await page.getByRole("button", { name: "Add entry" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Could not add this entry.",
  );

  await page.reload();
  await expect(page).toHaveURL(/status=completed/);
  await expect(page.getByText("Backend ready.")).toBeVisible();
  await expect(page.getByText("PostgreSQL schema: baseline")).toBeVisible();
  await expect(page.getByText(entryTitle, { exact: true })).toBeVisible();
  await expect(page.getByText(sourceUrl, { exact: true })).toBeVisible();
  await expect(
    page.getByText("State: completed", { exact: true }),
  ).toBeVisible();
});
