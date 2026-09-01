import {
  completeReadingEntry,
  createReadingEntry,
  listReadingEntries,
  reopenReadingEntry,
} from '@/generated/public-api/client';
import type { Problem } from '@/generated/public-api/model';

import { ContractViolationError } from './fetcher';

export type ReadingStatus = 'queued' | 'completed';

export type ReadingEntry = {
  id: string;
  sourceUrl: string;
  status: ReadingStatus;
  title: string;
};

export type ReadingEntryPage = {
  items: ReadingEntry[];
  nextCursor: string | null;
};

export class ProblemResponseError extends Error {
  readonly kind = 'problem' as const;

  constructor(readonly problem: Problem) {
    super(problem.detail);
    this.name = 'ProblemResponseError';
  }
}

export async function getReadingEntries(
  status: ReadingStatus | 'all',
  cursor: string | undefined,
  signal?: AbortSignal,
): Promise<ReadingEntryPage> {
  const response = await listReadingEntries(
    {
      status: status === 'all' ? undefined : status,
      cursor,
      limit: 20,
    },
    { signal },
  );

  return unwrapResponse<ReadingEntryPage>(response, 200, [400, 500]);
}

export async function addReadingEntry(input: {
  title: string;
  sourceUrl: string;
}): Promise<ReadingEntry> {
  const response = await createReadingEntry(input);
  return unwrapResponse<ReadingEntry>(response, 201, [400, 500]);
}

export async function completeEntry(id: string): Promise<ReadingEntry> {
  const response = await completeReadingEntry(id);
  return unwrapResponse<ReadingEntry>(response, 200, [404, 409, 500]);
}

export async function reopenEntry(id: string): Promise<ReadingEntry> {
  const response = await reopenReadingEntry(id);
  return unwrapResponse<ReadingEntry>(response, 200, [404, 409, 500]);
}

function unwrapResponse<T>(
  response: { data: unknown; status: number },
  successStatus: number,
  problemStatuses: readonly number[],
): T {
  if (response.status === successStatus) {
    return response.data as T;
  }
  if (problemStatuses.includes(response.status)) {
    throw new ProblemResponseError(response.data as Problem);
  }
  throw new ContractViolationError(
    `API returned undocumented HTTP status ${response.status}`,
  );
}
