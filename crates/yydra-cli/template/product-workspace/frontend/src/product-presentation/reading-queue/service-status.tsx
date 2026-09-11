// SPDX-License-Identifier: MIT OR Apache-2.0

import { useQuery } from "@tanstack/react-query";
import { Pressable, Text, View } from "react-native";
import { isFrameworkFailure, useFrameworkClient } from "@/framework/runtime";
import { failureMessage } from "./failure-messages";
import { styles } from "./styles";

export function ServiceStatus() {
  const client = useFrameworkClient();
  const health = useQuery({
    queryKey: ["workspace-health"],
    queryFn: ({ signal }) => client.health(signal),
  });
  return (
    <View style={styles.status} accessibilityRole="summary">
      {health.isPending ? <Text>Connecting to Product service…</Text> : null}
      {health.isError ? (
        <HealthFailure error={health.error} retry={health.refetch} />
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

function HealthFailure({ error, retry }: { error: unknown; retry(): unknown }) {
  const cancelled = isFrameworkFailure(error) && error.kind === "cancelled";
  return (
    <View style={styles.status}>
      <Text
        accessibilityRole={cancelled ? undefined : "alert"}
        role={cancelled ? "status" : undefined}
      >
        {cancelled
          ? "Product service connection was cancelled."
          : failureMessage(error, "Product service unavailable.")}
      </Text>
      <Pressable accessibilityRole="button" onPress={() => void retry()}>
        <Text>Retry</Text>
      </Pressable>
    </View>
  );
}
