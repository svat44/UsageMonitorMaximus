#!/usr/bin/env bash
# Native lifecycle smoke test on Linux (headless via Xvfb). NOT evidence for Windows/macOS.
# Verifies with the real window-event path that: the app starts, the dashboard window can be
# closed without terminating monitoring (hide-to-tray), and an intentional quit exits cleanly.
set -euo pipefail
BIN="${1:-target/debug/usage-monitor-app}"
DATA="$(mktemp -d)"
trap 'rm -rf "$DATA"' EXIT
export USAGE_MONITOR_DATA_DIR="$DATA" USAGE_MONITOR_SMOKE_TEST=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
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
     and r["engineHasSyncedWhileHidden"]
# With a tray the engine must survive the close; without one the app quits instead (by design).
ok = ok and (r["engineRunningAfterClose"] if r["trayAvailable"] else True)
sys.exit(0 if ok else 1)
PY
test -f "$DATA/usage-monitor.sqlite" && echo "database created: ok"
[ "$code" = 0 ] && echo "SMOKE OK (clean exit)"
