// SPDX-License-Identifier: MIT OR Apache-2.0

import { afterEach, describe, expect, it, vi } from "vitest";

import { createPublicApiClient, isTransportFailure } from "./client";

const validProfile = {
  opaqueId: "framework-contract-v1",
  occurredAt: "2000-01-01T00:00:00Z",
  safeCount: 1,
  exactAmount: "0.01",
  items: [],
  nullableNote: null,
};

const validEntry = {
  id: "opaque-entry-1",
  title: "A useful article",
  sourceUrl: "https://example.test/article",
  state: "queued" as const,
};

afterEach(() => {
  vi.useRealTimers();
});

describe("Framework Public API facade", () => {
  it("uses injected base URL, credentials, Fetch, and permissive response validation", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(async () =>
      Response.json({ ...validProfile, additiveFutureField: "ignored" }),
    );
    const client = createPublicApiClient({
      baseUrl: "https://service.test/root",
      fetchImplementation,
      credentialHeaders: () => ({ Authorization: "Bearer test-credential" }),
    });

    await expect(client.frameworkContractProfile()).resolves.toEqual(
      validProfile,
    );
    const [url, init] = fetchImplementation.mock.calls[0];
    expect(String(url)).toBe("https://service.test/api/v1/framework-contract");
    expect(new Headers(init?.headers).get("authorization")).toBe(
      "Bearer test-credential",
    );
  });

  it("lists and creates Reading Queue entries only through generated operations", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(
      async (input, init) => {
        if (init?.method === "POST") {
          expect(JSON.parse(String(init.body))).toEqual({
            title: validEntry.title,
            sourceUrl: validEntry.sourceUrl,
          });
          expect(new Headers(init.headers).get("content-type")).toBe(
            "application/json",
          );
          return Response.json(
            { ...validEntry, additiveFutureField: "accepted" },
            { status: 201 },
          );
        }
        return Response.json({
          entries: [{ ...validEntry, additiveFutureField: "accepted" }],
          additivePageField: "accepted",
        });
      },
    );
    const client = createPublicApiClient({
      baseUrl: "https://service.test/root",
      fetchImplementation,
    });

    await expect(client.listReadingQueueEntries()).resolves.toEqual({
      entries: [validEntry],
    });
    await expect(
      client.createReadingQueueEntry({
        title: validEntry.title,
        sourceUrl: validEntry.sourceUrl,
      }),
    ).resolves.toEqual(validEntry);
    expect(String(fetchImplementation.mock.calls[0][0])).toBe(
      "https://service.test/api/v1/reading-queue/entries",
    );
    expect(String(fetchImplementation.mock.calls[1][0])).toBe(
      "https://service.test/api/v1/reading-queue/entries",
    );
  });

  it("rejects unknown create fields before Fetch and exposes declared validation Problems", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(async () =>
      Response.json(
        {
          type: "https://yydra.dev/problems/invalid-reading-entry",
          title: "Invalid reading entry",
          status: 422,
          detail: "title must not be empty",
        },
        {
          status: 422,
          headers: { "content-type": "application/problem+json" },
        },
      ),
    );
    const client = createPublicApiClient({
      baseUrl: "https://service.test",
      fetchImplementation,
    });

    await expect(
      client.createReadingQueueEntry({
        title: "Example",
        sourceUrl: "https://example.test",
        handEditedField: true,
      } as never),
    ).rejects.toMatchObject({ kind: "contractViolation" });
    expect(fetchImplementation).not.toHaveBeenCalled();

    await expect(
      client.createReadingQueueEntry({
        title: "",
        sourceUrl: "https://x.test",
      }),
    ).rejects.toMatchObject({
      kind: "problem",
      problem: { status: 422 },
    });
  });

  it("classifies a malformed Reading Queue state as contractViolation", async () => {
    const client = createPublicApiClient({
      baseUrl: "https://service.test",
      fetchImplementation: async () =>
        Response.json({ entries: [{ ...validEntry, state: "invented" }] }),
    });

    await expect(client.listReadingQueueEntries()).rejects.toMatchObject({
      kind: "contractViolation",
    });
  });

  it("returns a contract-valid RFC 9457 Problem as the stable problem outcome", async () => {
    const problem = {
      type: "https://yydra.dev/problems/example",
      title: "Example failure",
      status: 500,
      traceId: "trace-public",
      additiveExtension: "accepted",
    };
    const client = createPublicApiClient({
      baseUrl: "https://service.test",
      fetchImplementation: async () =>
        new Response(JSON.stringify(problem), {
          status: 500,
          headers: { "content-type": "application/problem+json" },
        }),
    });

    await expect(client.frameworkContractProfile()).rejects.toMatchObject({
      kind: "problem",
      problem: {
        type: problem.type,
        status: 500,
      },
    });
  });

  it.each([
    [
      "undocumented status",
      new Response(JSON.stringify(validProfile), {
        status: 201,
        headers: { "content-type": "application/json" },
      }),
    ],
    [
      "undocumented content type",
      new Response(JSON.stringify(validProfile), {
        status: 200,
        headers: { "content-type": "text/plain" },
      }),
    ],
    [
      "malformed response",
      Response.json({ ...validProfile, safeCount: "not-an-integer" }),
    ],
    [
      "non-UTC timestamp",
      Response.json({
        ...validProfile,
        occurredAt: "2000-01-01T01:00:00+01:00",
      }),
    ],
  ])("classifies %s as contractViolation", async (_name, response) => {
    const client = createPublicApiClient({
      baseUrl: "https://service.test",
      fetchImplementation: async () => response,
    });

    await expect(client.frameworkContractProfile()).rejects.toMatchObject({
      kind: "contractViolation",
    });
  });

  it("classifies network failure as transport and caller abort as cancelled", async () => {
    const transportClient = createPublicApiClient({
      baseUrl: "https://service.test",
      fetchImplementation: async () => {
        throw new TypeError("network unavailable");
      },
    });
    await expect(transportClient.frameworkContractProfile()).rejects.toSatisfy(
      isTransportFailure,
    );

    const cancelledClient = createPublicApiClient({
      baseUrl: "https://service.test",
      fetchImplementation: () => new Promise(() => {}),
    });
    const controller = new AbortController();
    const request = cancelledClient.frameworkContractProfile({
      signal: controller.signal,
    });
    const cancelled = expect(request).rejects.toMatchObject({
      kind: "cancelled",
    });
    controller.abort();
    await cancelled;
  });

  it("enforces the configured timeout as a non-retryable transport failure", async () => {
    vi.useFakeTimers();
    const client = createPublicApiClient({
      baseUrl: "https://service.test",
      timeoutMs: 5,
      fetchImplementation: () => new Promise(() => {}),
    });
    const request = client.frameworkContractProfile();
    const timedOut = expect(request).rejects.toMatchObject({
      kind: "transport",
      message: "request timed out after 5 ms",
    });
    await vi.advanceTimersByTimeAsync(5);
    await timedOut;
  });
});
