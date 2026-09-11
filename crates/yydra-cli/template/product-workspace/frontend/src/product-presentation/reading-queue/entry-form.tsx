// SPDX-License-Identifier: MIT OR Apache-2.0

import { useRef, useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { mutationFailureMessage } from "./failure-messages";
import { styles } from "./styles";

export function ReadingEntryForm({
  addEntry,
}: {
  addEntry(input: { title: string; sourceUrl: string }): Promise<unknown>;
}) {
  const [title, setTitle] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const revision = useRef(0);
  const submitting = useRef(false);

  async function submit() {
    if (submitting.current) return;
    submitting.current = true;
    const submittedRevision = revision.current;
    setIsSubmitting(true);
    setError(null);
    try {
      await addEntry({ sourceUrl, title });
      if (revision.current === submittedRevision) {
        setTitle("");
        setSourceUrl("");
      }
    } catch (cause) {
      setError(cause);
    } finally {
      submitting.current = false;
      setIsSubmitting(false);
    }
  }

  return (
    <View style={styles.panel}>
      <Text accessibilityRole="header" style={styles.sectionTitle}>
        Add to Reading Queue
      </Text>
      <TextInput
        accessibilityLabel="Entry title"
        onChangeText={(value) => {
          revision.current += 1;
          setTitle(value);
        }}
        placeholder="Entry title"
        style={styles.input}
        value={title}
      />
      <TextInput
        accessibilityLabel="Source URL"
        autoCapitalize="none"
        inputMode="url"
        onChangeText={(value) => {
          revision.current += 1;
          setSourceUrl(value);
        }}
        placeholder="https://example.com/article"
        style={styles.input}
        value={sourceUrl}
      />
      <Pressable
        accessibilityRole="button"
        accessibilityState={{ disabled: isSubmitting }}
        disabled={isSubmitting}
        onPress={() => void submit()}
        style={styles.button}
      >
        <Text style={styles.buttonText}>
          {isSubmitting ? "Adding…" : "Add entry"}
        </Text>
      </Pressable>
      {error !== null ? (
        <Text accessibilityRole="alert">
          {mutationFailureMessage(error, "create")}
        </Text>
      ) : null}
    </View>
  );
}
