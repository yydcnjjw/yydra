import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('react-native', () => ({ Platform: { OS: 'web' } }));
vi.mock('@/generated/public-api/client', () => ({
  completeReadingEntry: vi.fn(),
  createReadingEntry: vi.fn(),
  listReadingEntries: vi.fn(),
  reopenReadingEntry: vi.fn(),
}));

import { listReadingEntries } from '@/generated/public-api/client';

import { getReadingEntries } from './readingQueue';

const mockedList = vi.mocked(listReadingEntries);
const headers = new Headers();
const problem = {
  type: 'https://yydra.dev/problems/invalid-cursor',
  title: 'Invalid cursor',
  status: 400,
  detail: 'The cursor is malformed',
  traceId: '019d0000-0000-7000-8000-000000000002',
};

beforeEach(() => mockedList.mockReset());

describe('Reading Queue Framework facade', () => {
  it('returns a declared success response', async () => {
    mockedList.mockResolvedValue({
      data: { items: [], nextCursor: null },
      status: 200,
      headers,
    });

    await expect(getReadingEntries('all', undefined)).resolves.toEqual({
      items: [],
      nextCursor: null,
    });
  });

  it('classifies a declared Problem without converting it to a schema failure', async () => {
    mockedList.mockResolvedValue({ data: problem, status: 400, headers });

    await expect(getReadingEntries('all', 'bad-cursor')).rejects.toMatchObject({
      kind: 'problem',
      problem,
    });
  });

  it('rejects an undocumented status even when the payload looks valid', async () => {
    mockedList.mockImplementationOnce(async () => ({
      data: problem,
      status: 418,
      headers,
    }) as never);

    await expect(getReadingEntries('all', undefined)).rejects.toThrow(
      'undocumented HTTP status 418',
    );
  });
});
