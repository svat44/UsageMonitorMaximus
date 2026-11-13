import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { EVENTS } from "@/services/events";
import type { AppConfig, MonitoringSnapshot } from "@/types";
import { defaultConfig, diagnostics, liveSnapshot, providers, setup, simulatedSnapshot, waitingSnapshot } from "./fixtures";

declare global {
  interface Window {
    __mock: { emitSnapshot: (s: MonitoringSnapshot) => Promise<void>; scenarios: Record<string, () => MonitoringSnapshot> };
  }
}

/** Replaces the Tauri IPC layer in a plain browser. Used by Playwright only. */
export function installTauriMock(): void {
  let config: AppConfig = { ...defaultConfig };
  let snapshot: MonitoringSnapshot = new URLSearchParams(location.search).get("scenario") === "waiting" ? waitingSnapshot() : liveSnapshot();

  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case "get_monitoring_snapshot": return snapshot;
        case "get_config": return config;
        case "list_providers": return providers;
        case "get_diagnostics": return diagnostics;
        case "get_statusline_setup": return setup;
        case "update_config": config = (args as { config: AppConfig }).config; return config;
        case "set_monitoring_enabled": config = { ...config, monitoringEnabled: (args as { enabled: boolean }).enabled }; return config;
        case "select_provider": config = { ...config, selectedProvider: (args as { providerId: string }).providerId }; return config;
        case "refresh_now": return { result: "succeeded" };
        default: return undefined;
      }
    },
    { shouldMockEvents: true },
  );

  window.__mock = {
    emitSnapshot: async (s) => {
      snapshot = s;
      await emit(EVENTS.updated, s);
    },
    scenarios: { live: liveSnapshot, waiting: waitingSnapshot, simulated: simulatedSnapshot },
  };
}
