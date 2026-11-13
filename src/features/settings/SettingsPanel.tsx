import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/services/tauri";
import { useMonitoringStore } from "@/stores/monitoringStore";
import type { AppConfig, ErrorInfo } from "@/types";

function Check({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex items-center gap-2 text-sm text-zinc-300">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="h-4 w-4 accent-emerald-500" />
      {label}
    </label>
  );
}

export function SettingsPanel() {
  const qc = useQueryClient();
  const config = useQuery({ queryKey: ["config"], queryFn: api.getConfig });
  const providers = useQuery({ queryKey: ["providers"], queryFn: api.listProviders });
  const save = useMutation<AppConfig, ErrorInfo, AppConfig>({
    mutationFn: api.updateConfig,
    onSuccess: (c) => qc.setQueryData(["config"], c),
  });
  const selectProvider = useMutation<AppConfig, ErrorInfo, string>({
    mutationFn: api.selectProvider,
    onSuccess: (c) => qc.setQueryData(["config"], c),
  });
  const connection = useMonitoringStore((s) => s.snapshot?.connection);

  if (!config.data || !providers.data) return null;
  const c = config.data;
  const active = providers.data.find((p) => p.id === c.selectedProvider);
  const update = (patch: Partial<AppConfig>) => save.mutate({ ...c, ...patch });

  return (
    <section aria-labelledby="settings" className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4">
      <h2 id="settings" className="text-sm font-semibold uppercase tracking-wide text-zinc-300">Settings</h2>
      <div className="mt-3 grid gap-4 sm:grid-cols-2">
        <div className="space-y-3">
          <label className="block text-sm text-zinc-300">
            Data source
            <select
              value={c.selectedProvider}
              onChange={(e) => selectProvider.mutate(e.target.value)}
              className="mt-1 block w-full rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-sm"
            >
              {providers.data.map((p) => (
                <option key={p.id} value={p.id}>{p.displayName}</option>
              ))}
            </select>
          </label>
          {active && <p className="text-xs text-zinc-500">{active.description}</p>}
          {active && (
            <label className="block text-sm text-zinc-300">
              Refresh interval (seconds)
              <input
                type="number"
                min={active.refresh.minIntervalSecs}
                placeholder={`${active.refresh.recommendedIntervalSecs} (default)`}
                defaultValue={c.refreshIntervalSecs ?? ""}
                key={`${c.selectedProvider}-${c.refreshIntervalSecs ?? "d"}`}
                onBlur={(e) => update({ refreshIntervalSecs: e.target.value === "" ? null : Number(e.target.value) })}
                className="mt-1 block w-32 rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-sm"
              />
              <span className="text-xs text-zinc-500">Minimum {active.refresh.minIntervalSecs}s for this source. Empty = recommended.</span>
            </label>
          )}
        </div>
        <div className="space-y-2">
          <Check label="Hide to tray when the window is closed" checked={c.minimizeToTray} onChange={(v) => update({ minimizeToTray: v })} />
          <Check label="Start with the operating system" checked={c.launchAtStartup} onChange={(v) => update({ launchAtStartup: v })} />
          <Check label="Start minimised (tray only)" checked={c.startMinimized} onChange={(v) => update({ startMinimized: v })} />
          <label className="block text-sm text-zinc-300">
            Log level
            <select
              value={c.logLevel}
              onChange={(e) => update({ logLevel: e.target.value as AppConfig["logLevel"] })}
              className="mt-1 block rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-sm"
            >
              {["error", "warn", "info", "debug", "trace"].map((l) => <option key={l}>{l}</option>)}
            </select>
          </label>
        </div>
      </div>
      {(save.error || selectProvider.error) && (
        <p role="alert" className="mt-3 text-sm text-red-300">{(save.error ?? selectProvider.error)?.message}</p>
      )}
      <div className="mt-4 flex gap-2">
        <button type="button" onClick={() => api.hideWindow()} className="rounded-md border border-zinc-700 px-2.5 py-1 text-sm hover:bg-zinc-800">Hide to tray</button>
        <button type="button" onClick={() => api.quit()} className="rounded-md border border-red-800 px-2.5 py-1 text-sm text-red-300 hover:bg-red-950">Quit</button>
        {connection && <span className="ml-auto self-center text-xs text-zinc-600">state: {connection}</span>}
      </div>
    </section>
  );
}
