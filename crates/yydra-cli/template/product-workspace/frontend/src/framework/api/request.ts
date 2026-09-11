// SPDX-License-Identifier: MIT OR Apache-2.0

import type { FrameworkFailure, PublicApiClientOptions } from "./client";

// The deadline covers credentials, transport, and consumption/validation of the body.
// Each caller retains its endpoint-specific HTTP and response contract.
export function createRequestExecutor({
  baseUrl,
  fetchImplementation,
  credentialHeaders,
  timeoutMs = 10_000,
}: PublicApiClientOptions) {
  const origin = normalizeBaseUrl(baseUrl);
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) {
    throw new Error("timeoutMs must be a positive safe integer");
  }

  return async function executeRequest<T>(
    consume: (requestFetch: typeof globalThis.fetch) => Promise<T>,
    callerSignal?: AbortSignal,
  ): Promise<T> {
    const controller = new AbortController();
    let timedOut = false;
    const cancel = () => controller.abort(callerSignal?.reason);
    callerSignal?.addEventListener("abort", cancel, { once: true });
    if (callerSignal?.aborted) cancel();
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort(new Error("request timed out"));
    }, timeoutMs);
    let onAbort: () => void;
    const aborted = new Promise<never>((_resolve, reject) => {
      onAbort = () => reject(controller.signal.reason);
      controller.signal.addEventListener("abort", onAbort, { once: true });
      if (controller.signal.aborted) onAbort();
    });
    const requestFetch: typeof globalThis.fetch = async (input, init) => {
      if (controller.signal.aborted) throw controller.signal.reason;
      const headers = new Headers(init?.headers);
      if (credentialHeaders) {
        const injected = new Headers(await credentialHeaders());
        injected.forEach((value, name) => headers.set(name, value));
      }
      // Credentials may finish after cancellation even if their provider ignores it.
      if (controller.signal.aborted) throw controller.signal.reason;
      return fetchImplementation(resolveUrl(input, origin), {
        ...init,
        headers,
        signal: controller.signal,
      });
    };
    try {
      return await Promise.race([consume(requestFetch), aborted]);
    } catch (cause) {
      if (callerSignal?.aborted) {
        throw {
          kind: "cancelled",
          message: "request was cancelled by its caller",
        } satisfies FrameworkFailure;
      }
      if (timedOut) {
        throw {
          kind: "transport",
          message: `request timed out after ${timeoutMs} ms`,
        } satisfies FrameworkFailure;
      }
      throw cause;
    } finally {
      clearTimeout(timeout);
      controller.signal.removeEventListener("abort", onAbort!);
      callerSignal?.removeEventListener("abort", cancel);
    }
  };
}

function normalizeBaseUrl(value: string): URL {
  const url = new URL(value);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("baseUrl must use http or https");
  }
  url.pathname = "/";
  url.search = "";
  url.hash = "";
  return url;
}

function resolveUrl(input: RequestInfo | URL, baseUrl: URL): URL {
  const value =
    typeof input === "string"
      ? input
      : input instanceof URL
        ? input.href
        : input.url;
  return new URL(value, baseUrl);
}
