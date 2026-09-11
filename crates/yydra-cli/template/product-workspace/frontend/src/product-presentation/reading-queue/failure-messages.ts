// SPDX-License-Identifier: MIT OR Apache-2.0

import { isFrameworkFailure } from "@/framework/runtime";

export function queueFailurePresentation(
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

export function failureMessage(
  error: unknown,
  problemFallback: string,
): string {
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

export function mutationFailureMessage(
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
