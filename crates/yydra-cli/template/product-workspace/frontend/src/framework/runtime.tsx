// SPDX-License-Identifier: MIT OR Apache-2.0

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createContext, PropsWithChildren, useContext, useState } from "react";

export interface HealthStatus {
  status: string;
  database: string;
}

export interface FrameworkClient {
  health(signal?: AbortSignal): Promise<HealthStatus>;
}

type FrameworkFailure = {
  kind: "transport" | "contractViolation";
  message: string;
};

const FrameworkClientContext = createContext<FrameworkClient | null>(null);

export function FrameworkRuntime({
  children,
  client,
}: PropsWithChildren<{ client?: FrameworkClient }>) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry(failureCount, error) {
              return isTransportFailure(error) && failureCount < 2;
            },
          },
          mutations: { retry: false },
        },
      }),
  );
  const [runtimeClient] = useState(
    () => client ?? createFrameworkClient(globalThis.fetch),
  );

  return (
    <FrameworkClientContext.Provider value={runtimeClient}>
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    </FrameworkClientContext.Provider>
  );
}

export function useFrameworkClient(): FrameworkClient {
  const client = useContext(FrameworkClientContext);
  if (client === null) {
    throw new Error("useFrameworkClient requires FrameworkRuntime");
  }
  return client;
}

export function createFrameworkClient(
  fetchImplementation: typeof fetch,
  baseUrl = process.env.EXPO_PUBLIC_API_URL ?? "http://127.0.0.1:4000",
): FrameworkClient {
  return {
    async health(signal) {
      let response: Response;
      try {
        response = await fetchImplementation(`${baseUrl}/health`, { signal });
      } catch (cause) {
        throw {
          kind: "transport",
          message:
            cause instanceof Error ? cause.message : "health request failed",
        } satisfies FrameworkFailure;
      }
      if (!response.ok) {
        throw {
          kind: "transport",
          message: `health request returned HTTP ${response.status}`,
        } satisfies FrameworkFailure;
      }
      const body: unknown = await response.json();
      if (!isHealthStatus(body)) {
        throw {
          kind: "contractViolation",
          message: "health response does not match the Framework contract",
        } satisfies FrameworkFailure;
      }
      return body;
    },
  };
}

function isHealthStatus(value: unknown): value is HealthStatus {
  return (
    typeof value === "object" &&
    value !== null &&
    "status" in value &&
    typeof value.status === "string" &&
    "database" in value &&
    typeof value.database === "string"
  );
}

export function isTransportFailure(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    error.kind === "transport"
  );
}
