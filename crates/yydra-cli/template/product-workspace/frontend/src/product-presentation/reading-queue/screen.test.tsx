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
import { useState } from "react";

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

const savedEntry = {
  id: "saved-entry",
  title: "Submitted title",
  sourceUrl: "https://example.test/submitted",
  state: "queued" as const,
};
function fillDraft() {
  fireEvent.change(screen.getByLabelText("Entry title"), {
    target: { value: savedEntry.title },
  });
  fireEvent.change(screen.getByLabelText("Source URL"), {
    target: { value: savedEntry.sourceUrl },
  });
}

describe("Reading Queue write and refresh lifecycle", () => {
  it("clears an unchanged draft once the write succeeds", async () => {
    renderScreen(
      fakeClient({ createReadingQueueEntry: vi.fn(async () => savedEntry) }),
    );
    await screen.findByText("The queue is empty.");
    fillDraft();
    fireEvent.click(screen.getByRole("button", { name: "Add entry" }));
    await waitFor(() =>
      expect(
        (screen.getByLabelText("Entry title") as HTMLInputElement).value,
      ).toBe(""),
    );
    expect(
      (screen.getByLabelText("Source URL") as HTMLInputElement).value,
    ).toBe("");
  });

  it.each(["Entry title", "Source URL"])(
    "retains the entire draft when %s changes during submission",
    async (field) => {
      const write = deferred<typeof savedEntry>();
      const create = vi.fn(() => write.promise);
      renderScreen(fakeClient({ createReadingQueueEntry: create }));
      await screen.findByText("The queue is empty.");
      fillDraft();
      fireEvent.click(screen.getByRole("button", { name: "Add entry" }));
      const nextValue =
        field === "Entry title" ? "Next draft" : "https://example.test/next";
      fireEvent.change(screen.getByLabelText(field), {
        target: { value: nextValue },
      });
      write.resolve(savedEntry);
      await waitFor(() =>
        expect(
          screen
            .getByRole("button", { name: "Add entry" })
            .getAttribute("aria-disabled"),
        ).not.toBe("true"),
      );
      expect(create).toHaveBeenCalledExactlyOnceWith({
        title: savedEntry.title,
        sourceUrl: savedEntry.sourceUrl,
      });
      expect(
        (screen.getByLabelText("Entry title") as HTMLInputElement).value,
      ).toBe(field === "Entry title" ? nextValue : savedEntry.title);
      expect(
        (screen.getByLabelText("Source URL") as HTMLInputElement).value,
      ).toBe(field === "Source URL" ? nextValue : savedEntry.sourceUrl);
    },
  );

  it.each(["create", "transition"])(
    "keeps saved data and retries only the read after a successful %s",
    async (operation) => {
      const refresh = deferred<{
        entries: (typeof savedEntry)[];
        nextCursor: null;
      }>();
      const list = vi
        .fn()
        .mockResolvedValueOnce({ entries: [savedEntry], nextCursor: null })
        .mockImplementationOnce(() => refresh.promise)
        .mockResolvedValue({
          entries: [{ ...savedEntry, title: "Fresh result" }],
          nextCursor: null,
        });
      const create = vi.fn(async () => savedEntry);
      const change = vi.fn(async () => ({
        ...savedEntry,
        state: "completed" as const,
      }));
      renderScreen(
        fakeClient({
          listReadingQueueEntries: list,
          createReadingQueueEntry: create,
          changeReadingQueueEntryState: change,
        }),
      );
      await screen.findByRole("heading", { name: savedEntry.title });
      if (operation === "create") {
        fillDraft();
        fireEvent.click(screen.getByRole("button", { name: "Add entry" }));
      } else {
        fireEvent.click(
          screen.getByRole("button", { name: `Complete ${savedEntry.title}` }),
        );
      }
      await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
      expect(
        screen.getByRole("heading", { name: savedEntry.title }),
      ).toBeTruthy();
      refresh.reject({ kind: "transport", message: "offline" });
      expect(
        await screen.findByText(/Entry saved. Queue refresh failed/),
      ).toBeTruthy();
      expect(
        screen.getByRole("heading", { name: savedEntry.title }),
      ).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Retry queue" }));
      await screen.findByRole("heading", { name: "Fresh result" });
      expect(list).toHaveBeenCalledTimes(3);
      expect(create).toHaveBeenCalledTimes(operation === "create" ? 1 : 0);
      expect(change).toHaveBeenCalledTimes(operation === "transition" ? 1 : 0);
      expect(
        screen.queryByText(/Entry saved. Queue refresh failed/),
      ).toBeNull();
    },
  );

  it("cancels a next page before refresh and ignores its late result", async () => {
    const nextPage = deferred<{
      entries: (typeof savedEntry)[];
      nextCursor: null;
    }>();
    const firstPage = deferred<{
      entries: (typeof savedEntry)[];
      nextCursor: string;
    }>();
    const list = vi
      .fn()
      .mockResolvedValueOnce({ entries: [savedEntry], nextCursor: "cursor-1" })
      .mockImplementationOnce(() => nextPage.promise)
      .mockImplementationOnce(() => firstPage.promise);
    renderScreen(fakeClient({ listReadingQueueEntries: list }));
    await screen.findByRole("heading", { name: savedEntry.title });
    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
    const nextSignal = list.mock.calls[1][1] as AbortSignal;
    fireEvent.click(
      screen.getByRole("button", { name: "Refresh from first page" }),
    );
    await waitFor(() => expect(list).toHaveBeenCalledTimes(3));
    expect(nextSignal.aborted).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    expect(list).toHaveBeenCalledTimes(3);
    nextPage.resolve({
      entries: [{ ...savedEntry, id: "late", title: "Late page" }],
      nextCursor: null,
    });
    firstPage.resolve({
      entries: [{ ...savedEntry, title: "Fresh first page" }],
      nextCursor: "fresh-cursor",
    });
    await screen.findByRole("heading", { name: "Fresh first page" });
    expect(screen.queryByRole("heading", { name: "Late page" })).toBeNull();
    expect(list.mock.calls[2][0]).toMatchObject({ cursor: undefined });
    expect(
      screen
        .getByRole("button", { name: "Load more" })
        .getAttribute("aria-disabled"),
    ).not.toBe("true");
  });
});

it("retains the write acknowledgement when changing filters cancels its refresh", async () => {
  const oldRefresh = deferred<{
    entries: (typeof savedEntry)[];
    nextCursor: null;
  }>();
  const newRefresh = deferred<{
    entries: (typeof savedEntry)[];
    nextCursor: null;
  }>();
  const list = vi
    .fn()
    .mockResolvedValueOnce({ entries: [savedEntry], nextCursor: null })
    .mockImplementationOnce(() => oldRefresh.promise)
    .mockImplementationOnce(() => newRefresh.promise)
    .mockResolvedValue({
      entries: [{ ...savedEntry, title: "Fresh filtered entry" }],
      nextCursor: null,
    });
  const create = vi.fn(async () => savedEntry);
  const runtime = createTestFrameworkRuntime(
    fakeClient({
      listReadingQueueEntries: list,
      createReadingQueueEntry: create,
    }),
  );
  function RoutedScreen() {
    const [status, setStatus] = useState<"all" | "queued" | "completed">("all");
    return (
      <FrameworkRuntime runtime={runtime}>
        <ReadingQueueScreen
          productName="Routing probe"
          sort="oldest"
          status={status}
          onRouteStateChange={setStatus}
        />
      </FrameworkRuntime>
    );
  }
  render(<RoutedScreen />);
  await screen.findByRole("heading", { name: savedEntry.title });
  fillDraft();
  fireEvent.click(screen.getByRole("button", { name: "Add entry" }));
  await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  const oldSignal = list.mock.calls[1][1] as AbortSignal;
  fireEvent.click(screen.getByRole("button", { name: "Queued entries" }));
  await waitFor(() => expect(list).toHaveBeenCalledTimes(3));
  expect(oldSignal.aborted).toBe(true);
  oldRefresh.resolve({
    entries: [{ ...savedEntry, title: "Cancelled result" }],
    nextCursor: null,
  });
  newRefresh.reject({ kind: "transport", message: "offline" });
  expect(
    await screen.findByText(/Entry saved. Queue refresh failed/),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Retry queue" }));
  await screen.findByRole("heading", { name: "Fresh filtered entry" });
  expect(screen.queryByText(/Entry saved. Queue refresh failed/)).toBeNull();
  expect(create).toHaveBeenCalledOnce();
  expect(list).toHaveBeenCalledTimes(4);
});
