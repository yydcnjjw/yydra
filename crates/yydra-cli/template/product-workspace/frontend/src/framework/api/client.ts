// SPDX-License-Identifier: MIT OR Apache-2.0

import { getFrameworkContractProfile } from "../../generated/public-api/fetch/client";
import {
  FrameworkContractProfile,
  ProblemDetails,
} from "../../generated/public-api/fetch/schemas";

export type { FrameworkContractProfile };

export type FrameworkFailure =
  | { kind: "problem"; problem: ProblemDetails }
  | { kind: "transport"; message: string }
  | { kind: "cancelled"; message: string }
  | { kind: "contractViolation"; message: string };

export interface PublicApiClient {
  frameworkContractProfile(options?: {
    signal?: AbortSignal;
  }): Promise<FrameworkContractProfile>;
}

export interface PublicApiClientOptions {
  baseUrl: string;
  fetchImplementation: typeof globalThis.fetch;
  credentialHeaders?: () => HeadersInit | Promise<HeadersInit>;
  timeoutMs?: number;
}

class DeclaredProblem extends Error {
  constructor(readonly problem: ProblemDetails) {
    super("the service returned a declared Problem Details response");
  }
}

class ContractViolation extends Error {}

export function createPublicApiClient({
  baseUrl,
  fetchImplementation,
  credentialHeaders,
  timeoutMs = 10_000,
}: PublicApiClientOptions): PublicApiClient {
  const origin = normalizeBaseUrl(baseUrl);
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) {
    throw new Error("timeoutMs must be a positive safe integer");
  }

  return {
    async frameworkContractProfile(options) {
      const callerSignal = options?.signal;
      const controller = new AbortController();
      let timedOut = false;
      let responseReceived = false;
      const cancel = () => controller.abort(callerSignal?.reason);
      callerSignal?.addEventListener("abort", cancel, { once: true });
      if (callerSignal?.aborted) {
        cancel();
      }
      const timeout = setTimeout(() => {
        timedOut = true;
        controller.abort(new Error("request timed out"));
      }, timeoutMs);
      let internalAbortListener: (() => void) | undefined;
      const aborted = new Promise<never>((_resolve, reject) => {
        internalAbortListener = () => {
          reject(
            controller.signal.reason ??
              new DOMException("request aborted", "AbortError"),
          );
        };
        controller.signal.addEventListener("abort", internalAbortListener, {
          once: true,
        });
        if (controller.signal.aborted) {
          internalAbortListener();
        }
      });

      const runtimeFetch: typeof globalThis.fetch = async (input, init) => {
        const headers = new Headers(init?.headers);
        if (credentialHeaders) {
          const injected = new Headers(await credentialHeaders());
          injected.forEach((value, name) => headers.set(name, value));
        }
        const response = await fetchImplementation(resolveUrl(input, origin), {
          ...init,
          headers,
          signal: controller.signal,
        });
        responseReceived = true;
        const contentType = mediaType(response.headers.get("content-type"));
        if (response.status === 500) {
          if (contentType !== "application/problem+json") {
            throw new ContractViolation(
              `status 500 used undocumented content type ${contentType ?? "missing"}`,
            );
          }
          let body: unknown;
          try {
            body = await response.clone().json();
          } catch {
            throw new ContractViolation(
              "Problem response body is not valid JSON",
            );
          }
          const parsed = ProblemDetails.safeParse(body);
          if (!parsed.success || parsed.data.status !== response.status) {
            throw new ContractViolation(
              "Problem response does not match its declared schema and HTTP status",
            );
          }
          throw new DeclaredProblem(parsed.data);
        }
        if (response.status !== 200) {
          throw new ContractViolation(
            `status ${response.status} is not declared for getFrameworkContractProfile`,
          );
        }
        if (contentType !== "application/json") {
          throw new ContractViolation(
            `status 200 used undocumented content type ${contentType ?? "missing"}`,
          );
        }
        return response;
      };

      try {
        const response = await Promise.race([
          getFrameworkContractProfile(
            { signal: controller.signal },
            runtimeFetch,
          ),
          aborted,
        ]);
        if (response.status !== 200) {
          throw new ContractViolation(
            `generated client returned undocumented status ${response.status}`,
          );
        }
        return response.data;
      } catch (cause) {
        if (cause instanceof DeclaredProblem) {
          throw {
            kind: "problem",
            problem: cause.problem,
          } satisfies FrameworkFailure;
        }
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
        if (cause instanceof ContractViolation || responseReceived) {
          throw {
            kind: "contractViolation",
            message:
              cause instanceof Error
                ? cause.message
                : "response violated the generated Public API Contract",
          } satisfies FrameworkFailure;
        }
        throw {
          kind: "transport",
          message:
            cause instanceof Error ? cause.message : "network request failed",
        } satisfies FrameworkFailure;
      } finally {
        clearTimeout(timeout);
        if (internalAbortListener) {
          controller.signal.removeEventListener("abort", internalAbortListener);
        }
        callerSignal?.removeEventListener("abort", cancel);
      }
    },
  };
}

export function isFrameworkFailure(value: unknown): value is FrameworkFailure {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    ["problem", "transport", "cancelled", "contractViolation"].includes(
      String(value.kind),
    )
  );
}

export function isTransportFailure(value: unknown): boolean {
  return isFrameworkFailure(value) && value.kind === "transport";
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

function mediaType(value: string | null): string | null {
  return value?.split(";", 1)[0]?.trim().toLowerCase() ?? null;
}
