#!/usr/bin/env bash
# Reproducible screenshots of the 4 UI scenarios against tools/mock_daemon.py.
#   tools/screenshots.sh <path-to-rc_ui> [out_dir] [dark|light]
set -euo pipefail
UI_BIN=${1:?usage: screenshots.sh <rc_ui binary> [out_dir] [theme]}
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=${2:-$HERE/../../docs/screenshots}
THEME=${3:-dark}
SUFFIX=""; [ "$THEME" = light ] && SUFFIX="_light"
mkdir -p "$OUT"
TMP=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$TMP"' EXIT
PORT=18765

# Test config: robot = the mock's fake bridge on localhost; Lichtblick page = the
# local stand-in (tools/fake_lichtblick.html) so the full-mode screenshot shows a view.
cat > "$TMP/cfg.json" <<EOF
{ "ipc": {"socket_path": "$TMP/rc.sock"},
  "robot": {"host": "127.0.0.1", "bridge_port": $PORT, "probe_interval_ms": 500},
  "lichtblick": {"enabled": ${RC_SHOT_LICHTBLICK:-true}, "url": "file://$HERE/fake_lichtblick.html"},
  "net": {"wifi_interface": "", "zerotier_cli": "zerotier-cli-missing-for-test"},
  "ui": {"sound": false, "theme": "$THEME", "font_family": "${RC_SHOT_FONT:-Helvetica Neue}"} }
EOF

shoot() { # name scenario delay_ms [mock extra args]
  local name=$1 scen=$2 delay=$3; shift 3
  python3 "$HERE/mock_daemon.py" --socket "$TMP/rc.sock" --scenario "$scen" --bridge-port $PORT "$@" 2>"$TMP/mock_$name.log" &
  local mpid=$!
  sleep 0.5
  QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-offscreen} "$UI_BIN" --config "$TMP/cfg.json" --windowed --mute --theme "$THEME" \
      --screenshot "$OUT/$name$SUFFIX.png" --screenshot-delay "$delay" 2>&1 | grep -E 'screenshot|rror' || true
  kill $mpid 2>/dev/null || true; wait $mpid 2>/dev/null || true
  cp "$TMP/mock_$name.log" "$OUT/../mock_$name.log" 2>/dev/null || true
  rm -f "$OUT/../mock_$name.log"
}

shoot full full 6000
shoot degraded degraded 3000
shoot menu menu 3000
shoot elrs_lost elrs_lost 3000
shoot menu_system menu 5000 --script 1.0:down,1.1:down,1.2:down,1.3:down,1.4:down,1.5:down,1.6:down,1.7:down,1.8:down,1.9:down,2.0:down,2.1:down,2.2:down,2.3:down
shoot lq_degraded lq_degraded 3000
ls -l "$OUT"
