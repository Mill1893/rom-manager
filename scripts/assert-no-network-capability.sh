#!/usr/bin/env bash
#
# Asserts that the default build cannot reach the network.
#
#     scripts/assert-no-network-capability.sh
#
# The privacy section of docs/validation/windows-sync-core/milestone-report.md
# claims the application "cannot reach the network, because nothing in it can
# open a socket". That is the strongest claim this project makes about itself,
# and until now it was prose: verified by hand on 2026-08-06 and never checked
# again. Its own figures had already drifted — it cited 123 passing tests
# against a suite that is now 513.
#
# A property nobody re-checks is a property that will be wrong eventually, and
# this one has a specific way of going wrong: `cargo add` on a crate that
# quietly depends on an HTTP client. Nothing in the tree today does, and the
# point of this script is that the day one does, the build says so.
#
# It deliberately does not try to prove a negative by cleverness. It checks
# four concrete things, each of which is how the property would actually be
# lost in practice.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Crates that can open a socket, or that exist to help something else do it.
# Not exhaustive, and cannot be — see the closing note about what this does
# not establish.
DENIED='ureq|reqwest|hyper|curl|isahc|attohttpc|surf|awc|tokio|async-std|smol|mio|quinn|h2|h3|rustls|native-tls|openssl|websocket|tungstenite|trust-dns|hickory'

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=1; }

# 1. The default dependency tree. This is the one that ships.
echo "== default feature set =="
DEFAULT_TREE="$(cargo tree --edges normal --prefix none --format '{p}' 2>/dev/null | sort -u)"
FOUND="$(printf '%s\n' "$DEFAULT_TREE" | grep -iE "^($DENIED) " || true)"
if [[ -n "$FOUND" ]]; then
  fail "network-capable crates in the default tree:"
  printf '%s\n' "$FOUND" | sed 's/^/        /' >&2
else
  pass "no network-capable crate in the default tree"
fi

# 2. The same detector, run where the answer is known to be yes.
#
#    Without this, a typo in the pattern above would make check 1 pass forever
#    while testing nothing. `provider-http` is the deliberate, opt-in door to
#    the network (#30), so it is the natural control: if the detector cannot
#    see ureq when ureq is definitely there, it cannot be trusted when it says
#    nothing is there.
echo "== provider-http feature set (control) =="
PROVIDER_TREE="$(cargo tree --edges normal --prefix none --format '{p}' --features provider-http 2>/dev/null | sort -u)"
if printf '%s\n' "$PROVIDER_TREE" | grep -qiE "^ureq "; then
  pass "the detector finds ureq when the opt-in feature is enabled"
else
  fail "the detector did not find ureq under --features provider-http, so check 1 proves nothing"
fi

# 3. Socket APIs written directly, which need no dependency at all.
echo "== application source =="
SOCKETS="$(grep -rnE 'std::net|TcpStream|TcpListener|UdpSocket|SocketAddr' src/ tauri/src/ 2>/dev/null || true)"
if [[ -n "$SOCKETS" ]]; then
  fail "socket APIs in application source:"
  printf '%s\n' "$SOCKETS" | sed 's/^/        /' >&2
else
  pass "no socket API in src/ or tauri/src/"
fi

# 4. The frontend. React and the test tools are dev-only; a runtime dependency
#    would ship inside the WebView, where the capability file cannot reach it.
echo "== frontend runtime dependencies =="
UI_DEPS="$(python3 -c 'import json; d=json.load(open("ui/package.json")).get("dependencies") or {}; print(" ".join(sorted(d)))')"
if [[ -n "$UI_DEPS" ]]; then
  fail "ui/package.json declares runtime dependencies: $UI_DEPS"
else
  pass "ui/package.json declares no runtime dependency"
fi

echo
if [[ "$FAILED" -ne 0 ]]; then
  echo "The default build may now be able to reach the network." >&2
  echo "If that is intended, it belongs behind an opt-in feature like" >&2
  echo "provider-http, and the privacy section of the milestone report has to" >&2
  echo "be rewritten to say so." >&2
  exit 1
fi

echo "the default build has no way to open a socket"
