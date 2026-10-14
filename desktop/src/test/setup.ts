import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
  // frontend-boundary.test.ts runs in the node environment, without a window.
  if (typeof window !== "undefined") window.location.hash = "";
});
