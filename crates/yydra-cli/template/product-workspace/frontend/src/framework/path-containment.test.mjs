// SPDX-License-Identifier: MIT OR Apache-2.0

import { describe, expect, it } from 'vitest';

import { isContainedRelativePath, parseH5Port } from './path-containment.mjs';

describe('static asset containment', () => {
  it('accepts descendants and the distribution root', () => {
    expect(isContainedRelativePath('', '/', false)).toBe(true);
    expect(isContainedRelativePath('assets/app.js', '/', false)).toBe(true);
    expect(isContainedRelativePath('assets\\app.js', '\\', false)).toBe(true);
  });

  it('rejects parent traversal and absolute paths for both separators', () => {
    expect(isContainedRelativePath('../secret', '/', false)).toBe(false);
    expect(isContainedRelativePath('..\\secret', '\\', false)).toBe(false);
    expect(isContainedRelativePath('/outside', '/', true)).toBe(false);
    expect(isContainedRelativePath('C:\\outside', '\\', true)).toBe(false);
  });
});

describe('production H5 port', () => {
  it('accepts only a complete in-range integer', () => {
    expect(parseH5Port('8081')).toBe(8081);
    expect(() => parseH5Port('8081junk')).toThrow(/integer/);
    expect(() => parseH5Port(' 8081 ')).toThrow(/integer/);
    expect(() => parseH5Port('+8081')).toThrow(/integer/);
    expect(() => parseH5Port('1e3')).toThrow(/integer/);
    expect(() => parseH5Port('0')).toThrow(/integer/);
    expect(() => parseH5Port('65536')).toThrow(/integer/);
  });
});
