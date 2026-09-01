import {
  InfiniteData,
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import {
  addReadingEntry,
  completeEntry,
  getReadingEntries,
  ReadingEntryPage,
  reopenEntry,
} from '@/framework/api/readingQueue';

import type { ReadingEntryAction, ReadingQueueFilter } from './domain';

const readingQueueKey = ['readingQueue'] as const;

export function useReadingEntries(status: ReadingQueueFilter) {
  return useInfiniteQuery<ReadingEntryPage, Error, InfiniteData<ReadingEntryPage>, string[], string | undefined>({
    queryKey: [...readingQueueKey, status],
    queryFn: ({ pageParam, signal }) => getReadingEntries(status, pageParam, signal),
    initialPageParam: undefined,
    getNextPageParam: (page) => page.nextCursor ?? undefined,
  });
}

export function useAddReadingEntry() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: addReadingEntry,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: readingQueueKey }),
  });
}

export function useReadingEntryTransition() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, action }: { id: string; action: ReadingEntryAction }) =>
      action === 'complete' ? completeEntry(id) : reopenEntry(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: readingQueueKey }),
  });
}
