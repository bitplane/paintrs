#!/usr/bin/env bash
# Run under Xvfb or on an isolated X11 display.
set -euo pipefail

"${1:-target/debug/paintrs}" &
app=$!
trap 'kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true' EXIT

for ((attempt = 0; attempt < 60; attempt++)); do
    kill -0 "$app" 2>/dev/null || { wait "$app"; exit 1; }
    if xwininfo -name "Untitled - Paintrs" >/dev/null 2>&1; then
        echo "Paintrs opened an X11 window"
        exit 0
    fi
    sleep 0.2
done

echo "error: Paintrs did not open an X11 window" >&2
exit 1
