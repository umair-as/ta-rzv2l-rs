# ta-rzv2l-rs

[![ci](https://github.com/umair-as/ta-rzv2l-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/umair-as/ta-rzv2l-rs/actions/workflows/ci.yml)
[![license: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![rust: stable](https://img.shields.io/badge/rust-stable-orange.svg)](rust-toolchain.toml)

Rust OP-TEE Trusted Applications for the Renesas RZ/V2L, built and verified on real hardware.

A personal learning project. The goal is to understand how Trusted Applications are built and
what a TEE does and does not provide. There is no product, no customer, and no fleet.

Success is measured by what has been **demonstrated and understood**, not by feature count. A
limitation proven on hardware and written down plainly is a better outcome than a feature that
appears to work.

## The device signer

The first application, working end to end on the board since 2026-08-13: one Trusted
Application owns one ECDSA P-256 key pair, generated inside the TEE and never exported — the
interface has no command that returns private material. A caller can read the public key
(`X||Y`, 64 bytes) or have a SHA-256 digest signed (`r||s`, 64 bytes; the TA does not hash).
Signatures are verified on the development host with a different crypto library, which is
stronger evidence than the client checking its own work.

Fourteen board checks pass, including: a signature verifies on the host and *fails* against a
tampered digest; malformed requests sent straight to the TA — bypassing the client — are
rejected by the TA itself with the exact expected error, in a single session that must still
answer a valid request afterward; and the public key is unchanged across invocations and
across a reboot. The TA fails closed on a damaged key object: only "not found" (genuine first
boot) triggers key generation, because silently regenerating over tamper would replace the
device's identity — exactly what an attacker wants to look like a first boot.

Every negative check has been mutation-tested: deliberately weakened TA builds make the checks
fail for the expected reason, so a green run means something.

How the pieces fit together — the two TrustZone worlds, the crates, the key lifecycle, the
build and test flow — is explained in [`docs/application-flow.md`](docs/application-flow.md).
What the signer protects against, per attacker tier, and what it deliberately does not claim,
is in [`docs/security-model.md`](docs/security-model.md). How to watch it run on the board —
and what each tracing tool can and cannot see across the TrustZone boundary — is in
[`docs/observing-the-ta.md`](docs/observing-the-ta.md).

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
the vendored tree's own README. The migration from the previous March-2024 SDK pin was
accepted only after the full board smoke test passed against this build.

## Building and testing

The dev loop is `scp`, not a Yocto image rebuild — seconds instead of an hour. Yocto packaging
is the production path and a separate, later concern.

Machine-specific paths go in an untracked `local.mk` at the repo root:

```make
TA_DEV_KIT_DIR      := <path to OP-TEE export-ta_arm64 from the board's exact build>
OPTEE_CLIENT_EXPORT := <dir containing usr/lib/libteec.so for cross-linking>
BOARD_HOST          := <board ip or name>          # optional; or pass on the command line
```

```sh
make                                  # build the signed .ta + on-board programs
make deploy BOARD_HOST=<ip-or-name>   # install them on the board (needs sudo over ssh)
make test   BOARD_HOST=<ip-or-name>   # run the board test; REBOOT=1 adds the reboot check
make lint                             # fmt + clippy; the TA forbids unwrap/expect/panic
```

Both the OP-TEE TA signing script and the host-side verifier need the Python `cryptography`
package. If a virtual environment is active, confirm that its `python3` can import the package
(the build and test both preflight this and say so plainly if not). Manual verification is one
pipe:

```sh
ssh <user>@<board> signer-client sign $(head -c 32 /dev/urandom | sha256sum | cut -c1-64) \
    | tests/verify.py
```

CI runs the subset of this that needs no hardware: formatting for every crate, clippy and
builds for the crates that don't require the board's dev kit, and a self-test of the verifier
(genuine signature accepted; tampered digest, signature, and public key each rejected). CI
green is not acceptance — the board test is.

## Platform notes that shape the design

**OP-TEE's TZDRAM is not isolated from normal-world root on the current board image.** Three
read-only `devmem2` probes at the OP-TEE TZDRAM base returned OP-TEE instructions instead of a
bus fault. TF-A initializes TZC-400 but, with `TRUSTED_BOARD_BOOT=0`, does not add the secure-only
DDR regions. Consequently, “never exported” describes the TA command interface; this project
does not claim private-key confidentiality against local root. The exact addresses, control test
and scope of that conclusion are recorded in [`docs/security-model.md`](docs/security-model.md).

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

The security consequences are summarized in [`docs/security-model.md`](docs/security-model.md).

## What comes next

Two independent mechanisms are the natural continuations, and each will be claimed only when
demonstrated on this board: a hardware-backed key (Renesas Secure IP) behind the same
two-command interface, and rollback-resistant storage via the eMMC's RPMB partition. Neither
changes the client or the protocol.

## License

Apache-2.0. See `LICENSE` and `NOTICE`.

Any keys or fixtures that appear in this repo are development material and are
NOT-FOR-PRODUCTION.
