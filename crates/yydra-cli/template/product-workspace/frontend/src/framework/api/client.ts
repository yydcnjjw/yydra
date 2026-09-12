// SPDX-License-Identifier: MIT OR Apache-2.0

import {
  changeReadingQueueEntryState,
  createReadingQueueEntry,
  getFrameworkContractProfile,
  getFrameworkProtectedContract,
  listReadingQueueEntries,
} from "@yydra/generated-api/fetch/client";
import {
  ChangeReadingEntryStateRequest,
  CreateReadingEntryRequest,
  FrameworkContractProfile,
  FrameworkProtectedContract,
  ListReadingQueueEntriesParams,
  ProblemDetails,
  ReadingQueueEntryResponse,
  ReadingQueueResponse,
} from "@yydra/generated-api/fetch/schemas/index";
import { CreateReadingEntryRequest as StrictCreateReadingEntryRequest } from "@yydra/generated-api/request/schemas/createReadingEntryRequest.zod";
import {
  ChangeReadingQueueEntryStateParams,
  ListReadingQueueEntriesQueryParams,
} from "@yydra/generated-api/request/contracts";
import { ChangeReadingEntryStateRequest as StrictChangeReadingEntryStateRequest } from "@yydra/generated-api/request/schemas/changeReadingEntryStateRequest.zod";

import { createRequestExecutor } from "./request";

export type {
  ChangeReadingEntryStateRequest,
  CreateReadingEntryRequest,
  FrameworkContractProfile,
  FrameworkProtectedContract,
  ListReadingQueueEntriesParams,
  ReadingQueueEntryResponse,
  ReadingQueueResponse,
};

export type FrameworkFailure =
  | { kind: "problem"; problem: ProblemDetails }
  | { kind: "transport"; message: string }
  | { kind: "cancelled"; message: string }
  | { kind: "contractViolation"; message: string };

interface RequestOptions {
  signal?: AbortSignal;
}

export interface PublicApiClient {
  frameworkContractProfile(
    options?: RequestOptions,
  ): Promise<FrameworkContractProfile>;
  frameworkProtectedContract(
    options?: RequestOptions,
  ): Promise<FrameworkProtectedContract>;
  listReadingQueueEntries(
    input?: ListReadingQueueEntriesParams,
    options?: RequestOptions,
  ): Promise<ReadingQueueResponse>;
  createReadingQueueEntry(
    input: CreateReadingEntryRequest,
    options?: RequestOptions,
  ): Promise<ReadingQueueEntryResponse>;
  changeReadingQueueEntryState(
    id: string,
    input: ChangeReadingEntryStateRequest,
    options?: RequestOptions,
  ): Promise<ReadingQueueEntryResponse>;
}

export interface PublicApiClientOptions {
  baseUrl: string;
  fetchImplementation: typeof globalThis.fetch;
  credentialHeaders?: () => HeadersInit | Promise<HeadersInit>;
  timeoutMs?: number;
}

interface GeneratedResponse {
  data: unknown;
  status: number;
}

interface Operation {
  name: string;
  successStatus: number;
  problemStatuses: readonly number[];
  invoke(
    fetchImplementation: typeof globalThis.fetch,
  ): Promise<GeneratedResponse>;
  options?: RequestOptions;
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
  timeoutMs,
}: PublicApiClientOptions): PublicApiClient {
  const executeRequest = createRequestExecutor({
    baseUrl,
    fetchImplementation,
    credentialHeaders,
    timeoutMs,
  });

  async function execute<T>({
    name,
    successStatus,
    problemStatuses,
    invoke,
    options,
  }: Operation): Promise<T> {
    let responseReceived = false;
    const declaredProblems = new Set(problemStatuses);
    try {
      return await executeRequest(async (requestFetch) => {
        const runtimeFetch: typeof globalThis.fetch = async (input, init) => {
          const response = await requestFetch(input, init);
          responseReceived = true;
          const contentType = mediaType(response.headers.get("content-type"));
          if (declaredProblems.has(response.status)) {
            if (contentType !== "application/problem+json") {
              throw new ContractViolation(
                `status ${response.status} used undocumented content type ${contentType ?? "missing"}`,
              );
            }
            if (
              response.status === 401 &&
              !response.headers.get("www-authenticate")?.trim()
            ) {
              throw new ContractViolation(
                "status 401 is missing the required WWW-Authenticate challenge",
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
          if (response.status !== successStatus) {
            throw new ContractViolation(
              `status ${response.status} is not declared for ${name}`,
            );
          }
          if (contentType !== "application/json") {
            throw new ContractViolation(
              `status ${successStatus} used undocumented content type ${contentType ?? "missing"}`,
            );
          }
          return response;
        };

        const response = await invoke(runtimeFetch);
        if (response.status !== successStatus) {
          throw new ContractViolation(
            `generated client returned undocumented status ${response.status}`,
          );
        }
        return response.data as T;
      }, options?.signal);
    } catch (cause) {
      if (isFrameworkFailure(cause)) throw cause;
      if (cause instanceof DeclaredProblem) {
        throw {
          kind: "problem",
          problem: cause.problem,
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
    }
  }

  return {
    frameworkContractProfile(options) {
      return execute<FrameworkContractProfile>({
        name: "getFrameworkContractProfile",
        successStatus: 200,
        problemStatuses: [500],
        options,
        invoke: (runtimeFetch) =>
          getFrameworkContractProfile(
            { signal: options?.signal },
            runtimeFetch,
          ),
      });
    },
    frameworkProtectedContract(options) {
      return execute<FrameworkProtectedContract>({
        name: "getFrameworkProtectedContract",
        successStatus: 200,
        problemStatuses: [401, 403],
        options,
        invoke: (runtimeFetch) =>
          getFrameworkProtectedContract(
            { signal: options?.signal },
            runtimeFetch,
          ),
      });
    },
    listReadingQueueEntries(input = {}, options) {
      const parsed = ListReadingQueueEntriesQueryParams.safeParse(input);
      if (!parsed.success) {
        return Promise.reject({
          kind: "contractViolation",
          message: "listReadingQueueEntries input violated its query schema",
        } satisfies FrameworkFailure);
      }
      return execute<ReadingQueueResponse>({
        name: "listReadingQueueEntries",
        successStatus: 200,
        problemStatuses: [400, 401, 403, 500, 503],
        options,
        invoke: (runtimeFetch) =>
          listReadingQueueEntries(
            parsed.data,
            { signal: options?.signal },
            runtimeFetch,
          ),
      });
    },
    createReadingQueueEntry(input, options) {
      const parsed = StrictCreateReadingEntryRequest.safeParse(input);
      if (!parsed.success) {
        return Promise.reject({
          kind: "contractViolation",
          message: "createReadingQueueEntry input violated its request schema",
        } satisfies FrameworkFailure);
      }
      return execute<ReadingQueueEntryResponse>({
        name: "createReadingQueueEntry",
        successStatus: 201,
        problemStatuses: [400, 401, 403, 422, 500, 503],
        options,
        invoke: (runtimeFetch) =>
          createReadingQueueEntry(
            parsed.data,
            { signal: options?.signal },
            runtimeFetch,
          ),
      });
    },
    changeReadingQueueEntryState(id, input, options) {
      const parsedParams = ChangeReadingQueueEntryStateParams.safeParse({
        entry_id: id,
      });
      const parsedInput = StrictChangeReadingEntryStateRequest.safeParse(input);
      if (!parsedParams.success || !parsedInput.success) {
        return Promise.reject({
          kind: "contractViolation",
          message:
            "changeReadingQueueEntryState input violated its request schema",
        } satisfies FrameworkFailure);
      }
      return execute<ReadingQueueEntryResponse>({
        name: "changeReadingQueueEntryState",
        successStatus: 200,
        problemStatuses: [400, 401, 403, 404, 409, 422, 500, 503],
        options,
        invoke: (runtimeFetch) =>
          changeReadingQueueEntryState(
            encodeURIComponent(parsedParams.data.entry_id),
            parsedInput.data,
            { signal: options?.signal },
            runtimeFetch,
          ),
      });
    },
  };
}

export function isFrameworkFailure(value: unknown): value is FrameworkFailure {
  if (typeof value !== "object" || value === null || !("kind" in value)) {
    return false;
  }
  if (value.kind === "problem") {
    return (
      "problem" in value &&
      typeof value.problem === "object" &&
      value.problem !== null &&
      "type" in value.problem &&
      typeof value.problem.type === "string" &&
      "status" in value.problem &&
      typeof value.problem.status === "number"
    );
  }
  return (
    ["transport", "cancelled", "contractViolation"].includes(
      String(value.kind),
    ) &&
    "message" in value &&
    typeof value.message === "string"
  );
}

export function isTransportFailure(value: unknown): boolean {
  return isFrameworkFailure(value) && value.kind === "transport";
}

function mediaType(value: string | null): string | null {
  return value?.split(";", 1)[0]?.trim().toLowerCase() ?? null;
}
