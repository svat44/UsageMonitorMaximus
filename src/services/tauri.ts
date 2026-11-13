import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { EVENTS } from "./events";
import type {
  AppConfig,
  AuthenticationRequiredPayload,
  DiagnosticsReport,
  ErrorInfo,
  LifecyclePayload,
  MonitoringSnapshot,
  ProviderDescriptor,
  ProviderStatusChanged,
  RateLimitedPayload,
  StatuslineSetup,
  SyncResult,
} from "@/types";

/** Typed wrappers around the Rust commands. Rejections are `ErrorInfo` objects. */
export const api = {
  getSnapshot: () => invoke<MonitoringSnapshot>("get_monitoring_snapshot"),
  getDiagnostics: () => invoke<DiagnosticsReport>("get_diagnostics"),
  listProviders: () => invoke<ProviderDescriptor[]>("list_providers"),
  getConfig: () => invoke<AppConfig>("get_config"),
  updateConfig: (config: AppConfig) => invoke<AppConfig>("update_config", { config }),
  setMonitoringEnabled: (enabled: boolean) => invoke<AppConfig>("set_monitoring_enabled", { enabled }),
  selectProvider: (providerId: string) => invoke<AppConfig>("select_provider", { providerId }),
  refreshNow: () => invoke<SyncResult>("refresh_now"),
  getStatuslineSetup: () => invoke<StatuslineSetup>("get_statusline_setup"),
  hideWindow: () => invoke<void>("hide_window"),
  quit: () => invoke<void>("quit_app"),
};

export interface EventHandlers {
  started?: (p: LifecyclePayload) => void;
  updated?: (p: MonitoringSnapshot) => void;
  providerStatusChanged?: (p: ProviderStatusChanged) => void;
  authenticationRequired?: (p: AuthenticationRequiredPayload) => void;
  rateLimited?: (p: RateLimitedPayload) => void;
  error?: (p: ErrorInfo) => void;
  stopped?: (p: LifecyclePayload) => void;
}

/** Subscribe to the documented backend events. Returns one function that unsubscribes all. */
export async function subscribe(h: EventHandlers): Promise<UnlistenFn> {
  const pairs: Array<[string, ((p: never) => void) | undefined]> = [
    [EVENTS.started, h.started],
    [EVENTS.updated, h.updated],
    [EVENTS.providerStatusChanged, h.providerStatusChanged],
    [EVENTS.authenticationRequired, h.authenticationRequired],
    [EVENTS.rateLimited, h.rateLimited],
    [EVENTS.error, h.error],
    [EVENTS.stopped, h.stopped],
  ];
  const unlisten: UnlistenFn[] = [];
  for (const [name, fn] of pairs) {
    if (fn) unlisten.push(await listen(name, (e) => (fn as (p: unknown) => void)(e.payload)));
  }
  return () => unlisten.forEach((u) => u());
}
