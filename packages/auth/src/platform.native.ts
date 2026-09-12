// SPDX-License-Identifier: MIT OR Apache-2.0
import * as Crypto from "expo-crypto";
import * as Linking from "expo-linking";
import * as SecureStore from "expo-secure-store";
import * as WebBrowser from "expo-web-browser";
import type { AuthPlatform } from "./controller";

function base64url(bytes: Uint8Array): string {
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  let result = "",
    bits = 0,
    value = 0;
  for (const byte of bytes) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 6) {
      bits -= 6;
      result += chars[(value >>> bits) & 63];
    }
  }
  if (bits > 0) result += chars[(value << (6 - bits)) & 63];
  return result;
}
export function createAuthPlatform(baseUrl: string): AuthPlatform {
  // Separate credentials when an installed app is pointed at another product API.
  const namespace = encodeURIComponent(new URL(baseUrl).origin).replace(
    /%/g,
    "_",
  );
  const credentialKey = `yydra.${namespace}.credential`;
  const pendingKey = `yydra.${namespace}.login`;
  const secureOptions = {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  };
  return {
    native: true,
    loadCredential: () => SecureStore.getItemAsync(credentialKey),
    saveCredential: (value) =>
      value === null
        ? SecureStore.deleteItemAsync(credentialKey)
        : SecureStore.setItemAsync(credentialKey, value, secureOptions),
    async begin(startUrl, callback) {
      const verifier = base64url(Crypto.getRandomBytes(32));
      const challenge = (
        await Crypto.digestStringAsync(
          Crypto.CryptoDigestAlgorithm.SHA256,
          verifier,
          { encoding: Crypto.CryptoEncoding.BASE64 },
        )
      )
        .replace(/\+/g, "-")
        .replace(/\//g, "_")
        .replace(/=+$/, "");
      await SecureStore.setItemAsync(
        pendingKey,
        JSON.stringify({ verifier, expires: Date.now() + 10 * 60_000 }),
        secureOptions,
      );
      const url = new URL(startUrl);
      url.searchParams.set("native_challenge", challenge);
      const result = await WebBrowser.openAuthSessionAsync(url.href, callback);
      if (result.type === "success") return result.url;
      await SecureStore.deleteItemAsync(pendingKey);
      return null;
    },
    async redeem(url, callback, exchange) {
      const actual = new URL(url),
        expected = new URL(callback);
      if (
        actual.protocol !== expected.protocol ||
        actual.host !== expected.host ||
        actual.pathname !== expected.pathname ||
        actual.hash
      ) {
        throw new Error("Unexpected authentication return");
      }
      if (actual.searchParams.has("authError")) {
        await SecureStore.deleteItemAsync(pendingKey);
        throw new Error("Sign-in was not completed");
      }
      const handoff = actual.searchParams.get("handoff");
      const pending = await SecureStore.getItemAsync(pendingKey);
      if (!handoff || !pending) throw new Error("No pending login");
      const value: unknown = JSON.parse(pending);
      if (
        typeof value !== "object" ||
        value === null ||
        !("verifier" in value) ||
        typeof value.verifier !== "string" ||
        !("expires" in value) ||
        typeof value.expires !== "number" ||
        value.expires <= Date.now()
      ) {
        await SecureStore.deleteItemAsync(pendingKey);
        throw new Error("Login expired");
      }
      const result = await exchange(handoff, value.verifier);
      await SecureStore.deleteItemAsync(pendingKey);
      return result;
    },
    initialUrl: () => Linking.getInitialURL(),
  };
}
