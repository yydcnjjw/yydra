// SPDX-License-Identifier: MIT OR Apache-2.0

import {
  ErrorBoundaryProps,
  useLocalSearchParams,
  useRouter,
} from "expo-router";
import { Pressable, StyleSheet, Text, View } from "react-native";

import {
  ReadingQueueSort,
  ReadingQueueStatusFilter,
} from "@/product-presentation/reading-queue/queries";
import { ProductAuthGate } from "@/product-presentation/auth/runtime";
import { ReadingQueueScreen } from "@/product-presentation/reading-queue/screen";

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

export default function IndexRoute() {
  const router = useRouter();
  const search = useLocalSearchParams<{
    status?: string | string[];
    sort?: string | string[];
  }>();
  const status = statusFromUrl(search.status);
  const sort = sortFromUrl(search.sort);

  return (
    <ProductAuthGate>
      <ReadingQueueScreen
        onRouteStateChange={(nextStatus, nextSort) =>
          router.setParams({
            sort: nextSort,
            status: nextStatus,
          })
        }
        productName={__PRODUCT_NAME_JSON__}
        sort={sort}
        status={status}
      />
    </ProductAuthGate>
  );
}

export function ErrorBoundary({ retry }: ErrorBoundaryProps) {
  return (
    <View role="alert" style={styles.fallback}>
      <Text accessibilityRole="header" style={styles.heading}>
        This screen could not be displayed safely.
      </Text>
      <Text>No internal error details are shown.</Text>
      <Pressable
        accessibilityRole="button"
        onPress={retry}
        style={styles.button}
      >
        <Text style={styles.buttonText}>Try this screen again</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  button: {
    backgroundColor: "#0f172a",
    borderRadius: 8,
    padding: 12,
  },
  buttonText: {
    color: "#ffffff",
    fontWeight: "700",
  },
  fallback: {
    alignItems: "center",
    gap: 12,
    marginHorizontal: "auto",
    maxWidth: 560,
    padding: 24,
    width: "100%",
  },
  heading: {
    fontSize: 24,
    fontWeight: "700",
  },
});
