import { afterEach, describe, expect, it, vi } from 'vitest';
import { z } from 'zod';

vi.mock('react-native', () => ({ Platform: { OS: 'web' } }));

import { ContractViolationError, frameworkFetch } from './fetcher';

const successSchema = z.object({ status: z.literal('ready') });
const problem = {
  type: 'https://yydra.dev/problems/invalid-input',
  title: 'Invalid input',
  status: 400,
  detail: 'title must not be empty',
  traceId: '019d0000-0000-7000-8000-000000000001',
};

afterEach(() => vi.unstubAllGlobals());

describe('frameworkFetch', () => {
  it('validates a success response with the generated operation schema', async () => {
    stubResponse(200, { status: 'ready' }, 'application/json');

    await expect(frameworkFetch('/health', { schema: successSchema })).resolves.toMatchObject({
      data: { status: 'ready' },
      status: 200,
    });
  });

  it('preserves a valid Problem response for operation-level classification', async () => {
    stubResponse(400, problem, 'application/problem+json');

    await expect(frameworkFetch('/reading-entries', { schema: successSchema })).resolves.toMatchObject({
      data: problem,
      status: 400,
    });
  });

  it('fails closed when a success body violates its generated schema', async () => {
    stubResponse(200, { status: 'wrong' }, 'application/json');

    await expect(frameworkFetch('/health', { schema: successSchema })).rejects.toBeInstanceOf(
      ContractViolationError,
    );
  });

  it('fails closed when an error is not Problem JSON', async () => {
    stubResponse(400, problem, 'application/json');

    await expect(frameworkFetch('/reading-entries', { schema: successSchema })).rejects.toThrow(
      'Expected application/problem+json',
    );
  });

  it('fails closed when Problem status disagrees with HTTP status', async () => {
    stubResponse(409, problem, 'application/problem+json');

    await expect(frameworkFetch('/reading-entries/x/complete', { schema: successSchema })).rejects.toThrow(
      'Problem status 400 disagrees with HTTP status 409',
    );
  });

  it('fails closed when JSON is malformed', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('{', {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );

    await expect(frameworkFetch('/health', { schema: successSchema })).rejects.toThrow(
      'body was not valid JSON',
    );
  });
});

function stubResponse(status: number, body: unknown, contentType: string) {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue(
      new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': contentType },
      }),
    ),
  );
}
