# ta-rzv2l-rs

Rust OP-TEE Trusted Applications for the Renesas RZ/V2L, built and verified on real hardware.

A personal learning project. The goal is to understand how Trusted Applications are built and
what a TEE does and does not provide. There is no product, no customer, and no fleet.

Success is measured by what has been **demonstrated and understood**, not by feature count. A
limitation proven on hardware and written down plainly is a better outcome than a feature that
appears to work.

## Status

**Milestone 1 — device signer: done, verified on hardware (2026-08-13).** One TA owning one
ECDSA P-256 key pair generated inside the TEE and never exported — the interface has no
command that returns private material. Two commands: read the public key (`X||Y`, 64 bytes),
sign a caller-supplied SHA-256 digest (`r||s`, 64 bytes; the TA does not hash). Signatures
are verified on the host with a different crypto library, which is stronger evidence than the
client checking its own work.

All fourteen board smoke checks pass, including: signature verifies on the host and *fails*
against a tampered digest; the client rejects short digests and unknown commands; direct
malformed TA invocations (short digest, unknown command ID, wrong parameter directions, extra
parameters) sent by an on-board probe that bypasses the client are rejected by the TA itself,
which stays functional afterward; and the public key is unchanged across invocations and
across a board reboot. The test boundary is described precisely in `docs/application-flow.md`.

The TA fails closed on a damaged key object: only `ITEM_NOT_FOUND` (genuine first boot)
triggers key generation. Anything else is an error — silently regenerating over tamper would
replace the device's identity, which is exactly what an attacker wants to look like a first
boot.

## Layout

For a newcomer-oriented explanation of the Rust crates, execution environments, key lifecycle,
and build/test path, see [`docs/application-flow.md`](docs/application-flow.md).

```
docs/application-flow.md              architecture and development walkthrough
docs/security-model.md                claims, attacker tiers, and platform limits
signer/proto/                         shared protocol: UUID, command IDs, wire sizes
signer/ta/                            the Trusted Application (no_std)
signer/host/                          signer-client CLI + ta-probe TA-boundary tester
signer/uuid.txt                       single source of truth for the TA UUID
third_party/teaclave-trustzone-sdk/   vendored Apache Teaclave SDK v0.7.0 (see its README)
tests/verify.py                       independent host-side signature verification
tests/board-smoke.sh                  end-to-end board test
scratch/                              session notes and handoffs (not tracked)
```

## Toolchain and platform pins

Everything here targets one specific build. Using a devkit from a different OP-TEE build than
the one running on the board risks subtle ABI mismatches.

| | |
|---|---|
| OP-TEE | `renesas-rz/rzg_optee-os`, branch `4.8.0/rz`, SRCREV `82a5cd3` |
| Platform | `PLATFORM_FLAVOR=g2l_smarc_2` |
| optee-client | `4.9.0` |
| Rust | stable, target `aarch64-unknown-linux-gnu` (see `rust-toolchain.toml`) |
| Cross toolchain | `aarch64-linux-gnu-` |
| Board | Renesas RZ/V2L SMARC EVK, `r9a07g054l2`, security-capable SKU, glibc 2.43 |

**The vendored SDK is upstream v0.7.0 with a one-line patch.** v0.7.0 is the Teaclave release
aligned with OP-TEE 4.8.0 — the OP-TEE version this board runs. One nightly-only feature gate
(`error_in_core`, stable since Rust 1.81) is removed so the TAs build on **stable** rustc;
everything else is byte-identical to the upstream tag. Provenance and the exact diff are in
`third_party/teaclave-trustzone-sdk/README.md`. The migration from the previous March-2024
SDK pin was accepted only after the full board smoke test passed against this build.

## Development loop

The dev loop is `scp`, not a Yocto image rebuild — seconds instead of an hour. Yocto packaging
is the production path and a separate, later concern.

Machine-specific paths go in an untracked `local.mk` at the repo root:

```make
TA_DEV_KIT_DIR      := <path to OP-TEE export-ta_arm64 from the board's exact build>
OPTEE_CLIENT_EXPORT := <dir containing usr/lib/libteec.so for cross-linking>
BOARD_HOST          := <board ip or name>          # optional; or pass on the command line
```

```sh
make                                  # build the signed .ta + signer-client
make deploy BOARD_HOST=<ip-or-name>   # install both on the board (needs sudo over ssh)
make test   BOARD_HOST=<ip-or-name>   # run the smoke test; REBOOT=1 adds the reboot check
```

Both the OP-TEE TA signing script and `tests/verify.py` need the Python `cryptography` package.
If a virtual environment is active, confirm that its `python3` can import the package. Manual
verification is one pipe:

```sh
ssh <user>@<board> signer-client sign $(head -c 32 /dev/urandom | sha256sum | cut -c1-64) \
    | tests/verify.py
```

## Platform notes that shape the design

**Secure storage on this board provides confidentiality and integrity, but not freshness.**
With `CFG_RPMB_FS=n`, normal-world root can delete or roll back `/var/lib/tee`. Proven on
hardware: a counter held in secure storage was reset to zero by `rm`, and moved backwards by
restoring a snapshot. The cause is in OP-TEE's source — `plat-rz` does not implement the
non-volatile counter hooks, and `CFG_INSECURE=y` lets the missing counter default to zero.

Practical consequence for anything built here: a stored key is durable against accident, not
against a privileged local attacker. Deleting secure storage makes a TA generate a *new* key.

**The board's `tee-supplicant` is built with `RPMB_EMU=1`.** Enabling `CFG_RPMB_FS=y` today
would silently bind to a volatile fake RPMB that looks like it works.

**The packaged `optee-os-tadevkit` is built without `CFG_RZ_SCE`.** Harmless for TAs using only
the GP Internal Core API; it blocks any Renesas Secure IP work until its bbappend mirrors the
`ENABLE_RZ_SCE` block from `optee-os_%.bbappend`.

The security consequences are summarized in `docs/security-model.md`.


## License

Apache-2.0. See `LICENSE` and `NOTICE`.

Any keys or fixtures that appear in this repo are development material and are
NOT-FOR-PRODUCTION.
