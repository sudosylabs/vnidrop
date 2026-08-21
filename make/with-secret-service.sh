#!/usr/bin/env bash
set -euo pipefail

if (($# == 0)); then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

for dependency in dbus-run-session gnome-keyring-daemon secret-tool; do
  if ! command -v "$dependency" >/dev/null 2>&1; then
    echo "missing CI Secret Service dependency: $dependency" >&2
    exit 1
  fi
done

if [[ "${VNIDROP_CI_SECRET_SERVICE_SESSION:-}" != "1" ]]; then
  exec dbus-run-session -- env VNIDROP_CI_SECRET_SERVICE_SESSION=1 "$0" "$@"
fi

# GitHub runners may export a control path for a host keyring that is not
# reachable inside the isolated D-Bus session.
unset GNOME_KEYRING_CONTROL GNOME_KEYRING_PID

secret_service_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/vnidrop-secret-service.XXXXXX")"
cleanup() {
  if [[ -n "${GNOME_KEYRING_PID:-}" ]]; then
    kill "$GNOME_KEYRING_PID" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$secret_service_root"
}
trap cleanup EXIT

export XDG_RUNTIME_DIR="$secret_service_root/runtime"
export XDG_DATA_HOME="$secret_service_root/data"
export XDG_CONFIG_HOME="$secret_service_root/config"
keyring_control="$XDG_RUNTIME_DIR/keyring"
mkdir -m 700 "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$keyring_control"

# An empty password is safe for this isolated, ephemeral CI keyring. Unlocking
# creates the default collection that the production Linux adapter requires.
keyring_environment="$(
  printf '\n' | gnome-keyring-daemon \
    --unlock \
    --components=secrets \
    --control-directory="$keyring_control"
)"
eval "$keyring_environment"
export GNOME_KEYRING_CONTROL GNOME_KEYRING_PID

# Fail before a long build if the session bus or default collection is unusable.
printf 'ready' | secret-tool store \
  --label='VniDrop CI Secret Service probe' \
  application com.vnidrop.ci purpose readiness
secret-tool clear application com.vnidrop.ci purpose readiness

"$@"
