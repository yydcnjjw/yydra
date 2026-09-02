// SPDX-License-Identifier: MIT OR Apache-2.0

import NetInfo from "@react-native-community/netinfo";
import {
  focusManager,
  MutationCache,
  onlineManager,
  QueryCache,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import {
  createContext,
  PropsWithChildren,
  useContext,
  useEffect,
  useState,
} from "react";
import { AppState, Platform } from "react-native";

import {
  ChangeReadingEntryStateRequest,
  CreateReadingEntryRequest,
  createPublicApiClient,
  FrameworkContractProfile,
  FrameworkFailure,
  FrameworkProtectedContract,
  isFrameworkFailure,
  isTransportFailure,
  ListReadingQueueEntriesParams,
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
  listReadingQueueEntries(
    input?: ListReadingQueueEntriesParams,
    signal?: AbortSignal,
  ): Promise<ReadingQueueResponse>;
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

export type FrameworkFailureKind = FrameworkFailure["kind"] | "unknown";

export interface FrameworkRuntimeDiagnostic {
  event: "query-failed" | "mutation-failed";
  failureKind: FrameworkFailureKind;
  operation: string;
  retryable: boolean;
}

export type FrameworkDiagnosticSink = (
  diagnostic: FrameworkRuntimeDiagnostic,
) => void;

export interface FrameworkRuntimeSignals {
  subscribeFocused(listener: (focused: boolean) => void): () => void;
  subscribeOnline(listener: (online: boolean) => void): () => void;
}

export interface FrameworkRuntimeAssembly {
  readonly client: FrameworkClient;
  readonly queryClient: QueryClient;
  start(): () => void;
}

export function FrameworkRuntime({
  children,
  runtime,
}: PropsWithChildren<{ runtime?: FrameworkRuntimeAssembly }>) {
  const [assembly] = useState(
    () => runtime ?? createProductionFrameworkRuntime(),
  );
  useEffect(() => assembly.start(), [assembly]);

  return (
    <FrameworkClientContext.Provider value={assembly.client}>
      <QueryClientProvider client={assembly.queryClient}>
        {children}
      </QueryClientProvider>
    </FrameworkClientContext.Provider>
  );
}

export function createFrameworkQueryClient(
  options: {
    diagnostics?: FrameworkDiagnosticSink;
    queryRetries?: boolean;
  } = {},
): QueryClient {
  const diagnostics = options.diagnostics ?? (() => undefined);
  const report = (
    event: FrameworkRuntimeDiagnostic["event"],
    error: unknown,
    operation: string,
  ) => {
    diagnostics({
      event,
      failureKind: frameworkFailureKind(error),
      operation,
      retryable: isTransportFailure(error),
    });
  };
  return new QueryClient({
    mutationCache: new MutationCache({
      onError(error, _variables, _context, mutation) {
        report(
          "mutation-failed",
          error,
          operationFromKey(mutation.options.mutationKey, "mutation"),
        );
      },
    }),
    defaultOptions: {
      queries: {
        retry:
          options.queryRetries === false
            ? false
            : (failureCount, error) =>
                isTransportFailure(error) && failureCount < 2,
        retryDelay: (attemptIndex) => Math.min(250 * 2 ** attemptIndex, 1_000),
      },
      mutations: { retry: false },
    },
    queryCache: new QueryCache({
      onError(error, query) {
        report(
          "query-failed",
          error,
          operationFromKey(query.queryKey, "query"),
        );
      },
    }),
  });
}

export function createProductionFrameworkRuntime(
  options: {
    client?: FrameworkClient;
    diagnostics?: FrameworkDiagnosticSink;
    signals?: FrameworkRuntimeSignals;
  } = {},
): FrameworkRuntimeAssembly {
  const diagnostics = options.diagnostics ?? defaultDiagnosticSink;
  const signals = options.signals ?? createPlatformRuntimeSignals();
  return {
    client: options.client ?? createFrameworkClient(globalThis.fetch),
    queryClient: createFrameworkQueryClient({ diagnostics }),
    start() {
      const stopOnline = signals.subscribeOnline((online) =>
        onlineManager.setOnline(online),
      );
      const stopFocused = signals.subscribeFocused((focused) =>
        focusManager.setFocused(focused),
      );
      return () => {
        stopFocused();
        stopOnline();
      };
    },
  };
}

export function createTestFrameworkRuntime(
  client: FrameworkClient,
  diagnostics?: FrameworkDiagnosticSink,
): FrameworkRuntimeAssembly {
  return {
    client,
    queryClient: createFrameworkQueryClient({
      diagnostics,
      queryRetries: false,
    }),
    start: () => () => undefined,
  };
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
    listReadingQueueEntries(input, signal) {
      return publicApi.listReadingQueueEntries(input, { signal });
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
      let body: unknown;
      try {
        body = await response.json();
      } catch {
        throw {
          kind: "contractViolation",
          message: "health response is not valid JSON",
        } satisfies FrameworkFailure;
      }
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

function frameworkFailureKind(error: unknown): FrameworkFailureKind {
  return isFrameworkFailure(error) ? error.kind : "unknown";
}

function operationFromKey(
  key: readonly unknown[] | undefined,
  fallback: string,
): string {
  const operation = key?.[0];
  return typeof operation === "string" &&
    [
      "reading-queue",
      "reading-queue-change-state",
      "reading-queue-create",
      "workspace-health",
    ].includes(operation)
    ? operation
    : fallback;
}

function defaultDiagnosticSink(diagnostic: FrameworkRuntimeDiagnostic): void {
  console.warn(
    JSON.stringify({ source: "yydra-framework-runtime", ...diagnostic }),
  );
}

function createPlatformRuntimeSignals(): FrameworkRuntimeSignals {
  if (Platform.OS === "web" && typeof window !== "undefined") {
    return {
      subscribeFocused(listener) {
        const report = () => listener(document.visibilityState !== "hidden");
        document.addEventListener("visibilitychange", report, false);
        report();
        return () =>
          document.removeEventListener("visibilitychange", report, false);
      },
      subscribeOnline(listener) {
        const report = () => listener(window.navigator.onLine);
        window.addEventListener("online", report, false);
        window.addEventListener("offline", report, false);
        report();
        return () => {
          window.removeEventListener("online", report, false);
          window.removeEventListener("offline", report, false);
        };
      },
    };
  }

  return {
    subscribeFocused(listener) {
      listener(AppState.currentState === "active");
      const subscription = AppState.addEventListener("change", (state) =>
        listener(state === "active"),
      );
      return () => subscription.remove();
    },
    subscribeOnline(listener) {
      return NetInfo.addEventListener((state) =>
        listener(
          state.isConnected === true && state.isInternetReachable !== false,
        ),
      );
    },
  };
}
