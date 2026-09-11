// SPDX-License-Identifier: MIT OR Apache-2.0
import { expect, type Page } from "@playwright/test";

declare global {
  function authFixtureFetch(
    input: RequestInfo | URL,
    init?: RequestInit,
  ): Promise<Response>;
}
export async function installAuthFixtureFetch(page: Page) {
  await page.addInitScript(
    ({ baseUrl }) => {
      // Only direct test probes use this helper. Product fetch is never replaced.
      Object.defineProperty(window, "authFixtureFetch", {
        value: async (input: RequestInfo | URL, init?: RequestInit) => {
          const headers = new Headers(init?.headers);
          if (init?.method && !["GET", "HEAD"].includes(init.method)) {
            const session = await fetch(`${baseUrl}/api/v1/auth/session`, {
              credentials: "include",
            });
            const body = await session.json();
            if (body.csrfToken) headers.set("x-yydra-csrf", body.csrfToken);
          }
          return fetch(input, { ...init, headers, credentials: "include" });
        },
      });
    },
    { baseUrl: process.env.EXPO_PUBLIC_API_URL },
  );
}
export async function signIn(page: Page, account = "Account A") {
  await page.goto("/");
  await page.getByRole("button", { name: "Continue with GitHub" }).click();
  await page.getByRole("link", { name: account, exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Sign out", exact: true }),
  ).toBeVisible();
}

export async function verifyAccountIsolation(page: Page, privateTitle: string) {
  const signOut = () =>
    page.getByRole("button", { name: "Sign out", exact: true }).click();
  await signOut();
  await expect(page.getByText(privateTitle, { exact: true })).not.toBeVisible();
  await page.getByRole("button", { name: "Continue with GitHub" }).click();
  await page.getByRole("link", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Sign-in was not completed",
  );
  await signIn(page, "Account B");
  await expect(page.getByText("The queue is empty.")).toBeVisible();
  await expect(page.getByText(privateTitle, { exact: true })).not.toBeVisible();
  await page.reload();
  await expect(page.getByText("The queue is empty.")).toBeVisible();
  await signOut();
  const revoked = await page.evaluate(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/api/v1/reading-queue/entries`, {
      credentials: "include",
    });
    return response.status;
  }, process.env.EXPO_PUBLIC_API_URL);
  expect(revoked).toBe(401);
  await signIn(page, "Account A");
  await page.goto("/?status=completed&sort=oldest");
  await expect(page.getByText(privateTitle, { exact: true })).toBeVisible();
}
