// SPDX-License-Identifier: MIT OR Apache-2.0

import { InfiniteQueryObserver } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

import {
  createFrameworkClient,
  createFrameworkQueryClient,
  isTransportFailure,
  readingQueueInfiniteQueryOptions,
} from "./runtime";

describe("Framework Runtime health client", () => {
  it("reaches the configured service and accepts the real health shape", async () => {
    const fetchImplementation = vi.fn(async () =>
      Response.json({ status: "ready", database: "baseline" }),
    );
    const client = createFrameworkClient(
      fetchImplementation,
      "http://service.test",
    );

    await expect(client.health()).resolves.toEqual({
      status: "ready",
      database: "baseline",
    });
    expect(fetchImplementation).toHaveBeenCalledWith(
      "http://service.test/health",
      { signal: undefined },
    );
  });

  it("classifies only transport failures as retryable", () => {
    expect(isTransportFailure({ kind: "transport", message: "offline" })).toBe(
      true,
    );
    expect(isTransportFailure({ kind: "transport" })).toBe(false);
    expect(
      isTransportFailure({ kind: "contractViolation", message: "invalid" }),
    ).toBe(false);
  });

  it("never retries mutations automatically", () => {
    expect(
      createFrameworkQueryClient().getDefaultOptions().mutations?.retry,
    ).toBe(false);
  });

  it("uses filter and sort query context, cursor pageParam, null termination, and one concurrent next-page request", async () => {
    let resolveNextPage:
      ((value: { entries: never[]; nextCursor: null }) => void) | undefined;
    const listReadingQueueEntries = vi
      .fn()
      .mockResolvedValueOnce({ entries: [], nextCursor: "v1.next.signed" })
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveNextPage = resolve;
          }),
      );
    const client = {
      listReadingQueueEntries,
    } as never;
    const queryClient = createFrameworkQueryClient();
    const options = readingQueueInfiniteQueryOptions(
      client,
      "completed",
      "newest",
      2,
    );
    expect(options.queryKey).toEqual([
      "reading-queue",
      { limit: 2, sort: "newest", status: "completed" },
    ]);
    await queryClient.fetchInfiniteQuery(options);
    expect(listReadingQueueEntries).toHaveBeenLastCalledWith(
      {
        cursor: undefined,
        limit: 2,
        sort: "newest",
        status: "completed",
      },
      expect.any(AbortSignal),
    );

    const observer = new InfiniteQueryObserver(queryClient, options);
    const first = observer.fetchNextPage({ cancelRefetch: false });
    const duplicate = observer.fetchNextPage({ cancelRefetch: false });
    await vi.waitFor(() =>
      expect(listReadingQueueEntries).toHaveBeenCalledTimes(2),
    );
    expect(listReadingQueueEntries).toHaveBeenLastCalledWith(
      {
        cursor: "v1.next.signed",
        limit: 2,
        sort: "newest",
        status: "completed",
      },
      expect.any(AbortSignal),
    );
    resolveNextPage?.({ entries: [], nextCursor: null });
    await Promise.all([first, duplicate]);
    expect(observer.getCurrentResult().hasNextPage).toBe(false);
    expect(listReadingQueueEntries).toHaveBeenCalledTimes(2);
  });

  it("forwards Reading Queue behavior through the Framework facade", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(
      async (_input, init) => {
        if (init?.method === "PATCH") {
          return Response.json({
            id: "opaque-entry",
            title: "Example",
            sourceUrl: "https://example.test",
            state: "completed",
          });
        }
        if (init?.method === "POST") {
          return Response.json(
            {
              id: "opaque-entry",
              title: "Example",
              sourceUrl: "https://example.test",
              state: "queued",
            },
            { status: 201 },
          );
        }
        return Response.json({ entries: [], nextCursor: null });
      },
    );
    const client = createFrameworkClient(
      fetchImplementation,
      "http://service.test",
    );

    await expect(client.listReadingQueueEntries()).resolves.toEqual({
      entries: [],
      nextCursor: null,
    });
    await expect(
      client.createReadingQueueEntry({
        title: "Example",
        sourceUrl: "https://example.test",
      }),
    ).resolves.toMatchObject({ id: "opaque-entry", state: "queued" });
    await expect(
      client.changeReadingQueueEntryState("opaque-entry", {
        state: "completed",
      }),
    ).resolves.toMatchObject({ id: "opaque-entry", state: "completed" });
  });

  it("injects credentials through the same production assembly seam", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(async () =>
      Response.json({ access: "granted" }),
    );
    const client = createFrameworkClient(
      fetchImplementation,
      "http://service.test",
      () => ({ Authorization: "Bearer contract-test" }),
    );

    await expect(client.frameworkProtectedContract()).resolves.toEqual({
      access: "granted",
    });
    expect(
      new Headers(fetchImplementation.mock.calls[0][1]?.headers).get(
        "authorization",
      ),
    ).toBe("Bearer contract-test");
  });
});
