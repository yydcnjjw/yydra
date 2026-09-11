// SPDX-License-Identifier: MIT OR Apache-2.0

import { StyleSheet } from "react-native";

export const styles = StyleSheet.create({
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
