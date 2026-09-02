// SPDX-License-Identifier: MIT OR Apache-2.0

import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useState } from "react";
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";

import {
  isFrameworkFailure,
  readingQueueInfiniteQueryOptions,
  readingQueueQueryKey,
  ReadingQueueSort,
  ReadingQueueStatusFilter,
  useFrameworkClient,
} from "@/framework/runtime";

const readingQueueRootKey = ["reading-queue"] as const;
const readingQueuePageSize = 10;

function firstSearchValue(
  value: string | string[] | undefined,
): string | undefined {
  return Array.isArray(value) ? value[0] : value;
}

function statusFromUrl(
  value: string | string[] | undefined,
): ReadingQueueStatusFilter {
  const status = firstSearchValue(value);
  return status === "queued" || status === "completed" ? status : "all";
}

function sortFromUrl(value: string | string[] | undefined): ReadingQueueSort {
  return firstSearchValue(value) === "newest" ? "newest" : "oldest";
}

function routeParams(
  status: ReadingQueueStatusFilter,
  sort: ReadingQueueSort,
): Record<string, string> {
  return {
    ...(status === "all" ? {} : { status }),
    ...(sort === "oldest" ? {} : { sort }),
  };
}

export default function IndexRoute() {
  const client = useFrameworkClient();
  const queryClient = useQueryClient();
  const router = useRouter();
  const search = useLocalSearchParams<{
    status?: string | string[];
    sort?: string | string[];
  }>();
  const status = statusFromUrl(search.status);
  const sort = sortFromUrl(search.sort);
  const queueKey = readingQueueQueryKey(status, sort, readingQueuePageSize);
  const [title, setTitle] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const health = useQuery({
    queryKey: ["workspace-health"],
    queryFn: ({ signal }) => client.health(signal),
  });
  const queue = useInfiniteQuery(
    readingQueueInfiniteQueryOptions(
      client,
      status,
      sort,
      readingQueuePageSize,
    ),
  );
  const entries = queue.data?.pages.flatMap((page) => page.entries) ?? [];
  const createEntry = useMutation({
    mutationFn: () => client.createReadingQueueEntry({ title, sourceUrl }),
    async onSuccess() {
      setTitle("");
      setSourceUrl("");
      await queryClient.resetQueries({ queryKey: readingQueueRootKey });
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
      await queryClient.resetQueries({ queryKey: readingQueueRootKey });
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
        <View style={styles.filterRow}>
          {(
            [
              ["all", "All entries"],
              ["queued", "Queued entries"],
              ["completed", "Completed entries"],
            ] as const
          ).map(([value, label]) => (
            <Pressable
              accessibilityRole="button"
              accessibilityState={{ selected: status === value }}
              key={value}
              onPress={() =>
                router.replace({
                  pathname: "/",
                  params: routeParams(value, sort),
                })
              }
              style={[
                styles.filterButton,
                status === value && styles.selectedButton,
              ]}
            >
              <Text style={styles.buttonText}>{label}</Text>
            </Pressable>
          ))}
        </View>
        <View style={styles.filterRow}>
          {(
            [
              ["oldest", "Oldest first"],
              ["newest", "Newest first"],
            ] as const
          ).map(([value, label]) => (
            <Pressable
              accessibilityRole="button"
              accessibilityState={{ selected: sort === value }}
              key={value}
              onPress={() =>
                router.replace({
                  pathname: "/",
                  params: routeParams(status, value),
                })
              }
              style={[
                styles.filterButton,
                sort === value && styles.selectedButton,
              ]}
            >
              <Text style={styles.buttonText}>{label}</Text>
            </Pressable>
          ))}
        </View>
        <Pressable
          accessibilityRole="button"
          disabled={queue.isFetching && !queue.isFetchingNextPage}
          onPress={() =>
            void queryClient.resetQueries({ queryKey: queueKey, exact: true })
          }
          style={styles.button}
        >
          <Text style={styles.buttonText}>Refresh from first page</Text>
        </Pressable>
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
        {entries.length === 0 && !queue.isPending && !queue.isError ? (
          <Text>
            {status === "all"
              ? "The queue is empty."
              : "No entries match this filter."}
          </Text>
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
                void queryClient.resetQueries({
                  queryKey: queueKey,
                  exact: true,
                });
              }}
            >
              <Text>Refresh queue</Text>
            </Pressable>
          </View>
        ) : null}
        {entries.map((entry) => (
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
        {queue.hasNextPage ? (
          <Pressable
            accessibilityRole="button"
            disabled={queue.isFetchingNextPage}
            onPress={() => void queue.fetchNextPage({ cancelRefetch: false })}
            style={styles.button}
          >
            <Text style={styles.buttonText}>
              {queue.isFetchingNextPage ? "Loading more…" : "Load more"}
            </Text>
          </Pressable>
        ) : null}
        {entries.length > 0 && !queue.hasNextPage ? (
          <Text>End of queue.</Text>
        ) : null}
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
  filterButton: {
    backgroundColor: "#475569",
    borderRadius: 8,
    flexGrow: 1,
    padding: 10,
  },
  filterRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
  },
  selectedButton: {
    backgroundColor: "#0369a1",
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
