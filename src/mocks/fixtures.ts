import type { AppConfig, DiagnosticsReport, Measurement, MonitoringSnapshot, ProviderDescriptor, StatuslineSetup } from "@/types";

const iso = (offsetSec: number) => new Date(Date.now() + offsetSec * 1000).toISOString();

export function measurement(p: Partial<Measurement> & Pick<Measurement, "id" | "category" | "unit" | "value">): Measurement {
  return {
    providerId: "claude-code-statusline",
    scope: null,
    collectedAt: iso(0),
    sourceAt: iso(-5),
    verification: "official_local_interface",
    simulated: false,
    estimated: false,
    stale: false,
    restored: false,
    resetsAt: null,
    window: null,
    ...p,
  };
}

/** Real-looking provider with Claude Code connected (subscriber). */
export function liveSnapshot(): MonitoringSnapshot {
  return {
    runState: "running",
    activeProvider: "claude-code-statusline",
    connection: "current",
    simulated: false,
    measurements: [
      measurement({ id: "subscription_five_hour_utilization", category: "subscription_usage", unit: "percent", value: { state: "available", value: 23.5 }, resetsAt: iso(2 * 3600), window: "five_hour" }),
      measurement({ id: "subscription_weekly_utilization", category: "subscription_usage", unit: "percent", value: { state: "available", value: 41.2 }, resetsAt: iso(3 * 86400), window: "seven_day" }),
      measurement({ id: "local_active_sessions", category: "local_claude_code", unit: "count", value: { state: "available", value: 1 } }),
      measurement({ id: "local_context_window_utilization", category: "local_claude_code", unit: "percent", value: { state: "available", value: 8 }, scope: "abc123" }),
      measurement({ id: "local_session_cost_estimate", category: "local_claude_code", unit: "usd", value: { state: "available", value: 0.0123 }, scope: "abc123", estimated: true }),
    ],
    coverage: [
      { category: "subscription_usage", state: "live" },
      { category: "api_consumption", state: "not_connected" },
      { category: "local_claude_code", state: "live" },
    ],
    lastAttemptAt: iso(-3),
    lastSuccessAt: iso(-3),
    nextAttemptAt: iso(27),
    rateLimitedUntil: null,
    consecutiveFailures: 0,
    latestError: null,
  };
}

/** Provider is reachable but Claude Code has reported nothing yet. */
export function waitingSnapshot(): MonitoringSnapshot {
  const un = (id: Measurement["id"], category: Measurement["category"], unit: Measurement["unit"]) =>
    measurement({ id, category, unit, sourceAt: null, value: { state: "unavailable", reason: "no_data_yet" } });
  return {
    ...liveSnapshot(),
    measurements: [
      un("subscription_five_hour_utilization", "subscription_usage", "percent"),
      un("subscription_weekly_utilization", "subscription_usage", "percent"),
      un("local_active_sessions", "local_claude_code", "count"),
    ],
    coverage: [
      { category: "subscription_usage", state: "not_connected" },
      { category: "api_consumption", state: "not_connected" },
      { category: "local_claude_code", state: "not_connected" },
    ],
    lastSuccessAt: iso(-3),
  };
}

export function simulatedSnapshot(): MonitoringSnapshot {
  const s = liveSnapshot();
  return {
    ...s,
    simulated: true,
    connection: "simulated",
    activeProvider: "simulated",
    measurements: s.measurements.map((m) => ({ ...m, simulated: true, verification: "simulated" as const, providerId: "simulated" })),
    coverage: s.coverage.map((c) => ({ ...c, state: c.category === "api_consumption" ? "simulated" : "simulated" })),
  };
}

export const providers: ProviderDescriptor[] = [
  {
    id: "claude-code-statusline",
    displayName: "Claude Code (status line bridge)",
    description: "Reads Claude Code's documented status-line data.",
    simulated: false,
    capabilities: [],
    refresh: { minIntervalSecs: 5, recommendedIntervalSecs: 30, pollingSupported: true, eventDriven: true, requestTimeoutSecs: 10, staleAfterSecs: 600, documentedRateLimit: null },
  },
  {
    id: "simulated",
    displayName: "Simulated data (development only)",
    description: "Deterministic fake values. Never real usage.",
    simulated: true,
    capabilities: [],
    refresh: { minIntervalSecs: 1, recommendedIntervalSecs: 5, pollingSupported: true, eventDriven: false, requestTimeoutSecs: 5, staleAfterSecs: 60, documentedRateLimit: null },
  },
];

export const defaultConfig: AppConfig = {
  selectedProvider: "claude-code-statusline",
  monitoringEnabled: true,
  refreshIntervalSecs: null,
  launchAtStartup: false,
  minimizeToTray: true,
  startMinimized: false,
  logLevel: "info",
};

export const diagnostics: DiagnosticsReport = {
  appVersion: "0.1.0",
  databaseInitialized: true,
  schemaVersion: 1,
  activeProvider: "claude-code-statusline",
  providerHealth: { state: "healthy", detail: null, lastSuccessAt: null },
  runState: "running",
  connection: "current",
  lastSuccessAt: null,
  latestErrorCategory: null,
  dataDir: "/mock/data",
};

export const setup: StatuslineSetup = {
  bridgePath: null,
  dataDir: "/mock/data",
  settingsSnippet: JSON.stringify({ statusLine: { type: "command", command: "<path to usage-monitor-statusline>" } }, null, 2),
};
