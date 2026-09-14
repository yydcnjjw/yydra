// SPDX-License-Identifier: MIT OR Apache-2.0
import type { AuthPlatform } from "./controller";
export function createAuthPlatform(baseUrl: string): AuthPlatform {
  new URL(baseUrl);
  return {
    native: false,
    loadCredential: async () => null,
    saveCredential: async () => undefined,
    begin: async (url) => {
      window.location.assign(url);
      return null;
    },
    redeem: async () => {
      throw new Error("Native callback unavailable on web");
    },
    initialUrl: async () => {
      if (typeof window === "undefined") return null;
      const url = new URL(window.location.href);
      const initial = url.href;
      if (url.searchParams.has("authError")) {
        url.searchParams.delete("authError");
        window.history.replaceState(null, "", url.href);
      }
      return initial;
    },
  };
}
