// SPDX-License-Identifier: MIT OR Apache-2.0

import { InfiniteQueryObserver } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

import { createFrameworkQueryClient } from "@/framework/runtime";

import { readingQueueInfiniteQueryOptions } from "./queries";

describe("Reading Queue query ownership", () => {
  it("uses URL context, cursor pageParam, null termination, and one concurrent next-page request", async () => {
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
});
