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
exit 0
EOF

cat >"$stub_bin/dbus-send" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${GNOME_KEYRING_CONTROL:-}" || -n "${GNOME_KEYRING_PID:-}" ]]; then
  printf 'dbus-send received inherited keyring state\n' >&2
  exit 89
fi

case " $* " in
  *" org.freedesktop.Secret.Service.ReadAlias "*)
    [[ " $* " == *" string:session "* ]]
    printf 'method return\n   object path "/org/freedesktop/secrets/collection/session"\n'
    ;;
  *" org.freedesktop.Secret.Service.SetAlias "*)
    [[ " $* " == *" string:default "* ]]
    [[ " $* " == *" objpath:/org/freedesktop/secrets/collection/session "* ]]
    touch "$VNIDROP_SECRET_SERVICE_TEST_STATE/default-alias-ready"
    printf 'method return\n'
    ;;
  *)
    exit 91
    ;;
esac
EOF

cat >"$stub_bin/secret-tool" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ ! -f "$VNIDROP_SECRET_SERVICE_TEST_STATE/default-alias-ready" ]]; then
  printf 'default Secret Service collection is missing\n' >&2
  exit 92
fi
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

run_wrapper() {
  rm -f \
    "$state_dir/default-alias-ready" \
    "$state_dir/probe-stored" \
    "$state_dir/probe-cleared" \
    "$state_dir/command-ran"

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
}

run_wrapper
run_wrapper
