import { describe, expect, it } from 'vitest';

import { isTransportFailure } from './runtime';

describe('Framework query retry classification', () => {
  it('retries transport failures only', () => {
    expect(isTransportFailure({ kind: 'transport' })).toBe(true);
    expect(isTransportFailure({ kind: 'problem' })).toBe(false);
    expect(isTransportFailure({ kind: 'contractViolation' })).toBe(false);
  });
});
