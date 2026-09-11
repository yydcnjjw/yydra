// SPDX-License-Identifier: MIT OR Apache-2.0
// @vitest-environment jsdom
import { createRequire } from "node:module";
import { describe, expect, it, vi } from "vitest";
import {
  AuthController,
  type AuthApi,
  type AuthPlatform,
  type ProductSession,
} from "@yydra/auth-client";

const account = (id: string): ProductSession => ({
  accountId: id,
  expiresAt: new Date(Date.now() + 60_000).toISOString(),
  csrfToken: "csrf-test",
  loginAvailable: true,
});
function fixture(fetcher: typeof fetch = vi.fn<typeof fetch>()) {
  let current = account("first");
  const saveCredential = vi.fn<(_: string | null) => Promise<void>>(
    async () => undefined,
  );
  const api: AuthApi = {
    session: async () => current,
    logout: vi.fn(async () => ({ revoked: true })),
    exchange: vi.fn(async () => ({
      credential: "new-native-credential",
      session: current,
    })),
  };
  const platform: AuthPlatform = {
    native: false,
    loadCredential: async () => null,
    saveCredential,
    begin: async () => null,
    redeem: async () => ({ credential: "", session: current }),
    initialUrl: async () => null,
  };
  const auth = new AuthController({
    baseUrl: "https://api.example.test",
    callback: "test://auth/callback",
    platform,
    api: () => api,
    fetcher,
  });
  return {
    auth,
    api,
    saveCredential,
    changeAccount: (id: string) => {
      current = account(id);
    },
  };
}
describe("product authentication lifecycle", () => {
  it("preserves Unicode through the React Native fetch polyfill", async () => {
    const { Response: NativeResponse } = createRequire(import.meta.url)(
      "whatwg-fetch",
    ) as { Response: typeof Response };
    const payload = { title: "认证后的中文阅读条目 📚" };
    vi.stubGlobal("Response", NativeResponse);
    const f = fixture(
      vi.fn<typeof fetch>(
        async () => new NativeResponse(JSON.stringify(payload)),
      ),
    );
    try {
      await f.auth.initialize();
      const response = await f.auth.authorizedFetch("/private");
      expect(await response.json()).toEqual(payload);
    } finally {
      f.auth.dispose();
      vi.unstubAllGlobals();
    }
  });
  it("does not report server revocation or discard credentials after failed logout", async () => {
    const f = fixture();
    await f.auth.initialize();
    vi.mocked(f.api.logout).mockRejectedValue(new Error("offline"));
    await f.auth.logout();
    expect(f.auth.getSnapshot().status).toBe("authenticated");
    expect(f.auth.getSnapshot().error).toContain(
      "server session may still be valid",
    );
    expect(f.saveCredential).not.toHaveBeenCalled();
    f.auth.dispose();
  });
  it("invalidates an old client's credentials when the account changes", async () => {
    const fetcher = vi.fn<typeof fetch>(async () => new Response("{}"));
    const f = fixture(fetcher);
    await f.auth.initialize();
    const oldClient = f.auth.sessionFetch();
    f.changeAccount("second");
    await f.auth.refresh();
    await expect(oldClient("/private")).rejects.toThrow("Session changed");
    expect(fetcher).not.toHaveBeenCalled();
    f.auth.dispose();
  });
  it("rejects a late response body after logout", async () => {
    let complete!: (value: Uint8Array) => void;
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        complete = (bytes) => {
          controller.enqueue(bytes);
          controller.close();
        };
      },
    });
    const f = fixture(vi.fn<typeof fetch>(async () => new Response(body)));
    await f.auth.initialize();
    const pending = f.auth.authorizedFetch("/private");
    const rejected = expect(pending).rejects.toThrow("Session changed");
    await f.auth.logout();
    complete(new TextEncoder().encode('{"account":"first"}'));
    await rejected;
    expect(f.auth.getSnapshot().status).toBe("anonymous");
    f.auth.dispose();
  });
  it("never sends credentials to another API origin", async () => {
    const fetcher = vi.fn<typeof fetch>();
    const f = fixture(fetcher);
    await f.auth.initialize();
    await expect(
      f.auth.authorizedFetch("https://untrusted.test/"),
    ).rejects.toThrow("product API origin");
    expect(fetcher).not.toHaveBeenCalled();
    f.auth.dispose();
  });
});
