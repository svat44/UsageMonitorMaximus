import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { liveSnapshot } from "@/mocks/fixtures";
import { useMonitoringStore } from "@/stores/monitoringStore";
import type { EventHandlers } from "@/services/tauri";

let handlers: EventHandlers = {};
const unlisten = vi.fn();
vi.mock("@/services/tauri", () => ({
  api: { getSnapshot: vi.fn(async () => liveSnapshot()) },
  subscribe: vi.fn(async (h: EventHandlers) => {
    handlers = h;
    return unlisten;
  }),
}));

import { useMonitoringEvents } from "./useMonitoringEvents";

describe("useMonitoringEvents", () => {
  beforeEach(() => {
    useMonitoringStore.setState({ snapshot: null, activity: [] });
    unlisten.mockClear();
  });

  it("loads the initial snapshot once, then follows backend events without polling", async () => {
    const { unmount } = renderHook(() => useMonitoringEvents());
    await waitFor(() => expect(useMonitoringStore.getState().snapshot?.connection).toBe("current"));

    const next = { ...liveSnapshot(), connection: "stale" as const };
    act(() => handlers.updated?.(next));
    expect(useMonitoringStore.getState().snapshot?.connection).toBe("stale");

    act(() => handlers.rateLimited?.({ providerId: "p", retryAfterSecs: 60, resumeAt: "2026-06-01T12:00:00Z" }));
    act(() => handlers.error?.({ category: "network", message: "down", providerId: "p", operation: "fetch", occurredAt: "", retryAfterSecs: null }));
    const log = useMonitoringStore.getState().activity.map((a) => a.event);
    expect(log).toEqual(["error", "rate-limited"]);

    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalled());
  });
});
