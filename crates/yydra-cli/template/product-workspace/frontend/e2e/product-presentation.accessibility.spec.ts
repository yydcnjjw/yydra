// SPDX-License-Identifier: MIT OR Apache-2.0

import { expect, test } from "@playwright/test";

test("Reading Queue exposes registered Product Presentation semantics", async ({
  page,
}) => {
  const title = `Visible semantics ${Date.now()}`;
  const sourceUrl = `https://example.test/visible-semantics/${Date.now()}`;

  await page.setViewportSize({ width: 320, height: 480 });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: __PRODUCT_NAME_JSON__, exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Reading Queue", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "All entries", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("button", { name: "Oldest first", exact: true }),
  ).toHaveAttribute("aria-selected", "true");

  await page.getByLabel("Entry title", { exact: true }).fill(title);
  await page.getByLabel("Source URL", { exact: true }).fill(sourceUrl);
  await page.getByRole("button", { name: "Add entry", exact: true }).click();

  const entry = page.getByRole("listitem", {
    name: `Reading entry ${title}`,
    exact: true,
  });
  await expect(
    entry.getByRole("heading", { name: title, exact: true }),
  ).toBeVisible();
  await expect(
    entry.getByRole("link", { name: sourceUrl, exact: true }),
  ).toBeVisible();
  await expect(entry.getByText("State: queued", { exact: true })).toBeVisible();
  await entry
    .getByRole("button", { name: `Complete ${title}`, exact: true })
    .click();
  await expect(
    entry.getByRole("button", { name: `Reopen ${title}`, exact: true }),
  ).toBeVisible();
  await expect(
    entry.getByText("State: completed", { exact: true }),
  ).toBeVisible();
  await entry.scrollIntoViewIfNeeded();
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth <=
        document.documentElement.clientWidth,
    ),
  ).toBe(true);

  const completed = page.getByRole("button", {
    name: "Completed entries",
    exact: true,
  });
  await completed.click();
  await expect(completed).toBeFocused();
  await expect(completed).toHaveAttribute("aria-selected", "true");
  await expect(page).toHaveURL(/status=completed/);
});
