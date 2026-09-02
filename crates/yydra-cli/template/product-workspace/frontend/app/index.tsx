// SPDX-License-Identifier: MIT OR Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { useFrameworkClient } from '@/framework/runtime';

export default function IndexRoute() {
  const client = useFrameworkClient();
  const health = useQuery({
    queryKey: ['workspace-health'],
    queryFn: ({ signal }) => client.health(signal),
  });

  return (
    <View style={styles.container} accessibilityRole="summary">
      <Text accessibilityRole="header" style={styles.title}>
        {__PRODUCT_NAME_JSON__}
      </Text>
      {health.isPending ? <Text>Connecting to Product service…</Text> : null}
      {health.isError ? (
        <View style={styles.status}>
          <Text accessibilityRole="alert">Product service unavailable.</Text>
          <Pressable accessibilityRole="button" onPress={() => health.refetch()}>
            <Text>Retry</Text>
          </Pressable>
        </View>
      ) : null}
      {health.data ? (
        <View style={styles.status}>
          <Text>Backend {health.data.status}.</Text>
          <Text>PostgreSQL schema: {health.data.database}</Text>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 12,
    padding: 24,
  },
  title: {
    fontSize: 28,
    fontWeight: '700',
  },
  status: {
    alignItems: 'center',
    gap: 8,
  },
});
