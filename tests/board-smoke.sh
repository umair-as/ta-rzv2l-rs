#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Board smoke test for the signer TA. Runs on the host, drives the board over
# ssh, and verifies every signature locally with a different crypto library.
#
#   BOARD_HOST=<ip> tests/board-smoke.sh
#   BOARD_HOST=<ip> REBOOT=1 tests/board-smoke.sh     # also test persistence
#
# Exit 0 = all checks pass.

set -eu

BOARD_USER="${BOARD_USER:-devel}"
BOARD_HOST="${BOARD_HOST:-}"
CLIENT="${CLIENT:-/usr/local/bin/signer-client}"
SSH="${SSH:-ssh}"
REBOOT="${REBOOT:-0}"

HERE=$(dirname "$0")
pass=0
fail=0

if [ -z "$BOARD_HOST" ]; then
	echo "BOARD_HOST is not set" >&2
	exit 2
fi

board() { $SSH "${BOARD_USER}@${BOARD_HOST}" "$@"; }

ok()   { echo "  PASS  $1"; pass=$((pass + 1)); }
bad()  { echo "  FAIL  $1"; fail=$((fail + 1)); }

echo "signer smoke test against ${BOARD_USER}@${BOARD_HOST}"

# --- 1. TA is installed -------------------------------------------------
uuid=$(cat "$HERE/../signer/uuid.txt")
if board "test -f /lib/optee_armtz/${uuid}.ta"; then
	ok "TA present at /lib/optee_armtz/${uuid}.ta"
else
	bad "TA not installed - run 'make deploy' first"
	exit 1
fi

# --- 2. public key ------------------------------------------------------
if pub_json=$(board "$CLIENT pubkey" 2>/dev/null); then
	pub1=$(printf '%s' "$pub_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pubkey"])')
	if [ "${#pub1}" -eq 128 ]; then
		ok "public key retrieved (${#pub1} hex chars)"
	else
		bad "public key is ${#pub1} hex chars, expected 128"
	fi
else
	bad "pubkey command failed"
	exit 1
fi

# --- 3. sign and verify -------------------------------------------------
digest=$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')
if sig_json=$(board "$CLIENT sign $digest" 2>/dev/null); then
	if printf '%s' "$sig_json" | python3 "$HERE/verify.py" >/dev/null 2>&1; then
		ok "signature verified on the host against the TA's public key"
	else
		bad "signature did NOT verify"
		printf '%s\n' "$sig_json"
	fi
else
	bad "sign command failed"
fi

# --- 4. the signature is over THIS digest, not just any ------------------
# Flip the first byte of the digest and confirm verification now fails.
# Without this, a verifier that ignores the digest would look like a pass.
other=$(printf '%s' "$digest" | sed 's/^../ff/')
if [ "$other" = "$digest" ]; then other=$(printf '%s' "$digest" | sed 's/^../00/'); fi
tampered=$(printf '%s' "$sig_json" | python3 -c \
	"import json,sys; d=json.load(sys.stdin); d['digest']='$other'; print(json.dumps(d))")
if printf '%s' "$tampered" | python3 "$HERE/verify.py" >/dev/null 2>&1; then
	bad "verifier accepted a signature over a DIFFERENT digest"
else
	ok "signature is bound to its digest (tampered digest rejected)"
fi

# --- 5. malformed input is rejected without killing the TA ---------------
if board "$CLIENT sign deadbeef" >/dev/null 2>&1; then
	bad "short digest was accepted"
else
	ok "short digest rejected"
fi

if board "$CLIENT bogus-command" >/dev/null 2>&1; then
	bad "unknown command was accepted"
else
	ok "unknown command rejected"
fi

# The TA must still work after being fed bad input.
if board "$CLIENT pubkey" >/dev/null 2>&1; then
	ok "TA still responsive after malformed input"
else
	bad "TA stopped responding after malformed input"
fi

# --- 6. same key across TA restarts --------------------------------------
pub2=$(board "$CLIENT pubkey" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["pubkey"])')
if [ "$pub1" = "$pub2" ]; then
	ok "same public key across invocations"
else
	bad "public key changed between invocations"
fi

# --- 7. persistence across reboot (opt-in) -------------------------------
if [ "$REBOOT" = "1" ]; then
	echo "  ....  rebooting the board (REBOOT=1)"
	board "sudo systemctl reboot" >/dev/null 2>&1 || true
	sleep 10
	n=0
	while [ "$n" -lt 30 ]; do
		if board "true" >/dev/null 2>&1; then break; fi
		sleep 5
		n=$((n + 1))
	done
	if board "true" >/dev/null 2>&1; then
		pub3=$(board "$CLIENT pubkey" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["pubkey"])')
		if [ "$pub1" = "$pub3" ]; then
			ok "same public key after reboot (key is persistent)"
		else
			bad "public key changed after reboot"
		fi
	else
		bad "board did not come back after reboot"
	fi
else
	echo "  SKIP  reboot persistence (set REBOOT=1 to include)"
fi

echo
echo "${pass} passed, ${fail} failed"
[ "$fail" -eq 0 ]
