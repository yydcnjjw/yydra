// SPDX-License-Identifier: MIT OR Apache-2.0

import { describe, expect, it, vi } from 'vitest';

import { createFrameworkClient, isTransportFailure } from './runtime';

describe('Framework Runtime health client', () => {
  it('reaches the configured service and accepts the real health shape', async () => {
    const fetchImplementation = vi.fn(async () =>
      Response.json({ status: 'ready', database: 'baseline' }),
    );
    const client = createFrameworkClient(fetchImplementation, 'http://service.test');

    await expect(client.health()).resolves.toEqual({
      status: 'ready',
      database: 'baseline',
    });
    expect(fetchImplementation).toHaveBeenCalledWith(
      'http://service.test/health',
      { signal: undefined },
    );
  });

  it('classifies only transport failures as retryable', () => {
    expect(isTransportFailure({ kind: 'transport' })).toBe(true);
    expect(isTransportFailure({ kind: 'contractViolation' })).toBe(false);
  });
});
