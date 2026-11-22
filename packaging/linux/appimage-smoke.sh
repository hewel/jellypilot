#!/usr/bin/env bash
# Runs on private Xvfb in a fresh network-disabled runtime container.
set -euo pipefail
[[ $(id -u) != 0 ]]
image=/input/JellyPilot.AppImage
sha256sum "$image"
export HOME=/tmp/home XDG_RUNTIME_DIR=/tmp/runtime LANG=C.UTF-8
mkdir -m 0700 "$HOME" "$XDG_RUNTIME_DIR"
# Only the AppImage transport changes (containers have no FUSE); no app,
# renderer, GPU or backend override. A normal offline first launch must work.
APPIMAGE_EXTRACT_AND_RUN=1 JELLYPILOT_STARTUP_CAPTURE=1 "$image" &
launcher=$!
trap 'kill "$launcher" 2>/dev/null || true' EXIT
for ((i=0; i<45; i++)); do
  if ! kill -0 "$launcher" 2>/dev/null; then
    echo 'AppImage exited before showing its main window' >&2
    exit 1
  fi
  for window in $(xdotool search --onlyvisible --name '^JellyPilot$' 2>/dev/null); do
    pid=$(xdotool getwindowpid "$window") || continue
    [[ $(readlink "/proc/$pid/exe") == */usr/bin/jellypilot ]] || continue
    sleep 3
    kill -0 "$launcher" "$pid"
    xdotool search --onlyvisible --name '^JellyPilot$' | grep -Fx "$window"
    echo "Offline Ubuntu 22.04 startup: JellyPilot window $window mapped, pid=$pid uid=$(id -u)"
    exit 0
  done
  sleep 1
done
echo 'No mapped JellyPilot main window on Ubuntu 22.04' >&2
exit 1
