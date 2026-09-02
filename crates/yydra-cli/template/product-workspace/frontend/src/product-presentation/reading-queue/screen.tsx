// SPDX-License-Identifier: MIT OR Apache-2.0

import {
  InfiniteData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useState } from "react";
import {
  Linking,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";

import {
  FrameworkClient,
  isFrameworkFailure,
  useFrameworkClient,
} from "@/framework/runtime";

import {
  readingQueueInfiniteQueryOptions,
  readingQueueQueryKey,
  readingQueueRootKey,
  ReadingQueueSort,
  ReadingQueueStatusFilter,
} from "./queries";

const readingQueuePageSize = 10;

type ReadingQueuePage = Awaited<
  ReturnType<FrameworkClient["listReadingQueueEntries"]>
>;

export interface ReadingQueueScreenProps {
  onRouteStateChange(
    status: ReadingQueueStatusFilter,
    sort: ReadingQueueSort,
  ): void;
  productName: string;
  sort: ReadingQueueSort;
  status: ReadingQueueStatusFilter;
}

export function ReadingQueueScreen({
  onRouteStateChange,
  productName,
  sort,
  status,
}: ReadingQueueScreenProps) {
  const client = useFrameworkClient();
  const queryClient = useQueryClient();
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
    mutationKey: ["reading-queue-create"],
    mutationFn: (input: { sourceUrl: string; title: string }) =>
      client.createReadingQueueEntry(input),
    async onSuccess() {
      setTitle("");
      setSourceUrl("");
      await queryClient.resetQueries({ queryKey: readingQueueRootKey });
    },
  });
  const changeEntryState = useMutation({
    mutationKey: ["reading-queue-change-state"],
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
  const hasQueueData = queue.data !== undefined;
  const queueFailure = queue.isError
    ? queueFailurePresentation(queue.error, hasQueueData)
    : undefined;

  const refreshFromFirstPage = () => {
    queryClient.setQueryData<InfiniteData<ReadingQueuePage>>(
      queueKey,
      (data) =>
        data === undefined
          ? undefined
          : {
              pageParams: data.pageParams.slice(0, 1),
              pages: data.pages.slice(0, 1),
            },
    );
    void queue.refetch();
  };

  return (
    <ScrollView contentContainerStyle={styles.container} role="main">
      <Text accessibilityRole="header" style={styles.title}>
        {productName}
      </Text>

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
          accessibilityState={{ disabled: createEntry.isPending }}
          disabled={createEntry.isPending}
          onPress={() => createEntry.mutate({ sourceUrl, title })}
          style={styles.button}
        >
          <Text style={styles.buttonText}>
            {createEntry.isPending ? "Adding…" : "Add entry"}
          </Text>
        </Pressable>
        {createEntry.isError ? (
          <Text accessibilityRole="alert">
            {mutationFailureMessage(createEntry.error, "create")}
          </Text>
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
              aria-selected={status === value}
              key={value}
              onPress={() => onRouteStateChange(value, sort)}
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
              aria-selected={sort === value}
              key={value}
              onPress={() => onRouteStateChange(status, value)}
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
          accessibilityState={{
            disabled: queue.isFetching && !queue.isFetchingNextPage,
          }}
          disabled={queue.isFetching && !queue.isFetchingNextPage}
          onPress={refreshFromFirstPage}
          style={styles.button}
        >
          <Text style={styles.buttonText}>Refresh from first page</Text>
        </Pressable>
        {queue.isPending ? <Text>Loading queue…</Text> : null}
        {queue.isFetching && hasQueueData && !queue.isFetchingNextPage ? (
          <Text role="status">Refreshing queue…</Text>
        ) : null}
        {queueFailure !== undefined ? (
          <View style={styles.status}>
            <Text
              accessibilityRole={queueFailure.alert ? "alert" : undefined}
              role={queueFailure.alert ? undefined : "status"}
            >
              {queueFailure.message}
            </Text>
            <Pressable
              accessibilityRole="button"
              onPress={() => void queue.refetch()}
            >
              <Text>Retry queue</Text>
            </Pressable>
          </View>
        ) : null}
        {entries.length === 0 && queue.isSuccess ? (
          <Text>
            {status === "all"
              ? "The queue is empty."
              : "No entries match this filter."}
          </Text>
        ) : null}
        {changeEntryState.isError ? (
          <View style={styles.status}>
            <Text accessibilityRole="alert">
              {mutationFailureMessage(changeEntryState.error, "transition")}
            </Text>
            <Pressable
              accessibilityRole="button"
              onPress={() => {
                changeEntryState.reset();
                refreshFromFirstPage();
              }}
            >
              <Text>Refresh queue</Text>
            </Pressable>
          </View>
        ) : null}
        <View accessibilityRole="list">
          {entries.map((entry) => (
            <View
              accessibilityLabel={`Reading entry ${entry.title}`}
              key={entry.id}
              role="listitem"
              style={styles.entry}
            >
              <Text accessibilityRole="header" style={styles.entryTitle}>
                {entry.title}
              </Text>
              <Pressable
                accessibilityLabel={entry.sourceUrl}
                accessibilityRole="link"
                onPress={() => void Linking.openURL(entry.sourceUrl)}
              >
                <Text style={styles.link}>{entry.sourceUrl}</Text>
              </Pressable>
              <Text>State: {entry.state}</Text>
              <Pressable
                accessibilityRole="button"
                accessibilityState={{ disabled: changeEntryState.isPending }}
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
        {queue.hasNextPage ? (
          <Pressable
            accessibilityRole="button"
            accessibilityState={{ disabled: queue.isFetchingNextPage }}
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

function queueFailurePresentation(
  error: unknown,
  background: boolean,
): { alert: boolean; message: string } {
  if (isFrameworkFailure(error) && error.kind === "cancelled") {
    return { alert: false, message: "Queue loading was cancelled." };
  }
  const detail = failureMessage(
    error,
    "The Product service could not load this queue.",
  );
  return {
    alert: true,
    message: background
      ? `Queue refresh failed. Saved results remain visible. ${detail}`
      : detail,
  };
}

function failureMessage(error: unknown, problemFallback: string): string {
  if (!isFrameworkFailure(error)) {
    return "Something went wrong. Try again safely.";
  }
  switch (error.kind) {
    case "problem":
      return problemFallback;
    case "transport":
      return "Cannot reach the Product service.";
    case "contractViolation":
      return "The Product service returned data this app cannot safely display.";
    case "cancelled":
      return "The request was cancelled.";
  }
}

function mutationFailureMessage(
  error: unknown,
  operation: "create" | "transition",
): string {
  if (isFrameworkFailure(error) && error.kind === "problem") {
    if (
      error.problem.type === "https://yydra.dev/problems/invalid-reading-entry"
    ) {
      return "Enter a title and a valid source URL.";
    }
    if (
      error.problem.type ===
      "https://yydra.dev/problems/reading-entry-transition-conflict"
    ) {
      return "This entry changed. Refresh the queue and try again.";
    }
    return operation === "create"
      ? "The Product service rejected this entry."
      : "The Product service rejected this state change.";
  }
  return failureMessage(
    error,
    operation === "create"
      ? "Could not add this entry."
      : "Could not update this entry.",
  );
}

const styles = StyleSheet.create({
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
  container: {
    alignItems: "stretch",
    alignSelf: "center",
    gap: 20,
    maxWidth: 720,
    minHeight: "100%",
    padding: 24,
    width: "100%",
  },
  entry: {
    borderTopColor: "#e2e8f0",
    borderTopWidth: 1,
    gap: 4,
    minWidth: 0,
    paddingTop: 12,
  },
  entryTitle: {
    fontSize: 16,
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
  input: {
    borderColor: "#94a3b8",
    borderRadius: 8,
    borderWidth: 1,
    padding: 12,
  },
  link: {
    color: "#0369a1",
    textDecorationLine: "underline",
  },
  panel: {
    borderColor: "#cbd5e1",
    borderRadius: 12,
    borderWidth: 1,
    gap: 12,
    minWidth: 0,
    padding: 16,
  },
  sectionTitle: {
    fontSize: 20,
    fontWeight: "700",
  },
  selectedButton: {
    backgroundColor: "#0369a1",
  },
  status: {
    alignItems: "center",
    gap: 8,
  },
  title: {
    fontSize: 28,
    fontWeight: "700",
    textAlign: "center",
  },
});
