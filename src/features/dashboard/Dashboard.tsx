import { RefreshCw } from "lucide-react";
import * as Switch from "@radix-ui/react-switch";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CategorySection } from "./CategorySection";
import { StatusNotices } from "./StatusNotices";
import { Badge } from "@/components/Badge";
import { useNow } from "@/hooks/useNow";
import { api } from "@/services/tauri";
import { useMonitoringStore } from "@/stores/monitoringStore";
import { CATEGORY_ORDER } from "@/utils/labels";
import type { ConnectionState } from "@/types";

const CONNECTION_LABEL: Record<ConnectionState, { tone: "good" | "warn" | "bad" | "neutral"; text: string }> = {
  not_started: { tone: "neutral", text: "Not started" },
  current: { tone: "good", text: "Connected · current" },
  stale: { tone: "warn", text: "Connected · stale" },
  authentication_required: { tone: "bad", text: "Authentication required" },
  rate_limited: { tone: "warn", text: "Rate limited" },
  temporarily_unavailable: { tone: "warn", text: "Temporarily unavailable" },
  permanently_unsupported: { tone: "bad", text: "Unsupported" },
  simulated: { tone: "warn", text: "Simulated" },
};

export function Dashboard() {
  const snapshot = useMonitoringStore((s) => s.snapshot);
  const now = useNow();
  const qc = useQueryClient();
  const refresh = useMutation({ mutationFn: api.refreshNow });
  const toggle = useMutation({
    mutationFn: (enabled: boolean) => api.setMonitoringEnabled(enabled),
    onSuccess: (cfg) => qc.setQueryData(["config"], cfg),
  });

  if (!snapshot) return <p className="p-6 text-sm text-zinc-400">Loading…</p>;
  const conn = CONNECTION_LABEL[snapshot.connection];
  const running = snapshot.runState === "running";

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Badge tone={conn.tone}>{conn.text}</Badge>
          {snapshot.lastSuccessAt && (
            <span className="text-xs text-zinc-500">last update {new Date(snapshot.lastSuccessAt).toLocaleTimeString()}</span>
          )}
        </div>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-zinc-300">
            Monitoring
            <Switch.Root
              checked={running}
              onCheckedChange={(v) => toggle.mutate(v)}
              aria-label="Monitoring enabled"
              className="relative h-5 w-9 rounded-full bg-zinc-700 outline-none data-[state=checked]:bg-emerald-600 focus-visible:ring-2 focus-visible:ring-sky-500"
            >
              <Switch.Thumb className="block h-4 w-4 translate-x-0.5 rounded-full bg-white transition-transform data-[state=checked]:translate-x-[18px]" />
            </Switch.Root>
          </label>
          <button
            type="button"
            onClick={() => refresh.mutate()}
            disabled={refresh.isPending}
            className="inline-flex items-center gap-1.5 rounded-md border border-zinc-700 px-2.5 py-1 text-sm hover:bg-zinc-800 disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${refresh.isPending ? "animate-spin" : ""}`} aria-hidden /> Refresh
          </button>
        </div>
      </div>

      <StatusNotices s={snapshot} />

      {CATEGORY_ORDER.map((category) => (
        <CategorySection
          key={category}
          category={category}
          coverage={snapshot.coverage.find((c) => c.category === category)?.state ?? "not_connected"}
          measurements={snapshot.measurements.filter((m) => m.category === category)}
          now={now}
        />
      ))}
    </div>
  );
}
