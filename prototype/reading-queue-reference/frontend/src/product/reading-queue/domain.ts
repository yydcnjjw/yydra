import type {
  ReadingEntry as ApiReadingEntry,
  ReadingStatus,
} from '@/framework/api/readingQueue';

export type ReadingEntry = ApiReadingEntry;
export type ReadingQueueFilter = ReadingStatus | 'all';

export type ReadingEntryAction = 'complete' | 'reopen';

/** Presentation guidance only; the Rust Product Domain remains authoritative. */
export function availableAction(entry: ReadingEntry): ReadingEntryAction {
  return entry.status === 'queued' ? 'complete' : 'reopen';
}
