// SPDX-License-Identifier: MIT OR Apache-2.0

import { describe, expect, it, vi } from "vitest";

import { createFrameworkClient, isTransportFailure } from "./runtime";

describe("Framework Runtime health client", () => {
  it("reaches the configured service and accepts the real health shape", async () => {
    const fetchImplementation = vi.fn(async () =>
      Response.json({ status: "ready", database: "baseline" }),
    );
    const client = createFrameworkClient(
      fetchImplementation,
      "http://service.test",
    );

    await expect(client.health()).resolves.toEqual({
      status: "ready",
      database: "baseline",
    });
    expect(fetchImplementation).toHaveBeenCalledWith(
      "http://service.test/health",
      { signal: undefined },
    );
  });

  it("classifies only transport failures as retryable", () => {
    expect(isTransportFailure({ kind: "transport" })).toBe(true);
    expect(isTransportFailure({ kind: "contractViolation" })).toBe(false);
  });

  it("forwards Reading Queue behavior through the Framework facade", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(
      async (_input, init) => {
        if (init?.method === "POST") {
          return Response.json(
            {
              id: "opaque-entry",
              title: "Example",
              sourceUrl: "https://example.test",
              state: "queued",
            },
            { status: 201 },
          );
        }
        return Response.json({ entries: [] });
      },
    );
    const client = createFrameworkClient(
      fetchImplementation,
      "http://service.test",
    );

    await expect(client.listReadingQueueEntries()).resolves.toEqual({
      entries: [],
    });
    await expect(
      client.createReadingQueueEntry({
        title: "Example",
        sourceUrl: "https://example.test",
      }),
    ).resolves.toMatchObject({ id: "opaque-entry", state: "queued" });
  });
});
