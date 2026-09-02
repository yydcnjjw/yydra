// SPDX-License-Identifier: MIT OR Apache-2.0

import { Stack } from "expo-router";

import { FrameworkRuntime } from "@/framework/runtime";

export default function RootLayout() {
  return (
    <FrameworkRuntime>
      <Stack screenOptions={{ headerShown: false }} />
    </FrameworkRuntime>
  );
}
