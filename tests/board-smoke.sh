#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Board smoke test for the signer TA. Runs on the host, drives the board over
# ssh, and verifies every signature locally with a different crypto library.
#
# Checks are labeled by the boundary they actually exercise:
#   CLI  = signer-client argument validation (rejected before any TEE call)
#   TA   = direct malformed invocations sent to the TA via ta-probe,
#          deliberately bypassing the CLI's validation
#
#   BOARD_HOST=<ip> tests/board-smoke.sh
#   BOARD_HOST=<ip> REBOOT=1 tests/board-smoke.sh     # also test persistence
#   PYTHON=/usr/bin/python3 ... to pick the interpreter used for verification
#
# Exit 0 = all checks pass.

set -eu

BOARD_USER="${BOARD_USER:-devel}"
BOARD_HOST="${BOARD_HOST:-}"
CLIENT="${CLIENT:-/usr/local/bin/signer-client}"
PROBE="${PROBE:-/usr/local/bin/ta-probe}"
SSH="${SSH:-ssh}"
REBOOT="${REBOOT:-0}"
PYTHON="${PYTHON:-python3}"

HERE=$(dirname "$0")
pass=0
fail=0

if [ -z "$BOARD_HOST" ]; then
	echo "BOARD_HOST is not set" >&2
	exit 2
fi

# Fail early and clearly if the chosen interpreter cannot verify signatures;
# otherwise a venv without `cryptography` misreports as "signature failed".
if ! "$PYTHON" -c 'import cryptography' 2>/dev/null; then
	echo "error: $PYTHON cannot import 'cryptography' (needed by tests/verify.py)." >&2
	echo "       Deactivate the venv or rerun with PYTHON=/usr/bin/python3" >&2
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
	pub1=$(printf '%s' "$pub_json" | "$PYTHON" -c 'import json,sys; print(json.load(sys.stdin)["pubkey"])')
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
	if verdict=$(printf '%s' "$sig_json" | "$PYTHON" "$HERE/verify.py" 2>&1); then
		ok "signature verified on the host against the TA's public key"
	else
		bad "signature did NOT verify"
		printf '  verifier said: %s\n' "$verdict"
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
tampered=$(printf '%s' "$sig_json" | "$PYTHON" -c \
	"import json,sys; d=json.load(sys.stdin); d['digest']='$other'; print(json.dumps(d))")
if printf '%s' "$tampered" | "$PYTHON" "$HERE/verify.py" >/dev/null 2>&1; then
	bad "verifier accepted a signature over a DIFFERENT digest"
else
	ok "signature is bound to its digest (tampered digest rejected)"
fi

# --- 5. CLI input validation (never reaches the TA) -----------------------
if board "$CLIENT sign deadbeef" >/dev/null 2>&1; then
	bad "CLI accepted a short digest"
else
	ok "CLI rejects a short digest"
fi

if board "$CLIENT bogus-command" >/dev/null 2>&1; then
	bad "CLI accepted an unknown command"
else
	ok "CLI rejects an unknown command"
fi

# --- 6. TA boundary: direct malformed invocations via ta-probe ------------
# ta-probe runs the whole sequence in ONE process and ONE session and ends
# with a valid request in that same session, so a TA crash cannot hide
# behind a freshly loaded instance. A malformed case passes only if the TA
# itself (error origin TA) returned the exact expected error code.
if board "test -x $PROBE"; then
	probe_out=$(board "$PROBE" 2>/dev/null) || true
	for case in sign-short-digest unknown-command pubkey-wrong-direction \
			sign-wrong-direction pubkey-extra-param; do
		if printf '%s\n' "$probe_out" | grep -q "^PASS $case:"; then
			ok "TA rejects $case with the exact expected error (same session)"
		else
			bad "TA boundary check failed: $case"
			printf '%s\n' "$probe_out" | grep "^FAIL $case:" || true
		fi
	done
	if printf '%s\n' "$probe_out" | grep -q "^PASS valid-after:"; then
		ok "TA still functional in the same session after malformed invocations"
	else
		bad "TA not functional after malformed invocations (same session)"
		printf '%s\n' "$probe_out" | grep "^FAIL valid-after:" || true
	fi
else
	bad "ta-probe not installed - run 'make deploy' first"
fi

# --- 7. same key across TA restarts --------------------------------------
pub2=$(board "$CLIENT pubkey" 2>/dev/null | "$PYTHON" -c 'import json,sys; print(json.load(sys.stdin)["pubkey"])')
if [ "$pub1" = "$pub2" ]; then
	ok "same public key across invocations"
else
	bad "public key changed between invocations"
fi

# --- 8. persistence across reboot (opt-in) -------------------------------
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
		pub3=$(board "$CLIENT pubkey" 2>/dev/null | "$PYTHON" -c 'import json,sys; print(json.load(sys.stdin)["pubkey"])')
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
