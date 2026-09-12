// SPDX-License-Identifier: MIT OR Apache-2.0
import {
  AuthController,
  createAuthPlatform,
  type AuthSnapshot,
} from "@yydra/auth";
import {
  createContext,
  PropsWithChildren,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
} from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { createAuthApi } from "@/framework/api/auth";
import {
  createFrameworkClient,
  createProductionFrameworkRuntime,
  FrameworkRuntime,
} from "@/framework/runtime";

const AuthContext = createContext<{
  auth: AuthController;
  state: AuthSnapshot;
} | null>(null);

export function ProductAuthentication({ children }: PropsWithChildren) {
  const [auth] = useState(() => {
    const baseUrl = process.env.EXPO_PUBLIC_API_URL ?? "http://127.0.0.1:4000";
    return new AuthController({
      baseUrl,
      callback: "__PRODUCT_ID__://auth/callback",
      platform: createAuthPlatform(baseUrl),
      api: createAuthApi,
    });
  });
  const state = useSyncExternalStore(
    auth.subscribe,
    auth.getSnapshot,
    auth.getSnapshot,
  );
  useEffect(() => {
    void auth.initialize().catch(() => auth.refresh());
    return () => auth.dispose();
  }, [auth]);
  return (
    <AuthContext.Provider value={{ auth, state }}>
      <AuthenticatedRuntime key={state.revision} auth={auth}>
        {children}
      </AuthenticatedRuntime>
    </AuthContext.Provider>
  );
}

export function ProductAuthGate({ children }: PropsWithChildren) {
  const context = useContext(AuthContext);
  if (!context)
    throw new Error("ProductAuthGate requires ProductAuthentication");
  const { auth, state } = context;
  return (
    <View style={styles.root}>
      {state.error && <Text role="alert">{state.error}</Text>}
      {state.status === "authenticated" ? (
        <>
          <View style={styles.bar}>
            <Text>Signed in with GitHub</Text>
            <Pressable
              accessibilityRole="button"
              onPress={() => void auth.logout()}
              style={styles.button}
            >
              <Text style={styles.buttonText}>Sign out</Text>
            </Pressable>
          </View>
          {children}
        </>
      ) : (
        <View style={styles.login}>
          <Text accessibilityRole="header" style={styles.heading}>
            Your reading queue
          </Text>
          <Text>
            Sign in to keep your reading list private and available across
            devices.
          </Text>
          {state.status === "loading" ? (
            <Text>Restoring your session…</Text>
          ) : (
            <>
              {state.loginAvailable ? (
                <Pressable
                  accessibilityRole="button"
                  style={styles.button}
                  onPress={() => void auth.login()}
                >
                  <Text style={styles.buttonText}>Continue with GitHub</Text>
                </Pressable>
              ) : (
                <Text>Sign-in is currently unavailable.</Text>
              )}
              <Pressable
                accessibilityRole="button"
                onPress={() => void auth.refresh()}
              >
                <Text>Try again</Text>
              </Pressable>
            </>
          )}
        </View>
      )}
    </View>
  );
}
function AuthenticatedRuntime({
  auth,
  children,
}: PropsWithChildren<{ auth: AuthController }>) {
  const [runtime] = useState(() =>
    createProductionFrameworkRuntime({
      client: createFrameworkClient(auth.sessionFetch()),
    }),
  );
  useEffect(
    () => () => {
      void runtime.queryClient.cancelQueries();
      runtime.queryClient.clear();
    },
    [runtime],
  );
  return <FrameworkRuntime runtime={runtime}>{children}</FrameworkRuntime>;
}
const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: "#f8fafc" },
  bar: {
    padding: 16,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 12,
  },
  login: {
    width: "100%",
    maxWidth: 560,
    marginHorizontal: "auto",
    padding: 24,
    gap: 20,
  },
  heading: { fontSize: 28, fontWeight: "700", color: "#0f172a" },
  button: { backgroundColor: "#0f172a", borderRadius: 8, padding: 12 },
  buttonText: { color: "#fff", fontWeight: "600" },
});
