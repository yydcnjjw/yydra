// SPDX-License-Identifier: MIT OR Apache-2.0

import { focusManager, onlineManager } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

import {
  createFrameworkClient,
  createFrameworkQueryClient,
  createProductionFrameworkRuntime,
  createTestFrameworkRuntime,
  isTransportFailure,
} from "./runtime";
describe("Framework Runtime health client", () => {
  it("reaches the configured service and accepts the real health shape", async () => {
    const fetchImplementation = vi.fn<typeof fetch>(async () =>
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
    expect(String(fetchImplementation.mock.calls[0][0])).toBe(
      "http://service.test/health",
    );
    expect(fetchImplementation.mock.calls[0][1]?.signal).toBeInstanceOf(
      AbortSignal,
    );
  });

  it("classifies malformed health JSON as a contract violation", async () => {
    const client = createFrameworkClient(
      vi.fn(async () => new Response("not JSON", { status: 200 })),
      "http://service.test",
    );

    await expect(client.health()).rejects.toMatchObject({
      kind: "contractViolation",
    });
  });

  it("classifies only transport failures as retryable", () => {
    expect(isTransportFailure({ kind: "transport", message: "offline" })).toBe(
      true,
    );
    expect(isTransportFailure({ kind: "transport" })).toBe(false);
    expect(
      isTransportFailure({ kind: "contractViolation", message: "invalid" }),
    ).toBe(false);
  });

  it("never retries mutations automatically", () => {
    expect(
      createFrameworkQueryClient().getDefaultOptions().mutations?.retry,
    ).toBe(false);
  });

  it("retries transport failures at most twice with bounded backoff", async () => {
    vi.useFakeTimers();
    try {
      const queryClient = createFrameworkQueryClient();
      const query = vi.fn(async () => {
        throw { kind: "transport", message: "offline" };
      });
      const result = queryClient
        .fetchQuery({ queryKey: ["retry-contract"], queryFn: query })
        .catch((error: unknown) => error);

      await vi.runAllTimersAsync();

      await expect(result).resolves.toMatchObject({ kind: "transport" });
      expect(query).toHaveBeenCalledTimes(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it.each(["problem", "cancelled", "contractViolation"] as const)(
    "does not retry %s failures",
    async (kind) => {
      const queryClient = createFrameworkQueryClient();
      const query = vi.fn(async () => {
        throw { kind, message: `${kind} failure` };
      });

      await expect(
        queryClient.fetchQuery({ queryKey: [kind], queryFn: query }),
      ).rejects.toMatchObject({ kind });
      expect(query).toHaveBeenCalledTimes(1);
    },
  );

  it("reports sanitized structured query diagnostics", async () => {
    const diagnostics = vi.fn();
    const queryClient = createFrameworkQueryClient({ diagnostics });

    await expect(
      queryClient.fetchQuery({
        queryKey: ["workspace-health", { secret: "must-not-leak" }],
        queryFn: async () => {
          throw { kind: "contractViolation", message: "raw response detail" };
        },
      }),
    ).rejects.toMatchObject({ kind: "contractViolation" });

    expect(diagnostics).toHaveBeenCalledWith({
      event: "query-failed",
      failureKind: "contractViolation",
      operation: "workspace-health",
      retryable: false,
    });
    expect(JSON.stringify(diagnostics.mock.calls)).not.toContain("secret");
    expect(JSON.stringify(diagnostics.mock.calls)).not.toContain(
      "raw response detail",
    );

    diagnostics.mockClear();
    await expect(
      queryClient.fetchQuery({
        queryKey: ["secret-first-key"],
        queryFn: async () => {
          throw { kind: "contractViolation", message: "failure" };
        },
      }),
    ).rejects.toMatchObject({ kind: "contractViolation" });
    expect(diagnostics).toHaveBeenCalledWith(
      expect.objectContaining({ operation: "query" }),
    );
    expect(JSON.stringify(diagnostics.mock.calls)).not.toContain(
      "secret-first-key",
    );
  });

  it("creates isolated no-retry Test Runtimes around an injected fake client", () => {
    const fakeClient = {} as never;
    const first = createTestFrameworkRuntime(fakeClient);
    const second = createTestFrameworkRuntime(fakeClient);

    expect(first.client).toBe(fakeClient);
    expect(first.queryClient).not.toBe(second.queryClient);
    expect(first.queryClient.getDefaultOptions().queries?.retry).toBe(false);
    expect(first.queryClient.getDefaultOptions().mutations?.retry).toBe(false);
  });

  it("connects production online and focus signals to TanStack Query", () => {
    let reportOnline: ((online: boolean) => void) | undefined;
    let reportFocused: ((focused: boolean) => void) | undefined;
    const stopOnline = vi.fn();
    const stopFocused = vi.fn();
    const runtime = createProductionFrameworkRuntime({
      client: {} as never,
      signals: {
        subscribeFocused(listener) {
          reportFocused = listener;
          return stopFocused;
        },
        subscribeOnline(listener) {
          reportOnline = listener;
          return stopOnline;
        },
      },
    });

    const stop = runtime.start();
    reportOnline?.(false);
    reportFocused?.(false);
    expect(onlineManager.isOnline()).toBe(false);
    expect(focusManager.isFocused()).toBe(false);

    stop();
    expect(stopOnline).toHaveBeenCalledOnce();
    expect(stopFocused).toHaveBeenCalledOnce();
    onlineManager.setOnline(true);
    focusManager.setFocused(true);
  });

  it("propagates TanStack Query cancellation through AbortSignal", async () => {
    let observedSignal: AbortSignal | undefined;
    const queryClient = createFrameworkQueryClient();
    const queryKey = ["abort-signal-contract"];
    const request = queryClient.fetchQuery({
      queryKey,
      queryFn: ({ signal }) =>
        new Promise((_resolve, reject) => {
          observedSignal = signal;
          signal.addEventListener("abort", () => reject(signal.reason));
        }),
    });
    await vi.waitFor(() => expect(observedSignal).toBeDefined());

    await queryClient.cancelQueries({ queryKey });
    await request.catch(() => undefined);

    expect(observedSignal?.aborted).toBe(true);
  });

  it("forwards Reading Queue behavior through the Framework facade", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(
      async (_input, init) => {
        if (init?.method === "PATCH") {
          return Response.json({
            id: "opaque-entry",
            title: "Example",
            sourceUrl: "https://example.test",
            state: "completed",
          });
        }
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
        return Response.json({ entries: [], nextCursor: null });
      },
    );
    const client = createFrameworkClient(
      fetchImplementation,
      "http://service.test",
    );

    await expect(client.listReadingQueueEntries()).resolves.toEqual({
      entries: [],
      nextCursor: null,
    });
    await expect(
      client.createReadingQueueEntry({
        title: "Example",
        sourceUrl: "https://example.test",
      }),
    ).resolves.toMatchObject({ id: "opaque-entry", state: "queued" });
    await expect(
      client.changeReadingQueueEntryState("opaque-entry", {
        state: "completed",
      }),
    ).resolves.toMatchObject({ id: "opaque-entry", state: "completed" });
  });

  it("injects credentials through the same production assembly seam", async () => {
    const fetchImplementation = vi.fn<typeof globalThis.fetch>(async () =>
      Response.json({ access: "granted" }),
    );
    const client = createFrameworkClient(
      fetchImplementation,
      "http://service.test",
      () => ({ Authorization: "Bearer contract-test" }),
    );

    await expect(client.frameworkProtectedContract()).resolves.toEqual({
      access: "granted",
    });
    expect(
      new Headers(fetchImplementation.mock.calls[0][1]?.headers).get(
        "authorization",
      ),
    ).toBe("Bearer contract-test");
  });
});
