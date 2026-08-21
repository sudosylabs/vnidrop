#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/vnidrop-secret-service-test.XXXXXX")"
trap 'rm -rf -- "$test_root"' EXIT

stub_bin="$test_root/bin"
state_dir="$test_root/state"
mkdir -p "$stub_bin" "$state_dir"

cat >"$stub_bin/dbus-run-session" <<'EOF'
#!/usr/bin/env bash
exit 90
EOF

cat >"$stub_bin/gnome-keyring-daemon" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${GNOME_KEYRING_CONTROL:-}" || -n "${GNOME_KEYRING_PID:-}" ]]; then
  printf 'gnome-keyring-daemon received inherited state\n' >&2
  exit 86
fi

expected="--control-directory=$XDG_RUNTIME_DIR/keyring"
found=false
for argument in "$@"; do
  if [[ "$argument" == "$expected" ]]; then
    found=true
    break
  fi
done
if [[ "$found" != "true" ]]; then
  printf 'gnome-keyring-daemon did not receive isolated control directory\n' >&2
  exit 87
fi

touch "$VNIDROP_SECRET_SERVICE_TEST_STATE/daemon-started"
printf 'GNOME_KEYRING_CONTROL=%q\n' "$XDG_RUNTIME_DIR/keyring"
printf 'GNOME_KEYRING_PID=%q\n' 999999
EOF

cat >"$stub_bin/secret-tool" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

test -f "$VNIDROP_SECRET_SERVICE_TEST_STATE/daemon-started"
case "${1:-}" in
  store)
    secret="$(cat)"
    [[ "$secret" == "ready" ]]
    touch "$VNIDROP_SECRET_SERVICE_TEST_STATE/probe-stored"
    ;;
  clear)
    touch "$VNIDROP_SECRET_SERVICE_TEST_STATE/probe-cleared"
    ;;
  *)
    exit 88
    ;;
esac
EOF

cat >"$stub_bin/vnidrop-test-command" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
touch "$VNIDROP_SECRET_SERVICE_TEST_STATE/command-ran"
EOF

chmod +x "$stub_bin"/*

PATH="$stub_bin:$PATH" \
  RUNNER_TEMP="$test_root" \
  VNIDROP_CI_SECRET_SERVICE_SESSION=1 \
  VNIDROP_SECRET_SERVICE_TEST_STATE="$state_dir" \
  GNOME_KEYRING_CONTROL=/run/user/1001/keyring \
  GNOME_KEYRING_PID=12345 \
  "$ROOT/make/with-secret-service.sh" vnidrop-test-command

test -f "$state_dir/probe-stored"
test -f "$state_dir/probe-cleared"
test -f "$state_dir/command-ran"
