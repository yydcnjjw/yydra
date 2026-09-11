// SPDX-License-Identifier: MIT OR Apache-2.0

import { ScrollView, Text } from "react-native";
import { ReadingEntryForm } from "./entry-form";
import { ReadingQueuePanel } from "./queue-panel";
import { ReadingQueueSort, ReadingQueueStatusFilter } from "./queries";
import { ServiceStatus } from "./service-status";
import { styles } from "./styles";
import { useReadingQueue } from "./use-reading-queue";

export interface ReadingQueueScreenProps {
  onRouteStateChange(
    status: ReadingQueueStatusFilter,
    sort: ReadingQueueSort,
  ): void;
  onSortPreferenceChange?(sort: ReadingQueueSort): void;
  productName: string;
  sort: ReadingQueueSort;
  status: ReadingQueueStatusFilter;
}

export function ReadingQueueScreen({
  onRouteStateChange,
  onSortPreferenceChange,
  productName,
  sort,
  status,
}: ReadingQueueScreenProps) {
  const queue = useReadingQueue(status, sort);
  return (
    <ScrollView contentContainerStyle={styles.container} role="main">
      <Text accessibilityRole="header" style={styles.title}>
        {productName}
      </Text>
      <ServiceStatus />
      <ReadingEntryForm addEntry={queue.addEntry} />
      <ReadingQueuePanel
        queue={queue}
        onRouteStateChange={onRouteStateChange}
        onSortPreferenceChange={onSortPreferenceChange}
        sort={sort}
        status={status}
      />
    </ScrollView>
  );
}
