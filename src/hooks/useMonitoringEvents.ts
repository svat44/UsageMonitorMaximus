import { useEffect } from "react";
import { api, subscribe } from "@/services/tauri";
import { useMonitoringStore } from "@/stores/monitoringStore";

/** Load the snapshot once, then keep it current from backend events (no polling). */
export function useMonitoringEvents(): void {
  useEffect(() => {
    const { setSnapshot, log } = useMonitoringStore.getState();
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void subscribe({
      started: (p) => log("started", p.providerId ?? ""),
      updated: (s) => setSnapshot(s),
      providerStatusChanged: (p) => log("provider-status-changed", `${p.previous} → ${p.current}`),
      authenticationRequired: (p) => log("authentication-required", p.providerId),
      rateLimited: (p) => log("rate-limited", `resume at ${p.resumeAt}`),
      error: (e) => log("error", `${e.category}: ${e.message}`),
      stopped: () => log("stopped", ""),
    }).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    // Subscribe first, then fetch, so no update can fall between the two.
    api.getSnapshot().then((s) => !disposed && setSnapshot(s)).catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}
