// SPDX-License-Identifier: MIT OR Apache-2.0
export interface ProductSession {
  accountId: string | null;
  expiresAt: string | null;
  csrfToken: string | null;
  loginAvailable: boolean;
}
export interface AuthApi {
  session(): Promise<ProductSession>;
  logout(): Promise<{ revoked: boolean }>;
  exchange(
    handoff: string,
    verifier: string,
  ): Promise<{
    credential: string;
    session: ProductSession;
  }>;
}
export interface AuthPlatform {
  native: boolean;
  loadCredential(): Promise<string | null>;
  saveCredential(value: string | null): Promise<void>;
  begin(startUrl: string, callback: string): Promise<string | null>;
  redeem(
    url: string,
    callback: string,
    exchange: AuthApi["exchange"],
  ): Promise<{
    credential: string;
    session: ProductSession;
  }>;
  initialUrl(): Promise<string | null>;
}
export interface AuthSnapshot {
  status: "loading" | "authenticated" | "anonymous" | "unavailable";
  accountId: string | null;
  expiresAt: string | null;
  loginAvailable: boolean;
  revision: number;
  error: string | null;
}

export class AuthController {
  private credential: string | null = null;
  private csrf: string | null = null;
  private listeners = new Set<() => void>();
  private pending = new Set<AbortController>();
  private expiryTimer: ReturnType<typeof setTimeout> | undefined;
  private refreshSequence = 0;
  private action: Promise<void> | null = null;
  private snapshot: AuthSnapshot = {
    status: "loading",
    accountId: null,
    expiresAt: null,
    loginAvailable: false,
    revision: 0,
    error: null,
  };
  private api: AuthApi;
  constructor(
    private options: {
      baseUrl: string;
      callback: string;
      platform: AuthPlatform;
      api(fetcher: typeof fetch): AuthApi;
      fetcher?: typeof fetch;
    },
  ) {
    this.api = options.api(this.authorizedFetch);
  }

  getSnapshot = () => this.snapshot;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private notify() {
    this.listeners.forEach((listener) => listener());
  }
  private invalidate() {
    this.refreshSequence += 1;
    this.pending.forEach((request) => request.abort());
    this.pending.clear();
    clearTimeout(this.expiryTimer);
    this.snapshot = { ...this.snapshot, revision: this.snapshot.revision + 1 };
  }
  private setSession(session: ProductSession) {
    this.invalidate();
    this.csrf = session.csrfToken;
    this.snapshot = {
      status: session.accountId === null ? "anonymous" : "authenticated",
      accountId: session.accountId,
      expiresAt: session.expiresAt,
      loginAvailable: session.loginAvailable,
      revision: this.snapshot.revision,
      error: null,
    };
    if (session.expiresAt && session.accountId) {
      const remaining = Date.parse(session.expiresAt) - Date.now();
      if (!Number.isFinite(remaining) || remaining <= 0) {
        this.snapshot = {
          ...this.snapshot,
          status: "anonymous",
          accountId: null,
          expiresAt: null,
        };
      } else {
        this.expiryTimer = setTimeout(
          () => {
            void this.refresh();
          },
          Math.min(remaining, 2_147_483_647),
        );
      }
    }
    this.notify();
  }
  sessionFetch(): typeof fetch {
    const revision = this.snapshot.revision;
    return async (input, init) => {
      if (revision !== this.snapshot.revision)
        throw new Error("Session changed");
      const response = await this.authorizedFetch(input, init);
      if (revision !== this.snapshot.revision)
        throw new Error("Session changed");
      return response;
    };
  }
  // Buffer the complete response while its cancellation/generation guard is alive.
  // This also protects a late response body, not only the initial headers.
  authorizedFetch: typeof fetch = async (input, init) => {
    const resolved = new URL(
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url,
      this.options.baseUrl,
    );
    if (resolved.origin !== new URL(this.options.baseUrl).origin)
      throw new Error(
        "Authentication credentials require the product API origin",
      );
    const revision = this.snapshot.revision;
    const controller = new AbortController();
    const abort = () => controller.abort(init?.signal?.reason);
    init?.signal?.addEventListener("abort", abort, { once: true });
    if (init?.signal?.aborted) abort();
    this.pending.add(controller);
    const timeout = setTimeout(() => controller.abort(), 15_000);
    const headers = new Headers(init?.headers);
    const url = resolved;

    if (this.credential)
      headers.set("Authorization", `Bearer ${this.credential}`);
    if (this.csrf) headers.set("x-yydra-csrf", this.csrf);
    try {
      const response = await (this.options.fetcher ?? globalThis.fetch)(url, {
        ...init,
        headers,
        signal: controller.signal,
        credentials: this.options.platform.native ? "omit" : "include",
      });
      // Preserve Blob decoding on React Native: its fetch polyfill decodes a
      // reconstructed ArrayBuffer response as single-byte text, corrupting UTF-8.
      const body = await response.blob();
      if (controller.signal.aborted || revision !== this.snapshot.revision)
        throw new Error("Session changed during request");
      if (response.status === 401) {
        this.credential = null;
        await this.options.platform.saveCredential(null);
        if (revision !== this.snapshot.revision)
          throw new Error("Session changed during credential removal");
        this.setSession({
          accountId: null,
          expiresAt: null,
          csrfToken: null,
          loginAvailable: this.snapshot.loginAvailable,
        });
      }
      return new Response(
        [204, 205, 304].includes(response.status) ? null : body,
        {
          status: response.status,
          statusText: response.statusText,
          headers: response.headers,
        },
      );
    } finally {
      clearTimeout(timeout);
      this.pending.delete(controller);
      init?.signal?.removeEventListener("abort", abort);
    }
  };
  refresh = async () => {
    if (this.action) return;
    const operation = ++this.refreshSequence;
    const revision = this.snapshot.revision;
    const current = () =>
      operation === this.refreshSequence && revision === this.snapshot.revision;
    try {
      const credential = await this.options.platform.loadCredential();
      if (!current()) return;
      this.credential = credential;
      const session = await this.api.session();
      if (!current()) return;
      if (session.accountId === null) {
        await this.options.platform.saveCredential(null);
        if (!current()) return;
        this.credential = null;
      }
      this.setSession(session);
    } catch {
      if (!current()) return;
      this.invalidate();
      this.snapshot = {
        ...this.snapshot,
        status: "unavailable",
        accountId: null,
        error: "Cannot reach the sign-in service. Please try again.",
      };
      this.notify();
    }
  };
  initialize = async () => {
    const initial = await this.options.platform.initialUrl();
    if (initial && this.options.platform.native && this.isCallback(initial)) {
      await this.refresh();
      await this.runAction(() => this.complete(initial));
    } else if (initial && new URL(initial).searchParams.has("authError")) {
      await this.refresh();
      this.snapshot = {
        ...this.snapshot,
        error: "Sign-in was not completed. Please try again.",
      };
      this.notify();
    } else await this.refresh();
  };
  private isCallback(url: string) {
    try {
      const actual = new URL(url),
        expected = new URL(this.options.callback);
      return (
        actual.protocol === expected.protocol &&
        actual.host === expected.host &&
        actual.pathname === expected.pathname
      );
    } catch {
      return false;
    }
  }
  private async complete(url: string) {
    const result = await this.options.platform.redeem(
      url,
      this.options.callback,
      this.api.exchange,
    );
    await this.options.platform.saveCredential(result.credential);
    this.credential = result.credential;
    this.setSession(result.session);
  }
  login = () =>
    this.runAction(async () => {
      const url = await this.options.platform.begin(
        new URL("/auth/github", this.options.baseUrl).href,
        this.options.callback,
      );
      if (url !== null) await this.complete(url);
    });
  logout = () =>
    this.runAction(async () => {
      const result = await this.api.logout();
      if (!result.revoked) throw new Error("Revocation was not confirmed");
      await this.options.platform.saveCredential(null);
      this.credential = null;
      this.setSession({
        accountId: null,
        expiresAt: null,
        csrfToken: null,
        loginAvailable: this.snapshot.loginAvailable,
      });
    });
  private runAction(operation: () => Promise<void>): Promise<void> {
    if (this.action) return this.action;
    this.refreshSequence += 1;
    this.action = operation()
      .catch(() => {
        this.snapshot = {
          ...this.snapshot,
          error:
            "The operation did not complete. Please retry. If sign-out failed, the server session may still be valid.",
        };
        this.notify();
      })
      .finally(() => {
        this.action = null;
      });
    return this.action;
  }
  dispose() {
    this.invalidate();
    this.listeners.clear();
  }
}
