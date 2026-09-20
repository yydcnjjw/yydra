// SPDX-License-Identifier: MIT OR Apache-2.0
import {
  getProductSession,
  logoutProductSession,
  exchangeNativeHandoff,
} from "@yydra/generated-api/fetch/client";
import type { AuthApi, ProductSession } from "@yydra/auth";
import { ProblemDetails } from "@yydra/generated-api/fetch/schemas/index";
import type { FrameworkFailure } from "./client";

function session(value: unknown): ProductSession {
  if (typeof value !== "object" || value === null)
    throw new Error("Invalid session response");
  const v = value as Record<string, unknown>;
  if (
    (v.accountId !== null && typeof v.accountId !== "string") ||
    (v.expiresAt !== null &&
      (typeof v.expiresAt !== "string" ||
        !Number.isFinite(Date.parse(v.expiresAt)))) ||
    (v.csrfToken !== null && typeof v.csrfToken !== "string") ||
    typeof v.loginAvailable !== "boolean"
  )
    throw new Error("Invalid session response");
  return v as unknown as ProductSession;
}
export function createAuthApi(fetcher: typeof fetch): AuthApi {
  const checkedFetch: typeof fetch = async (input, init) => {
    const response = await fetcher(input, init);
    if (response.ok) return response;
    const mediaType = response.headers
      .get("content-type")
      ?.split(";", 1)[0]
      ?.trim();
    const body: unknown = await response
      .clone()
      .json()
      .catch(() => undefined);
    const parsed = ProblemDetails.safeParse(body);
    if (
      mediaType === "application/problem+json" &&
      parsed.success &&
      parsed.data.status === response.status &&
      (response.status !== 401 ||
        response.headers.get("www-authenticate")?.trim())
    ) {
      throw {
        kind: "problem",
        problem: parsed.data,
      } satisfies FrameworkFailure;
    }
    throw {
      kind: "contractViolation",
      message: "Authentication response violated the API contract",
    } satisfies FrameworkFailure;
  };
  return {
    async session() {
      const response = await getProductSession({}, checkedFetch);
      if (response.status !== 200) throw new Error("Session unavailable");
      return session(response.data);
    },
    async logout() {
      const response = await logoutProductSession({}, checkedFetch);
      if (response.status !== 200 || response.data.revoked !== true)
        throw new Error("Logout not confirmed");
      return response.data;
    },
    async exchange(handoff, verifier) {
      const response = await exchangeNativeHandoff(
        { handoff, verifier },
        {},
        checkedFetch,
      );
      if (
        response.status !== 200 ||
        typeof response.data.credential !== "string"
      )
        throw new Error("Invalid handoff response");
      return {
        credential: response.data.credential,
        session: session(response.data.session),
      };
    },
  };
}
