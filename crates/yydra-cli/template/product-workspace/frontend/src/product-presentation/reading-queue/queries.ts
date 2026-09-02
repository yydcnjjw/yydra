// SPDX-License-Identifier: MIT OR Apache-2.0

import { infiniteQueryOptions } from "@tanstack/react-query";

import { FrameworkClient } from "@/framework/runtime";

export type ReadingQueueStatusFilter = "all" | "queued" | "completed";
export type ReadingQueueSort = "oldest" | "newest";

export const readingQueueRootKey = ["reading-queue"] as const;

export function readingQueueQueryKey(
  status: ReadingQueueStatusFilter,
  sort: ReadingQueueSort,
  limit: number,
) {
  return ["reading-queue", { status, sort, limit }] as const;
}

export function readingQueueInfiniteQueryOptions(
  client: FrameworkClient,
  status: ReadingQueueStatusFilter,
  sort: ReadingQueueSort,
  limit: number,
) {
  return infiniteQueryOptions({
    queryKey: readingQueueQueryKey(status, sort, limit),
    queryFn: ({ pageParam, signal }) =>
      client.listReadingQueueEntries(
        { status, sort, limit, cursor: pageParam },
        signal,
      ),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });
}
