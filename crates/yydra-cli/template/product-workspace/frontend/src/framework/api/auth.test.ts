// SPDX-License-Identifier: MIT OR Apache-2.0
import { describe, expect, it } from "vitest";
import { createAuthApi } from "./auth";

describe("authentication problems", () => {
  it("preserves the safe problem identity, request ID, and structured violations", async () => {
    const problem = {
      type: "https://yydra.dev/problems/invalid-auth-request",
      title: "Invalid authentication request",
      status: 400,
      requestId: "server-request",
      violations: [{ field: "handoff", code: "invalid" }],
    };
    const api = createAuthApi(async () =>
      Response.json(problem, {
        status: 400,
        headers: { "content-type": "application/problem+json" },
      }),
    );
    await expect(api.exchange("opaque", "verifier")).rejects.toEqual({
      kind: "problem",
      problem,
    });
  });
  it("rejects an error response without request correlation", async () => {
    const api = createAuthApi(async () =>
      Response.json(
        {
          type: "https://yydra.dev/problems/internal",
          title: "Internal failure",
          status: 500,
        },
        {
          status: 500,
          headers: { "content-type": "application/problem+json" },
        },
      ),
    );
    await expect(api.session()).rejects.toMatchObject({
      kind: "contractViolation",
    });
  });
});
