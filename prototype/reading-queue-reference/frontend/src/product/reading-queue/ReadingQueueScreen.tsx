import { useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Linking,
  Platform,
  Pressable,
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { ProblemResponseError } from '@/framework/api/readingQueue';

import { availableAction, ReadingEntry, ReadingQueueFilter } from './domain';
import {
  useAddReadingEntry,
  useReadingEntries,
  useReadingEntryTransition,
} from './queries';

const FILTERS: readonly ReadingQueueFilter[] = ['all', 'queued', 'completed'];

export function ReadingQueueScreen() {
  const [filter, setFilter] = useState<ReadingQueueFilter>('all');
  const [title, setTitle] = useState('');
  const [sourceUrl, setSourceUrl] = useState('');
  const entriesQuery = useReadingEntries(filter);
  const addEntry = useAddReadingEntry();
  const transition = useReadingEntryTransition();
  const entries = useMemo(
    () => entriesQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [entriesQuery.data],
  );

  async function submitEntry() {
    await addEntry.mutateAsync({ title: title.trim(), sourceUrl: sourceUrl.trim() });
    setTitle('');
    setSourceUrl('');
  }

  return (
    <SafeAreaView style={styles.safeArea}>
      <ScrollView contentContainerStyle={styles.page} keyboardShouldPersistTaps="handled">
        <View style={styles.hero}>
          <Text style={styles.eyebrow}>YYDRA GOLDEN STACK / REFERENCE</Text>
          <Text accessibilityRole="header" style={styles.heading}>
            Reading Queue
          </Text>
          <Text style={styles.intro}>
            Keep a small, dependable queue of what you want to read next.
          </Text>
        </View>

        <View accessibilityLabel="Add a reading entry" style={styles.panel}>
          <Text style={styles.sectionTitle}>Add something worth returning to</Text>
          <TextInput
            accessibilityLabel="Title"
            onChangeText={setTitle}
            placeholder="A useful title"
            placeholderTextColor="#7f8782"
            style={styles.input}
            value={title}
          />
          <TextInput
            accessibilityLabel="Source URL"
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
            onChangeText={setSourceUrl}
            placeholder="https://example.com/article"
            placeholderTextColor="#7f8782"
            style={styles.input}
            value={sourceUrl}
          />
          <Pressable
            accessibilityRole="button"
            disabled={!title.trim() || !sourceUrl.trim() || addEntry.isPending}
            onPress={() => void submitEntry()}
            style={({ pressed }) => [
              styles.primaryButton,
              pressed && styles.buttonPressed,
              (!title.trim() || !sourceUrl.trim() || addEntry.isPending) && styles.buttonDisabled,
            ]}
          >
            <Text style={styles.primaryButtonText}>
              {addEntry.isPending ? 'Adding…' : 'Add to queue'}
            </Text>
          </Pressable>
          {addEntry.error ? <ErrorNotice error={addEntry.error} /> : null}
        </View>

        <View style={styles.queueHeader}>
          <Text style={styles.sectionTitle}>Your queue</Text>
          <View accessibilityLabel="Filter reading entries" style={styles.filters}>
            {FILTERS.map((value) => (
              <Pressable
                accessibilityRole="button"
                accessibilityState={{ selected: filter === value }}
                key={value}
                onPress={() => setFilter(value)}
                style={[styles.filterButton, filter === value && styles.filterButtonActive]}
              >
                <Text style={[styles.filterText, filter === value && styles.filterTextActive]}>
                  {filterLabel(value)}
                </Text>
              </Pressable>
            ))}
          </View>
        </View>

        {entriesQuery.isPending ? (
          <ActivityIndicator accessibilityLabel="Loading reading entries" color="#215b42" />
        ) : null}
        {entriesQuery.error ? <ErrorNotice error={entriesQuery.error} /> : null}
        {!entriesQuery.isPending && !entriesQuery.error && entries.length === 0 ? (
          <View style={styles.emptyState}>
            <Text style={styles.emptyTitle}>Nothing here yet.</Text>
            <Text style={styles.emptyBody}>Add an article above or try another filter.</Text>
          </View>
        ) : null}

        <View style={styles.entries}>
          {entries.map((entry) => (
            <EntryCard
              busy={transition.isPending && transition.variables?.id === entry.id}
              entry={entry}
              key={entry.id}
              onTransition={() =>
                transition.mutate({ id: entry.id, action: availableAction(entry) })
              }
            />
          ))}
        </View>
        {transition.error ? <ErrorNotice error={transition.error} /> : null}

        {entriesQuery.hasNextPage ? (
          <Pressable
            accessibilityRole="button"
            disabled={entriesQuery.isFetchingNextPage}
            onPress={() => void entriesQuery.fetchNextPage()}
            style={styles.secondaryButton}
          >
            <Text style={styles.secondaryButtonText}>
              {entriesQuery.isFetchingNextPage ? 'Loading…' : 'Load more'}
            </Text>
          </Pressable>
        ) : null}
      </ScrollView>
    </SafeAreaView>
  );
}

function EntryCard({
  busy,
  entry,
  onTransition,
}: {
  busy: boolean;
  entry: ReadingEntry;
  onTransition: () => void;
}) {
  const action = availableAction(entry);
  return (
    <View accessibilityLabel={`Reading entry ${entry.title}`} style={styles.entryCard}>
      <View style={styles.entryCopy}>
        <Text accessibilityRole="header" style={styles.entryTitle}>
          {entry.title}
        </Text>
        <Pressable
          accessibilityRole="link"
          onPress={() => void Linking.openURL(entry.sourceUrl)}
        >
          <Text numberOfLines={1} style={styles.url}>
            {entry.sourceUrl}
          </Text>
        </Pressable>
      </View>
      <View style={styles.entryActions}>
        <Text style={[styles.status, entry.status === 'completed' && styles.statusCompleted]}>
          {entry.status === 'queued' ? 'Queued' : 'Completed'}
        </Text>
        <Pressable
          accessibilityLabel={`${action === 'complete' ? 'Complete' : 'Reopen'} ${entry.title}`}
          accessibilityRole="button"
          disabled={busy}
          onPress={onTransition}
          style={({ pressed }) => [styles.actionButton, pressed && styles.buttonPressed]}
        >
          <Text style={styles.actionButtonText}>
            {busy ? 'Saving…' : action === 'complete' ? 'Complete' : 'Reopen'}
          </Text>
        </Pressable>
      </View>
    </View>
  );
}

function ErrorNotice({ error }: { error: Error }) {
  const message =
    error instanceof ProblemResponseError
      ? `${error.problem.detail} (trace ${error.problem.traceId})`
      : error.message;
  return (
    <View accessibilityRole="alert" style={styles.error}>
      <Text style={styles.errorText}>{message}</Text>
    </View>
  );
}

function filterLabel(filter: ReadingQueueFilter): string {
  switch (filter) {
    case 'all':
      return 'All';
    case 'queued':
      return 'Queued';
    case 'completed':
      return 'Completed';
  }
}

const styles = StyleSheet.create({
  safeArea: { flex: 1, backgroundColor: '#f3f0e8' },
  page: {
    alignSelf: 'center',
    width: '100%',
    maxWidth: 880,
    paddingHorizontal: 20,
    paddingBottom: 64,
  },
  hero: { paddingTop: Platform.OS === 'web' ? 64 : 36, paddingBottom: 32 },
  eyebrow: { color: '#9d4c2d', fontSize: 12, fontWeight: '800', letterSpacing: 1.4 },
  heading: { color: '#18362a', fontSize: 44, fontWeight: '800', letterSpacing: -1.5, marginTop: 8 },
  intro: { color: '#53625b', fontSize: 17, lineHeight: 26, marginTop: 8 },
  panel: {
    backgroundColor: '#fffdf7',
    borderColor: '#d9d5c9',
    borderRadius: 16,
    borderWidth: 1,
    gap: 12,
    padding: 20,
  },
  sectionTitle: { color: '#18362a', fontSize: 20, fontWeight: '700' },
  input: {
    backgroundColor: '#ffffff',
    borderColor: '#b8beb8',
    borderRadius: 10,
    borderWidth: 1,
    color: '#16231d',
    fontSize: 16,
    paddingHorizontal: 14,
    paddingVertical: 12,
  },
  primaryButton: { alignItems: 'center', backgroundColor: '#215b42', borderRadius: 10, padding: 13 },
  primaryButtonText: { color: '#ffffff', fontSize: 16, fontWeight: '700' },
  buttonDisabled: { opacity: 0.45 },
  buttonPressed: { opacity: 0.72 },
  queueHeader: { gap: 14, marginTop: 38, marginBottom: 18 },
  filters: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  filterButton: { borderColor: '#9ea99f', borderRadius: 999, borderWidth: 1, paddingHorizontal: 14, paddingVertical: 8 },
  filterButtonActive: { backgroundColor: '#18362a', borderColor: '#18362a' },
  filterText: { color: '#3f5148', fontWeight: '600' },
  filterTextActive: { color: '#ffffff' },
  entries: { gap: 12 },
  entryCard: {
    backgroundColor: '#fffdf7',
    borderColor: '#d9d5c9',
    borderRadius: 14,
    borderWidth: 1,
    gap: 16,
    padding: 18,
  },
  entryCopy: { flex: 1, gap: 6 },
  entryTitle: { color: '#172820', fontSize: 18, fontWeight: '700' },
  url: { color: '#356b78', fontSize: 14, textDecorationLine: 'underline' },
  entryActions: { alignItems: 'center', flexDirection: 'row', gap: 10, justifyContent: 'space-between' },
  status: { backgroundColor: '#f4ddaa', borderRadius: 999, color: '#664a14', fontSize: 12, fontWeight: '700', overflow: 'hidden', paddingHorizontal: 10, paddingVertical: 6 },
  statusCompleted: { backgroundColor: '#d6e8dc', color: '#215b42' },
  actionButton: { borderColor: '#215b42', borderRadius: 8, borderWidth: 1, paddingHorizontal: 13, paddingVertical: 8 },
  actionButtonText: { color: '#215b42', fontWeight: '700' },
  secondaryButton: { alignItems: 'center', borderColor: '#215b42', borderRadius: 10, borderWidth: 1, marginTop: 18, padding: 12 },
  secondaryButtonText: { color: '#215b42', fontWeight: '700' },
  emptyState: { alignItems: 'center', borderColor: '#c9c8c0', borderRadius: 14, borderStyle: 'dashed', borderWidth: 1, gap: 4, padding: 32 },
  emptyTitle: { color: '#273d33', fontSize: 17, fontWeight: '700' },
  emptyBody: { color: '#68736d' },
  error: { backgroundColor: '#f9ded8', borderRadius: 8, padding: 12 },
  errorText: { color: '#7c291f', lineHeight: 20 },
});
