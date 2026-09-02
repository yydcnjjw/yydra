// SPDX-License-Identifier: MIT OR Apache-2.0

export function isContainedRelativePath(relativePath, separator, absolute) {
  return !absolute && relativePath !== '..' && !relativePath.startsWith(`..${separator}`);
}

export function parseH5Port(raw) {
  if (!/^[1-9][0-9]*$/.test(raw)) {
    throw new Error('YYDRA_H5_PORT must be an integer from 1 through 65535');
  }
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error('YYDRA_H5_PORT must be an integer from 1 through 65535');
  }
  return port;
}
