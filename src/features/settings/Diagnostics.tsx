import { useQuery } from "@tanstack/react-query";
import { api } from "@/services/tauri";
import { useMonitoringStore } from "@/stores/monitoringStore";

export function Diagnostics() {
  const diag = useQuery({ queryKey: ["diagnostics"], queryFn: api.getDiagnostics, refetchInterval: 30_000 });
  const activity = useMonitoringStore((s) => s.activity);
  const d = diag.data;
  return (
    <details className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4">
      <summary className="cursor-pointer text-sm font-semibold uppercase tracking-wide text-zinc-300">Diagnostics</summary>
      {d && (
        <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
          <dt className="text-zinc-500">App version</dt><dd>{d.appVersion}</dd>
          <dt className="text-zinc-500">Database</dt><dd>{d.databaseInitialized ? `ready (schema v${d.schemaVersion})` : "not initialised"}</dd>
          <dt className="text-zinc-500">Provider</dt><dd>{d.activeProvider ?? "—"} {d.providerHealth && `(${d.providerHealth.state})`}</dd>
          <dt className="text-zinc-500">Status</dt><dd>{d.runState} / {d.connection}</dd>
          <dt className="text-zinc-500">Last success</dt><dd>{d.lastSuccessAt ?? "never"}</dd>
          <dt className="text-zinc-500">Latest error</dt><dd>{d.latestErrorCategory ?? "none"}</dd>
          <dt className="text-zinc-500">Data dir</dt><dd className="font-mono break-all">{d.dataDir}</dd>
        </dl>
      )}
      <h3 className="mt-3 text-xs font-medium text-zinc-400">Recent events</h3>
      <ul className="mt-1 max-h-40 overflow-auto font-mono text-[11px] text-zinc-400">
        {activity.length === 0 && <li>none yet</li>}
        {activity.map((a, i) => <li key={i}>{a.at.slice(11, 19)} {a.event} {a.detail}</li>)}
      </ul>
    </details>
  );
}
