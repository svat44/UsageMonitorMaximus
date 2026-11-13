import { Dashboard } from "@/features/dashboard/Dashboard";
import { ConnectClaudeCode } from "@/features/settings/ConnectClaudeCode";
import { Diagnostics } from "@/features/settings/Diagnostics";
import { SettingsPanel } from "@/features/settings/SettingsPanel";
import { useMonitoringEvents } from "@/hooks/useMonitoringEvents";

export default function App() {
  useMonitoringEvents();
  return (
    <main className="mx-auto max-w-3xl space-y-4 p-4">
      <header>
        <h1 className="text-lg font-semibold">Usage Monitor Maximus</h1>
        <p className="text-xs text-zinc-500">Module 01 · infrastructure preview</p>
      </header>
      <Dashboard />
      <ConnectClaudeCode />
      <SettingsPanel />
      <Diagnostics />
    </main>
  );
}
