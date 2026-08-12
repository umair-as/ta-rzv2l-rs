# ta-rzv2l-rs

Rust OP-TEE Trusted Applications for the Renesas RZ/V2L, built and verified on real hardware.

A personal learning project. The goal is to understand how Trusted Applications are built and
what a TEE does and does not provide. There is no product, no customer, and no fleet.

Success is measured by what has been **demonstrated and understood**, not by feature count. A
limitation proven on hardware and written down plainly is a better outcome than a feature that
appears to work.

## Status

Scaffolding only. No Trusted Application has been written in this repo yet.

**Milestone 1 — device signer.** One TA owning one ECDSA P-256 key pair generated inside the
TEE and never exported. Two commands: read the public key, sign a caller-supplied SHA-256
digest. Signatures are verified on the host with a different crypto library, which is stronger
evidence than the client checking its own work.

## Layout

```
third_party/teaclave-trustzone-sdk/   vendored + patched Apache Teaclave SDK
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

**The vendored SDK is patched.** Two nightly-only gates were removed ABI-safely so the TAs
build on **stable** rustc rather than a pinned nightly. That patch is the reason this repo
works at all — do not replace `third_party/teaclave-trustzone-sdk/` with upstream.

## Development loop

The dev loop is `scp`, not a Yocto image rebuild — seconds instead of an hour. Yocto packaging
is the production path and a separate, later concern.

```sh
make                                  # build TA + client
make deploy BOARD_HOST=<ip-or-name>   # scp both to the board
make test   BOARD_HOST=<ip-or-name>   # run the smoke test
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

Fuller cited detail is in `scratch/CAPABILITY-MAP-2026-08-12.md`.

## History

This repo replaces an earlier one that accumulated a 53-requirement specification framework
across four deliverables with roughly a dozen executable checks — more scaffolding than was
useful, and written in a product register that did not match what the project is. That work is
preserved in a frozen archive alongside this directory; nothing was lost.

## License

Apache-2.0. See `LICENSE` and `NOTICE`.

Any keys or fixtures that appear in this repo are development material and are
NOT-FOR-PRODUCTION.
