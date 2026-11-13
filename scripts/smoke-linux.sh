#!/usr/bin/env bash
# Native lifecycle smoke test on Linux (headless via Xvfb). NOT evidence for Windows/macOS.
# Verifies with the real window-event path that: the app starts, the dashboard window can be
# closed without terminating monitoring (hide-to-tray), and an intentional quit exits cleanly.
set -euo pipefail
BIN="${1:-target/debug/usage-monitor-app}"
DATA="$(mktemp -d)"
trap 'rm -rf "$DATA"' EXIT
mkdir -p "$DATA/home"
export HOME="$DATA/home" XDG_CONFIG_HOME="$DATA/home/.config" USAGE_MONITOR_DATA_DIR="$DATA" USAGE_MONITOR_SMOKE_TEST=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
# Pre-seed via the REAL bridge binary with a fixture that follows the documented status-line schema.
# (A fixture, not a real subscription account: it proves the plumbing, not the numbers.)
BRIDGE="$(dirname "$BIN")/usage-monitor-statusline"
RESET5=$(( $(date +%s) + 7200 )); RESET7=$(( $(date +%s) + 259200 ))
echo "{\"session_id\":\"smoke1\",\"context_window\":{\"used_percentage\":8},\"cost\":{\"total_cost_usd\":0.0123},\"rate_limits\":{\"five_hour\":{\"used_percentage\":23.5,\"resets_at\":$RESET5},\"seven_day\":{\"used_percentage\":41.2,\"resets_at\":$RESET7}}}" \
  | "$BRIDGE" --data-dir "$DATA"
set +e
timeout 60 xvfb-run -a dbus-run-session -- "$BIN" >"$DATA/stdout.log" 2>&1
code=$?
set -e
echo "exit code: $code"
cat "$DATA/smoke-result.json" 2>/dev/null || { echo "no smoke-result.json"; tail -30 "$DATA/stdout.log"; exit 1; }
echo
python3 - "$DATA/smoke-result.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]))
ok = r["visibleBeforeClose"] and r["engineRunningBeforeClose"] and r["windowHiddenAfterClose"] \
     and r["engineHasSyncedWhileHidden"] and r["autostartEnabledAfterEnable"] and r["autostartDisabledAfterDisable"]
# With a tray the engine must survive the close; without one the app quits instead (by design).
ok = ok and (r["engineRunningAfterClose"] if r["trayAvailable"] else True)
sys.exit(0 if ok else 1)
PY
python3 - "$DATA/smoke-ui.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]))
print("UI rendered in webview:", r["rendered"])
assert r["rendered"], r
for needle in ("Claude subscription usage","Local Claude Code activity","Anthropic API consumption"):
    assert needle.lower() in r["text"].lower(), needle
assert "23.5%" in r["text"] and "41.2%" in r["text"], "bridge fixture values did not reach the UI"
assert "not connected yet" not in r["text"], "should not claim 'not connected' when data flows"
print("bridge -> provider -> engine -> event -> React: 23.5% and 41.2% rendered")
print("UI text sample:", " | ".join(r["text"].split("\n")[:14]))
PY
test -f "$DATA/usage-monitor.sqlite" && echo "database created: ok"
[ "$code" = 0 ] && echo "SMOKE OK (clean exit)"
