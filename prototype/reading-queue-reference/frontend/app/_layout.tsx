import { Stack } from 'expo-router';

import { FrameworkRuntime } from '@/framework/runtime';

export default function RootLayout() {
  return (
    <FrameworkRuntime>
      <Stack screenOptions={{ headerShown: false }} />
    </FrameworkRuntime>
  );
}
