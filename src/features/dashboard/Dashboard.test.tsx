import { render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Dashboard } from "./Dashboard";
import { liveSnapshot, simulatedSnapshot, waitingSnapshot } from "@/mocks/fixtures";
import { useMonitoringStore } from "@/stores/monitoringStore";
import type { MonitoringSnapshot } from "@/types";

vi.mock("@/services/tauri", () => ({
  api: { refreshNow: vi.fn(async () => ({ result: "succeeded" })), setMonitoringEnabled: vi.fn() },
}));

function show(s: MonitoringSnapshot) {
  useMonitoringStore.setState({ snapshot: s });
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <Dashboard />
    </QueryClientProvider>,
  );
}

describe("Dashboard", () => {
  beforeEach(() => useMonitoringStore.setState({ snapshot: null, activity: [] }));

  it("shows a loading state before the first snapshot", () => {
    show(null as unknown as MonitoringSnapshot);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("keeps subscription, local and API categories separate", () => {
    show(liveSnapshot());
    const sub = within(screen.getByTestId("cat-subscription_usage"));
    const local = within(screen.getByTestId("cat-local_claude_code"));
    const api = within(screen.getByTestId("cat-api_consumption"));
    expect(sub.getByText("5-hour window used")).toBeInTheDocument();
    expect(sub.getByTestId("v-subscription_five_hour_utilization")).toHaveTextContent("23.5%");
    // The context-window percentage lives ONLY under local Claude Code, never under subscription.
    expect(sub.queryByText("Context window used")).toBeNull();
    expect(local.getByText("Context window used")).toBeInTheDocument();
    expect(local.getByTestId("v-local_context_window_utilization")).toHaveTextContent("8%");
    expect(api.getByText("Not connected")).toBeInTheDocument();
    expect(api.getByText("Nothing reported for this category.")).toBeInTheDocument();
  });

  it("labels cost as an estimate", () => {
    show(liveSnapshot());
    expect(within(screen.getByTestId("m-local_session_cost_estimate")).getByText("Estimate")).toBeInTheDocument();
  });

  it("says subscription usage is not connected when no real data exists, and shows no zeros", () => {
    show(waitingSnapshot());
    expect(screen.getByText("Subscription usage is not connected yet")).toBeInTheDocument();
    const v = screen.getByTestId("v-subscription_five_hour_utilization");
    expect(v).toHaveTextContent("No data yet");
    expect(v).not.toHaveTextContent("0%");
    expect(within(screen.getByTestId("cat-subscription_usage")).getByText("Not connected")).toBeInTheDocument();
  });

  it("flags simulated data prominently and never as connected", () => {
    show(simulatedSnapshot());
    expect(screen.getByText("Simulated data")).toBeInTheDocument();
    expect(screen.queryByText("Subscription usage is not connected yet")).toBeNull();
    expect(screen.getAllByText("Simulated").length).toBeGreaterThan(3);
    expect(screen.queryByText("Live")).toBeNull();
  });

  it("marks stale and restored values and explains rate limiting", () => {
    const s = liveSnapshot();
    s.connection = "rate_limited";
    s.rateLimitedUntil = new Date(Date.now() + 120_000).toISOString();
    s.measurements = s.measurements.map((m) => ({ ...m, stale: true, restored: m.id === "local_active_sessions" }));
    show(s);
    expect(screen.getByText("Rate limited", { selector: "div" })).toBeInTheDocument();
    expect(screen.getAllByText("Stale").length).toBeGreaterThan(0);
    expect(screen.getByText("From history")).toBeInTheDocument();
  });

  it("shows authentication required with the backend message", () => {
    const s = liveSnapshot();
    s.connection = "authentication_required";
    s.latestError = { category: "authentication", message: "token expired", providerId: "p", operation: "fetch", occurredAt: new Date().toISOString(), retryAfterSecs: null };
    show(s);
    expect(screen.getByText("Authentication required", { selector: "div" })).toBeInTheDocument();
    expect(screen.getByText("token expired")).toBeInTheDocument();
  });
});
