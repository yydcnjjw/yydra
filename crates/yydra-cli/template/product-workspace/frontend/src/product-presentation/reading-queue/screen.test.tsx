// SPDX-License-Identifier: MIT OR Apache-2.0
// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  createTestFrameworkRuntime,
  FrameworkClient,
  FrameworkRuntime,
} from "@/framework/runtime";

import { ReadingQueueScreen } from "./screen";

interface Deferred<T> {
  promise: Promise<T>;
  reject(error: unknown): void;
  resolve(value: T): void;
}

function deferred<T>(): Deferred<T> {
  let rejectPromise: (error: unknown) => void = () => undefined;
  let resolvePromise: (value: T) => void = () => undefined;
  const promise = new Promise<T>((resolve, reject) => {
    rejectPromise = reject;
    resolvePromise = resolve;
  });
  return { promise, reject: rejectPromise, resolve: resolvePromise };
}

function fakeClient(overrides: Partial<FrameworkClient> = {}): FrameworkClient {
  return {
    changeReadingQueueEntryState: vi.fn(),
    createReadingQueueEntry: vi.fn(),
    frameworkContractProfile: vi.fn(),
    frameworkProtectedContract: vi.fn(),
    health: vi.fn(async () => ({ status: "ready", database: "baseline" })),
    listReadingQueueEntries: vi.fn(async () => ({
      entries: [],
      nextCursor: null,
    })),
    ...overrides,
  } as FrameworkClient;
}

function renderScreen(
  client: FrameworkClient,
  options: {
    onRouteStateChange?: (
      status: "all" | "queued" | "completed",
      sort: "oldest" | "newest",
    ) => void;
    sort?: "oldest" | "newest";
    status?: "all" | "queued" | "completed";
  } = {},
) {
  const runtime = createTestFrameworkRuntime(client);
  return render(
    <FrameworkRuntime runtime={runtime}>
      <ReadingQueueScreen
        onRouteStateChange={options.onRouteStateChange ?? vi.fn()}
        productName="Runtime Probe"
        sort={options.sort ?? "oldest"}
        status={options.status ?? "all"}
      />
    </FrameworkRuntime>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Reading Queue Product Presentation", () => {
  it("distinguishes initial loading from a true empty success", async () => {
    const health = deferred<{ status: string; database: string }>();
    const queue = deferred<{ entries: never[]; nextCursor: null }>();
    renderScreen(
      fakeClient({
        health: vi.fn(() => health.promise),
        listReadingQueueEntries: vi.fn(() => queue.promise),
      }),
    );

    expect(screen.getByText("Connecting to Product service…")).toBeTruthy();
    expect(screen.getByText("Loading queue…")).toBeTruthy();
    expect(screen.queryByText("The queue is empty.")).toBeNull();

    health.resolve({ status: "ready", database: "baseline" });
    queue.resolve({ entries: [], nextCursor: null });

    expect(await screen.findByText("Backend ready.")).toBeTruthy();
    expect(await screen.findByText("The queue is empty.")).toBeTruthy();
  });

  it("keeps stale data visible and reports a background transport failure", async () => {
    const failedRefresh = deferred<{ entries: never[]; nextCursor: null }>();
    const list = vi
      .fn()
      .mockResolvedValueOnce({
        entries: [
          {
            id: "opaque-entry",
            sourceUrl: "https://example.test/kept-visible",
            state: "queued",
            title: "Kept visible",
          },
        ],
        nextCursor: null,
      })
      .mockImplementationOnce(() => failedRefresh.promise);
    renderScreen(fakeClient({ listReadingQueueEntries: list }));
    expect(
      await screen.findByRole("heading", { name: "Kept visible" }),
    ).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: "Refresh from first page" }),
    );
    expect(await screen.findByText("Refreshing queue…")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Kept visible" })).toBeTruthy();

    failedRefresh.reject({ kind: "transport", message: "offline" });
    expect(
      (
        await screen.findByText(
          "Queue refresh failed. Saved results remain visible. Cannot reach the Product service.",
        )
      ).getAttribute("role"),
    ).toBe("alert");
    expect(screen.getByRole("heading", { name: "Kept visible" })).toBeTruthy();
  });

  it.each([
    [
      {
        kind: "problem",
        problem: {
          status: 503,
          title: "Unavailable",
          type: "https://yydra.dev/problems/unavailable",
        },
      },
      "The Product service could not load this queue.",
      true,
    ],
    [
      { kind: "transport", message: "offline" },
      "Cannot reach the Product service.",
      true,
    ],
    [
      { kind: "contractViolation", message: "raw invalid response" },
      "The Product service returned data this app cannot safely display.",
      true,
    ],
    [
      { kind: "cancelled", message: "cancelled" },
      "Queue loading was cancelled.",
      false,
    ],
  ] as const)(
    "renders a safe blocking outcome for %s",
    async (failure, message, isAlert) => {
      renderScreen(
        fakeClient({
          listReadingQueueEntries: vi.fn(async () => {
            throw failure;
          }),
        }),
      );

      expect(await screen.findByText(message)).toBeTruthy();
      expect(screen.queryByRole("alert") !== null).toBe(isAlert);
      expect(screen.queryByText("The queue is empty.")).toBeNull();
    },
  );

  it("maps a typed create Problem to Product-owned recovery copy", async () => {
    renderScreen(
      fakeClient({
        createReadingQueueEntry: vi.fn(async () => {
          throw {
            kind: "problem",
            problem: {
              status: 422,
              title: "Invalid Reading Entry",
              type: "https://yydra.dev/problems/invalid-reading-entry",
            },
          };
        }),
      }),
    );
    await screen.findByText("The queue is empty.");

    fireEvent.change(screen.getByLabelText("Entry title"), {
      target: { value: "   " },
    });
    fireEvent.change(screen.getByLabelText("Source URL"), {
      target: { value: "https://example.test/rejected" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add entry" }));

    expect(
      (
        await screen.findByText("Enter a title and a valid source URL.")
      ).getAttribute("role"),
    ).toBe("alert");
  });

  it("keeps URL state outside the screen and exposes selected filter state", async () => {
    const onRouteStateChange = vi.fn();
    renderScreen(fakeClient(), {
      onRouteStateChange,
      sort: "newest",
      status: "queued",
    });
    await screen.findByText("No entries match this filter.");

    expect(
      screen
        .getByRole("button", { name: "Queued entries" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(
      screen
        .getByRole("button", { name: "Newest first" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Completed entries" }));

    await waitFor(() =>
      expect(onRouteStateChange).toHaveBeenCalledWith("completed", "newest"),
    );
  });
});
