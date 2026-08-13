#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Self-test for tests/verify.py — runnable anywhere, no board needed.

Generates a P-256 key pair, signs a random digest the same way the TA does
(raw r || s over a prehashed SHA-256 value), then requires verify.py to
accept the genuine triple and to reject a tampered digest, a tampered
signature, and a tampered public key. This re-runs, on every invocation,
the mutation test that originally proved verify.py non-vacuous: a verifier
that cannot fail is worse than no verifier.

Exit 0 = all four checks behaved as required.
"""

import json
import os
import secrets
import subprocess
import sys

from cryptography.hazmat.primitives.asymmetric import ec, utils
from cryptography.hazmat.primitives.hashes import SHA256

HERE = os.path.dirname(os.path.abspath(__file__))
VERIFY = os.path.join(HERE, "verify.py")


def verify_rc(payload):
    """Run verify.py on a payload; return its exit code."""
    proc = subprocess.run(
        [sys.executable, VERIFY],
        input=json.dumps(payload).encode(),
        capture_output=True,
    )
    return proc.returncode


def flip_first_byte(hexstr):
    raw = bytearray(bytes.fromhex(hexstr))
    raw[0] ^= 0xFF
    return raw.hex()


def main():
    key = ec.generate_private_key(ec.SECP256R1())
    digest = secrets.token_bytes(32)
    der = key.sign(digest, ec.ECDSA(utils.Prehashed(SHA256())))
    r, s = utils.decode_dss_signature(der)
    nums = key.public_key().public_numbers()

    genuine = {
        "command": "sign",
        "curve": "p256",
        "pubkey": nums.x.to_bytes(32, "big").hex() + nums.y.to_bytes(32, "big").hex(),
        "digest": digest.hex(),
        "signature": r.to_bytes(32, "big").hex() + s.to_bytes(32, "big").hex(),
    }

    cases = [("genuine triple accepted", genuine, True)]
    for field in ("digest", "signature", "pubkey"):
        tampered = dict(genuine)
        tampered[field] = flip_first_byte(tampered[field])
        cases.append((f"tampered {field} rejected", tampered, False))

    failures = 0
    for name, payload, want_ok in cases:
        got_ok = verify_rc(payload) == 0
        status = "PASS" if got_ok == want_ok else "FAIL"
        if got_ok != want_ok:
            failures += 1
        print(f"  {status}  {name}")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
