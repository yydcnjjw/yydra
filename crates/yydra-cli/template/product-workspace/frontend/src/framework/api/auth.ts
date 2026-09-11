// SPDX-License-Identifier: MIT OR Apache-2.0
import {
  getProductSession,
  logoutProductSession,
  exchangeNativeHandoff,
} from "@yydra/generated-api/fetch/client";
import type { AuthApi, ProductSession } from "@yydra/auth-client";

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
  return {
    async session() {
      const response = await getProductSession({}, fetcher);
      if (response.status !== 200) throw new Error("Session unavailable");
      return session(response.data);
    },
    async logout() {
      const response = await logoutProductSession({}, fetcher);
      if (response.status !== 200 || response.data.revoked !== true)
        throw new Error("Logout not confirmed");
      return response.data;
    },
    async exchange(handoff, verifier) {
      const response = await exchangeNativeHandoff(
        { handoff, verifier },
        {},
        fetcher,
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
