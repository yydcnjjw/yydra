// SPDX-License-Identifier: MIT OR Apache-2.0

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";

import { isFrameworkFailure, useFrameworkClient } from "@/framework/runtime";

const readingQueueKey = ["reading-queue"] as const;

export default function IndexRoute() {
  const client = useFrameworkClient();
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const health = useQuery({
    queryKey: ["workspace-health"],
    queryFn: ({ signal }) => client.health(signal),
  });
  const queue = useQuery({
    queryKey: readingQueueKey,
    queryFn: ({ signal }) => client.listReadingQueueEntries(signal),
  });
  const createEntry = useMutation({
    mutationFn: () => client.createReadingQueueEntry({ title, sourceUrl }),
    async onSuccess() {
      setTitle("");
      setSourceUrl("");
      await queryClient.invalidateQueries({ queryKey: readingQueueKey });
    },
  });
  const changeEntryState = useMutation({
    mutationFn: ({
      id,
      state,
    }: {
      id: string;
      state: "queued" | "completed";
    }) => client.changeReadingQueueEntryState(id, { state }),
    async onSuccess() {
      await queryClient.invalidateQueries({ queryKey: readingQueueKey });
    },
  });

  return (
    <ScrollView contentContainerStyle={styles.container}>
      <Text accessibilityRole="header" style={styles.title}>
        {__PRODUCT_NAME_JSON__}
      </Text>

      <View style={styles.status} accessibilityRole="summary">
        {health.isPending ? <Text>Connecting to Product service…</Text> : null}
        {health.isError ? (
          <View style={styles.status}>
            <Text accessibilityRole="alert">Product service unavailable.</Text>
            <Pressable
              accessibilityRole="button"
              onPress={() => health.refetch()}
            >
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

      <View style={styles.panel}>
        <Text accessibilityRole="header" style={styles.sectionTitle}>
          Add to Reading Queue
        </Text>
        <TextInput
          accessibilityLabel="Entry title"
          onChangeText={setTitle}
          placeholder="Entry title"
          style={styles.input}
          value={title}
        />
        <TextInput
          accessibilityLabel="Source URL"
          autoCapitalize="none"
          inputMode="url"
          onChangeText={setSourceUrl}
          placeholder="https://example.com/article"
          style={styles.input}
          value={sourceUrl}
        />
        <Pressable
          accessibilityRole="button"
          disabled={createEntry.isPending}
          onPress={() => createEntry.mutate()}
          style={styles.button}
        >
          <Text style={styles.buttonText}>
            {createEntry.isPending ? "Adding…" : "Add entry"}
          </Text>
        </Pressable>
        {createEntry.isError ? (
          <Text accessibilityRole="alert">Could not add this entry.</Text>
        ) : null}
      </View>

      <View style={styles.panel}>
        <Text accessibilityRole="header" style={styles.sectionTitle}>
          Reading Queue
        </Text>
        {queue.isPending ? <Text>Loading queue…</Text> : null}
        {queue.isError ? (
          <View style={styles.status}>
            <Text accessibilityRole="alert">Could not load the queue.</Text>
            <Pressable
              accessibilityRole="button"
              onPress={() => queue.refetch()}
            >
              <Text>Retry queue</Text>
            </Pressable>
          </View>
        ) : null}
        {queue.data?.entries.length === 0 ? (
          <Text>The queue is empty.</Text>
        ) : null}
        {changeEntryState.isError ? (
          <View style={styles.status}>
            <Text accessibilityRole="alert">
              {isFrameworkFailure(changeEntryState.error) &&
              changeEntryState.error.kind === "problem" &&
              changeEntryState.error.problem.type ===
                "https://yydra.dev/problems/reading-entry-transition-conflict"
                ? "This entry changed. Refresh the queue and try again."
                : "Could not update this entry."}
            </Text>
            <Pressable
              accessibilityRole="button"
              onPress={() => {
                changeEntryState.reset();
                void queue.refetch();
              }}
            >
              <Text>Refresh queue</Text>
            </Pressable>
          </View>
        ) : null}
        {queue.data?.entries.map((entry) => (
          <View key={entry.id} style={styles.entry}>
            <Text style={styles.entryTitle}>{entry.title}</Text>
            <Text>{entry.sourceUrl}</Text>
            <Text>State: {entry.state}</Text>
            <Pressable
              accessibilityRole="button"
              disabled={changeEntryState.isPending}
              onPress={() =>
                changeEntryState.mutate({
                  id: entry.id,
                  state: entry.state === "queued" ? "completed" : "queued",
                })
              }
              style={styles.button}
            >
              <Text style={styles.buttonText}>
                {entry.state === "queued"
                  ? `Complete ${entry.title}`
                  : `Reopen ${entry.title}`}
              </Text>
            </Pressable>
          </View>
        ))}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    alignItems: "stretch",
    gap: 20,
    marginHorizontal: "auto",
    maxWidth: 720,
    padding: 24,
    width: "100%",
  },
  title: {
    fontSize: 28,
    fontWeight: "700",
    textAlign: "center",
  },
  sectionTitle: {
    fontSize: 20,
    fontWeight: "700",
  },
  status: {
    alignItems: "center",
    gap: 8,
  },
  panel: {
    borderColor: "#cbd5e1",
    borderRadius: 12,
    borderWidth: 1,
    gap: 12,
    padding: 16,
  },
  input: {
    borderColor: "#94a3b8",
    borderRadius: 8,
    borderWidth: 1,
    padding: 12,
  },
  button: {
    alignItems: "center",
    backgroundColor: "#0f172a",
    borderRadius: 8,
    padding: 12,
  },
  buttonText: {
    color: "#ffffff",
    fontWeight: "700",
  },
  entry: {
    borderTopColor: "#e2e8f0",
    borderTopWidth: 1,
    gap: 4,
    paddingTop: 12,
  },
  entryTitle: {
    fontSize: 16,
    fontWeight: "700",
  },
});
