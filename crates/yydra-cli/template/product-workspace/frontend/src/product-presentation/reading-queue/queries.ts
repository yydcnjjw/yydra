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
  const queryFn = ({
    pageParam,
    signal,
  }: {
    pageParam: string | undefined;
    signal: AbortSignal;
  }) =>
    client.listReadingQueueEntries(
      { status, sort, limit, cursor: pageParam },
      signal,
    );
  return {
    ...infiniteQueryOptions({
      queryKey: readingQueueQueryKey(status, sort, limit),
      queryFn,
      initialPageParam: undefined as string | undefined,
      getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
    }),
    queryFn,
  };
}
