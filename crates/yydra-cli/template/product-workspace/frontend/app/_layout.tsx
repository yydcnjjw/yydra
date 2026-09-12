// SPDX-License-Identifier: MIT OR Apache-2.0

import { Stack } from "expo-router";

import { ProductAuthentication } from "@/product-presentation/auth/runtime";

export default function RootLayout() {
  return (
    <ProductAuthentication>
      <Stack screenOptions={{ headerShown: false }} />
    </ProductAuthentication>
  );
}
