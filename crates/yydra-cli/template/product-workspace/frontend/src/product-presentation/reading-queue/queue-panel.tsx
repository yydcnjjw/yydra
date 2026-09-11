// SPDX-License-Identifier: MIT OR Apache-2.0

import { Linking, Pressable, Text, View } from "react-native";
import {
  mutationFailureMessage,
  queueFailurePresentation,
} from "./failure-messages";
import type { ReadingQueueScreenProps } from "./screen";
import type { useReadingQueue } from "./use-reading-queue";
import { styles } from "./styles";

export function ReadingQueuePanel({
  queue,
  onRouteStateChange,
  sort,
  status,
}: Omit<ReadingQueueScreenProps, "productName"> & {
  queue: ReturnType<typeof useReadingQueue>;
}) {
  const queueFailure =
    queue.error === null
      ? undefined
      : queueFailurePresentation(queue.error, queue.hasData);
  if (queueFailure && queue.refreshFailedAfterSave) {
    queueFailure.message =
      "Entry saved. Queue refresh failed. Saved results may be out of date. Refresh the queue to see the latest changes.";
  }
  return (
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
          disabled: queue.isRefreshing,
        }}
        disabled={queue.isRefreshing}
        onPress={queue.refresh}
        style={styles.button}
      >
        <Text style={styles.buttonText}>Refresh from first page</Text>
      </Pressable>
      {queue.isPending ? <Text>Loading queue…</Text> : null}
      {queue.isRefreshing && queue.hasData ? (
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
            onPress={() => void queue.refresh()}
          >
            <Text>Retry queue</Text>
          </Pressable>
        </View>
      ) : null}
      {queue.entries.length === 0 && queue.isSuccess ? (
        <Text>
          {status === "all"
            ? "The queue is empty."
            : "No entries match this filter."}
        </Text>
      ) : null}
      {queue.changeError !== null ? (
        <View style={styles.status}>
          <Text accessibilityRole="alert">
            {mutationFailureMessage(queue.changeError, "transition")}
          </Text>
          <Pressable
            accessibilityRole="button"
            onPress={() => {
              queue.refreshAfterConflict();
            }}
          >
            <Text>Refresh queue</Text>
          </Pressable>
        </View>
      ) : null}
      <View accessibilityRole="list">
        {queue.entries.map((entry) => (
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
              accessibilityState={{ disabled: queue.isChanging }}
              disabled={queue.isChanging}
              onPress={() =>
                queue.changeState({
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
          accessibilityState={{ disabled: queue.isFetching }}
          disabled={queue.isFetching}
          onPress={queue.loadMore}
          style={styles.button}
        >
          <Text style={styles.buttonText}>
            {queue.isFetchingNextPage ? "Loading more…" : "Load more"}
          </Text>
        </Pressable>
      ) : null}
      {queue.entries.length > 0 && !queue.hasNextPage ? (
        <Text>End of queue.</Text>
      ) : null}
    </View>
  );
}
