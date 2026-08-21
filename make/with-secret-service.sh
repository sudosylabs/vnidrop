#!/usr/bin/env bash
set -euo pipefail

if (($# == 0)); then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

for dependency in dbus-run-session dbus-send gnome-keyring-daemon secret-tool; do
  if ! command -v "$dependency" >/dev/null 2>&1; then
    echo "missing CI Secret Service dependency: $dependency" >&2
    exit 1
  fi
done

# GitHub runners may export a control path for a host keyring that is not
# reachable inside the isolated D-Bus session.
unset GNOME_KEYRING_CONTROL GNOME_KEYRING_PID

if [[ "${VNIDROP_CI_SECRET_SERVICE_SESSION:-}" != "1" ]]; then
  exec dbus-run-session -- env VNIDROP_CI_SECRET_SERVICE_SESSION=1 "$0" "$@"
fi

secret_service_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/vnidrop-secret-service.XXXXXX")"
cleanup() {
  rm -rf -- "$secret_service_root"
}
trap cleanup EXIT

export XDG_RUNTIME_DIR="$secret_service_root/runtime"
export XDG_DATA_HOME="$secret_service_root/data"
export XDG_CONFIG_HOME="$secret_service_root/config"
mkdir -m 700 "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME"

# A headless runner cannot answer the GUI prompt used to create a login
# collection. Its unlocked session collection is ephemeral and safe for CI.
session_reply="$(
  dbus-send --session --print-reply \
    --dest=org.freedesktop.secrets \
    /org/freedesktop/secrets \
    org.freedesktop.Secret.Service.ReadAlias \
    string:session
)"
session_collection="$(printf '%s\n' "$session_reply" | sed -n 's/.*object path "\([^"]*\)"/\1/p')"
if [[ -z "$session_collection" || "$session_collection" == "/" ]]; then
  echo "CI Secret Service did not expose a session collection" >&2
  exit 1
fi
dbus-send --session --print-reply \
  --dest=org.freedesktop.secrets \
  /org/freedesktop/secrets \
  org.freedesktop.Secret.Service.SetAlias \
  string:default \
  objpath:"$session_collection" >/dev/null

# Fail before a long build if the session bus or default collection is unusable.
printf 'ready' | secret-tool store \
  --label='VniDrop CI Secret Service probe' \
  application com.vnidrop.ci purpose readiness
secret-tool clear application com.vnidrop.ci purpose readiness

"$@"
