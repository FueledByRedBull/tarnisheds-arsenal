#!/bin/bash
# Runs inside a Fedora container: installs the built .rpm, then runs the Linux smoke test on it.
set -euo pipefail

rpms=(src-tauri/target/release/bundle/rpm/*.rpm)
if [ "${#rpms[@]}" -ne 1 ]; then
  echo "expected one .rpm, found ${#rpms[@]}" >&2
  exit 1
fi

dnf install -y --setopt=install_weak_deps=False "${rpms[0]}" /usr/bin/xvfb-run /usr/bin/WebKitWebDriver dejavu-sans-fonts

binary="$(rpm -ql "$(rpm -qp --queryformat '%{NAME}' "${rpms[0]}")" | grep -m1 '^/usr/bin/')"
PATH="/opt/node:$PATH" xvfb-run -a node scripts/smoke-linux.mjs "$binary"
