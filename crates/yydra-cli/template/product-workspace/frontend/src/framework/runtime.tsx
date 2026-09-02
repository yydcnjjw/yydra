// SPDX-License-Identifier: MIT OR Apache-2.0

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createContext, PropsWithChildren, useContext, useState } from "react";

import {
  ChangeReadingEntryStateRequest,
  CreateReadingEntryRequest,
  createPublicApiClient,
  FrameworkContractProfile,
  FrameworkFailure,
  FrameworkProtectedContract,
  isTransportFailure,
  ReadingQueueEntryResponse,
  ReadingQueueResponse,
} from "./api/client";

export { isFrameworkFailure, isTransportFailure } from "./api/client";

export interface HealthStatus {
  status: string;
  database: string;
}

export interface FrameworkClient {
  health(signal?: AbortSignal): Promise<HealthStatus>;
  frameworkContractProfile(
    signal?: AbortSignal,
  ): Promise<FrameworkContractProfile>;
  frameworkProtectedContract(
    signal?: AbortSignal,
  ): Promise<FrameworkProtectedContract>;
  listReadingQueueEntries(signal?: AbortSignal): Promise<ReadingQueueResponse>;
  createReadingQueueEntry(
    input: CreateReadingEntryRequest,
    signal?: AbortSignal,
  ): Promise<ReadingQueueEntryResponse>;
  changeReadingQueueEntryState(
    id: string,
    input: ChangeReadingEntryStateRequest,
    signal?: AbortSignal,
  ): Promise<ReadingQueueEntryResponse>;
}

const FrameworkClientContext = createContext<FrameworkClient | null>(null);

export function FrameworkRuntime({
  children,
  client,
}: PropsWithChildren<{ client?: FrameworkClient }>) {
  const [queryClient] = useState(() => createFrameworkQueryClient());
  const [runtimeClient] = useState(
    () => client ?? createFrameworkClient(globalThis.fetch),
  );

  return (
    <FrameworkClientContext.Provider value={runtimeClient}>
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    </FrameworkClientContext.Provider>
  );
}

export function createFrameworkQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry(failureCount, error) {
          return isTransportFailure(error) && failureCount < 2;
        },
      },
      mutations: { retry: false },
    },
  });
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
  credentialHeaders?: () => HeadersInit | Promise<HeadersInit>,
): FrameworkClient {
  const publicApi = createPublicApiClient({
    baseUrl,
    fetchImplementation,
    credentialHeaders,
  });
  return {
    frameworkContractProfile(signal) {
      return publicApi.frameworkContractProfile({ signal });
    },
    frameworkProtectedContract(signal) {
      return publicApi.frameworkProtectedContract({ signal });
    },
    listReadingQueueEntries(signal) {
      return publicApi.listReadingQueueEntries({ signal });
    },
    createReadingQueueEntry(input, signal) {
      return publicApi.createReadingQueueEntry(input, { signal });
    },
    changeReadingQueueEntryState(id, input, signal) {
      return publicApi.changeReadingQueueEntryState(id, input, { signal });
    },
    async health(signal) {
      let response: Response;
      try {
        response = await fetchImplementation(`${baseUrl}/health`, { signal });
      } catch (cause) {
        if (signal?.aborted) {
          throw {
            kind: "cancelled",
            message: "health request was cancelled by its caller",
          } satisfies FrameworkFailure;
        }
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
