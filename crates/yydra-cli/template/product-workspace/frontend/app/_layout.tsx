// SPDX-License-Identifier: MIT OR Apache-2.0

import { Stack } from "expo-router";
import { useEffect } from "react";
import { Text } from "react-native";
import { useHydrated } from "@yydra/client-settings/react";

import { ProductAuthentication } from "@/product-presentation/auth/runtime";
import { clientSettings } from "@/product-presentation/settings";

export default function RootLayout() {
  const hydrated = useHydrated(clientSettings);
  useEffect(() => {
    if (!clientSettings.persist.hasHydrated()) {
      void clientSettings.persist.rehydrate();
    }
  }, []);
  if (!hydrated) {
    return <Text>Loading settings…</Text>;
  }
  return (
    <ProductAuthentication>
      <Stack screenOptions={{ headerShown: false }} />
    </ProductAuthentication>
  );
}
