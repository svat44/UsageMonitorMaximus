import { useQuery } from "@tanstack/react-query";
import { api } from "@/services/tauri";

export function ConnectClaudeCode() {
  const setup = useQuery({ queryKey: ["statusline-setup"], queryFn: api.getStatuslineSetup });
  if (!setup.data) return null;
  const d = setup.data;
  return (
    <section aria-labelledby="connect" className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4">
      <h2 id="connect" className="text-sm font-semibold uppercase tracking-wide text-zinc-300">Connect Claude Code</h2>
      <p className="mt-1 text-xs text-zinc-500">
        Claude Code's documented status-line feature hands usage data to a command you configure. Add this to your own
        <code className="mx-1 rounded bg-zinc-800 px-1">~/.claude/settings.json</code>
        (this app never edits it and never reads your Claude credentials).
      </p>
      {!d.bridgePath && (
        <p className="mt-2 text-xs text-amber-300">The bridge executable was not found next to the app; replace the placeholder path below.</p>
      )}
      <pre className="mt-3 overflow-x-auto rounded-md bg-zinc-950 p-3 text-xs text-zinc-300" data-testid="settings-snippet">{d.settingsSnippet}</pre>
      <p className="mt-2 text-xs text-zinc-500">Data directory: <span className="font-mono">{d.dataDir}</span></p>
    </section>
  );
}
