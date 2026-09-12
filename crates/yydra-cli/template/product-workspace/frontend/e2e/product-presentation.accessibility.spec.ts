// SPDX-License-Identifier: MIT OR Apache-2.0

import { expect, test } from "@playwright/test";
import { signIn } from "./auth-fixture";

test("Reading Queue exposes registered Product Presentation semantics", async ({
  page,
}) => {
  const productName: string = __PRODUCT_NAME_JSON__;
  const title = `Visible semantics ${Date.now()}`;
  const sourceUrl = `https://example.test/visible-semantics/${Date.now()}`;

  await page.setViewportSize({ width: 320, height: 480 });
  await signIn(page);
  const productHeadings = page.getByRole("heading", {
    name: productName,
    exact: true,
  });
  await expect(productHeadings).toHaveCount(
    ["Add to Reading Queue", "Reading Queue"].includes(productName) ? 2 : 1,
  );
  await expect(productHeadings.first()).toBeVisible();
  const queueHeadings = page.getByRole("heading", {
    name: "Reading Queue",
    exact: true,
  });
  await expect(queueHeadings).toHaveCount(
    productName === "Reading Queue" ? 2 : 1,
  );
  await expect(
    productName === "Reading Queue" ? queueHeadings.nth(1) : queueHeadings,
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
