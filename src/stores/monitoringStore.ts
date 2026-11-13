import { create } from "zustand";
import type { MonitoringSnapshot } from "@/types";

export interface ActivityEntry {
  at: string;
  event: string;
  detail: string;
}

interface MonitoringState {
  snapshot: MonitoringSnapshot | null;
  activity: ActivityEntry[];
  setSnapshot: (s: MonitoringSnapshot) => void;
  log: (event: string, detail: string) => void;
}

const MAX_ACTIVITY = 30;

/** Live state mirrored from the Rust backend (the backend stays the single source of truth). */
export const useMonitoringStore = create<MonitoringState>((set) => ({
  snapshot: null,
  activity: [],
  setSnapshot: (snapshot) => set({ snapshot }),
  log: (event, detail) =>
    set((s) => ({
      activity: [{ at: new Date().toISOString(), event, detail }, ...s.activity].slice(0, MAX_ACTIVITY),
    })),
}));
