// SPDX-License-Identifier: MIT OR Apache-2.0

import type { FrameworkFailure, PublicApiClientOptions } from "./client";
import { createRequestExecutor } from "./request";

export interface HealthStatus {
  status: string;
  database: string;
}

export function createHealthClient(
  options: Omit<PublicApiClientOptions, "credentialHeaders">,
) {
  const executeRequest = createRequestExecutor(options);
  return (signal?: AbortSignal): Promise<HealthStatus> =>
    executeRequest(async (requestFetch) => {
      let response: Response;
      try {
        response = await requestFetch("/health");
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
    }, signal);
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
