// SPDX-License-Identifier: MIT OR Apache-2.0
// @vitest-environment jsdom

import { useQueryClient } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import {
  createTestFrameworkRuntime,
  FrameworkClient,
  FrameworkRuntime,
  useFrameworkClient,
} from "./runtime";

describe("Framework Runtime assembly seam", () => {
  it("provides the injected fake client and isolated QueryClient through the production component seam", () => {
    const fakeClient = {} as FrameworkClient;
    const runtime = createTestFrameworkRuntime(fakeClient);

    function RuntimeProbe() {
      const client = useFrameworkClient();
      const queryClient = useQueryClient();
      return (
        <span>
          {client === fakeClient && queryClient === runtime.queryClient
            ? "same-runtime-seam"
            : "wrong-runtime-seam"}
        </span>
      );
    }

    render(
      <FrameworkRuntime runtime={runtime}>
        <RuntimeProbe />
      </FrameworkRuntime>,
    );

    expect(screen.getByText("same-runtime-seam")).toBeTruthy();
  });
});
