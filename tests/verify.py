#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Independently verify a signature produced by the signer TA.

Reads the client's JSON on stdin. Deliberately uses a different crypto library
on a different machine than the one that produced the signature - if this
passes, the TA really did produce a valid ECDSA P-256 signature over the
digest, using the public key it claims to hold.

Exit 0 = verified, 1 = rejected, 2 = bad input.
"""

import json
import sys

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric import ec, utils
from cryptography.hazmat.primitives.hashes import SHA256


def fail(msg, code=2):
    print(f"FAIL: {msg}", file=sys.stderr)
    sys.exit(code)


def main():
    try:
        data = json.load(sys.stdin)
    except json.JSONDecodeError as exc:
        fail(f"stdout was not valid JSON: {exc}")

    for field in ("pubkey", "digest", "signature"):
        if field not in data:
            fail(f"missing field {field!r} (is this 'sign' output?)")

    try:
        pubkey = bytes.fromhex(data["pubkey"])
        digest = bytes.fromhex(data["digest"])
        sig = bytes.fromhex(data["signature"])
    except ValueError as exc:
        fail(f"malformed hex: {exc}")

    if len(pubkey) != 64:
        fail(f"pubkey is {len(pubkey)} bytes, expected 64 (X||Y)")
    if len(digest) != 32:
        fail(f"digest is {len(digest)} bytes, expected 32")
    if len(sig) != 64:
        fail(f"signature is {len(sig)} bytes, expected 64 (r||s)")

    x = int.from_bytes(pubkey[:32], "big")
    y = int.from_bytes(pubkey[32:], "big")
    r = int.from_bytes(sig[:32], "big")
    s = int.from_bytes(sig[32:], "big")

    try:
        key = ec.EllipticCurvePublicNumbers(x, y, ec.SECP256R1()).public_key()
    except ValueError as exc:
        # A point not on the curve means the TA returned something wrong -
        # for example unpadded coordinates.
        fail(f"public point is not on P-256: {exc}", 1)

    try:
        key.verify(
            utils.encode_dss_signature(r, s),
            digest,
            ec.ECDSA(utils.Prehashed(SHA256())),
        )
    except InvalidSignature:
        fail("signature did NOT verify", 1)

    print("OK: signature verified against the TA's public key")
    return 0


if __name__ == "__main__":
    sys.exit(main())
