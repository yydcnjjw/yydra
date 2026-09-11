// SPDX-License-Identifier: MIT OR Apache-2.0

import {
  InfiniteData,
  QueryFilters,
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { useRef, useState } from "react";
import { FrameworkClient, useFrameworkClient } from "@/framework/runtime";
import {
  readingQueueInfiniteQueryOptions,
  readingQueueRootKey,
  ReadingQueueSort,
  ReadingQueueStatusFilter,
} from "./queries";

type ReadingQueuePage = Awaited<
  ReturnType<FrameworkClient["listReadingQueueEntries"]>
>;

export function useReadingQueue(
  status: ReadingQueueStatusFilter,
  sort: ReadingQueueSort,
) {
  const client = useFrameworkClient();
  const queryClient = useQueryClient();
  const options = readingQueueInfiniteQueryOptions(client, status, sort, 10);
  const refreshGeneration = useRef(0);
  const refreshing = useRef(false);
  const saveRevision = useRef(0);
  const [savedNeedsRefresh, setSavedNeedsRefresh] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const queue = useInfiniteQuery({
    ...options,
    async queryFn(context) {
      const revision = saveRevision.current;
      const page = await options.queryFn(context);
      // A fulfilled refetch promise may mean cancellation or an offline pause.
      // Only a real, uncancelled first-page read can acknowledge this write.
      if (
        context.pageParam === undefined &&
        !context.signal.aborted &&
        revision === saveRevision.current
      ) {
        setSavedNeedsRefresh(false);
      }
      return page;
    },
  });

  async function refresh(afterSave = false) {
    const generation = ++refreshGeneration.current;
    refreshing.current = true;
    if (afterSave) {
      saveRevision.current += 1;
      setSavedNeedsRefresh(true);
    }
    setIsRefreshing(true);
    const filters: QueryFilters = afterSave
      ? { queryKey: readingQueueRootKey }
      : { queryKey: options.queryKey, exact: true };
    try {
      // Cancel before truncating so a late next page cannot restore old pages.
      await queryClient.cancelQueries(filters);
      if (generation !== refreshGeneration.current) return;
      queryClient.setQueriesData<InfiniteData<ReadingQueuePage>>(
        filters,
        (data) =>
          data === undefined
            ? undefined
            : {
                pages: data.pages.slice(0, 1),
                pageParams: data.pageParams.slice(0, 1),
              },
      );
      await queryClient.invalidateQueries({ ...filters, refetchType: "none" });
      if (generation !== refreshGeneration.current) return;
      await queryClient.refetchQueries(
        { ...filters, type: "active" },
        { throwOnError: true },
      );
    } catch {
      // The observed query exposes the read failure independently of the write.
    } finally {
      if (generation === refreshGeneration.current) {
        refreshing.current = false;
        setIsRefreshing(false);
      }
    }
  }

  const createEntry = useMutation({
    mutationKey: ["reading-queue-create"],
    mutationFn: (input: { sourceUrl: string; title: string }) =>
      client.createReadingQueueEntry(input),
    onSuccess() {
      void refresh(true);
    },
  });
  const changeEntryState = useMutation({
    mutationKey: ["reading-queue-change-state"],
    mutationFn: ({
      id,
      state,
    }: {
      id: string;
      state: "queued" | "completed";
    }) => client.changeReadingQueueEntryState(id, { state }),
    onSuccess() {
      void refresh(true);
    },
  });

  return {
    entries: queue.data?.pages.flatMap((page) => page.entries) ?? [],
    hasData: queue.data !== undefined,
    isPending: queue.isPending,
    isSuccess: queue.isSuccess,
    error: queue.error,
    isFetching: isRefreshing || queue.isFetching,
    isRefreshing:
      isRefreshing || (queue.isFetching && !queue.isFetchingNextPage),
    isFetchingNextPage: queue.isFetchingNextPage,
    hasNextPage: queue.hasNextPage,
    refreshFailedAfterSave: savedNeedsRefresh && queue.isError,
    addEntry: createEntry.mutateAsync,
    changeState: changeEntryState.mutate,
    isChanging: changeEntryState.isPending,
    changeError: changeEntryState.error,
    refresh: () => {
      void refresh();
    },
    refreshAfterConflict: () => {
      changeEntryState.reset();
      void refresh();
    },
    loadMore: () => {
      if (
        !refreshing.current &&
        queue.hasNextPage &&
        !queryClient.isFetching({ queryKey: options.queryKey, exact: true })
      ) {
        void queue.fetchNextPage({ cancelRefetch: false });
      }
    },
  };
}
