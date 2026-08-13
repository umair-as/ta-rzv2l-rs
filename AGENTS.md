# Agent instructions — ta-rzv2l-rs

Rust OP-TEE Trusted Applications for the Renesas RZ/V2L SMARC EVK, built and verified on real
hardware. A personal learning project: success is what has been **demonstrated and
understood**, not feature count. A limitation proven on hardware and written down plainly
beats a feature that appears to work. Never write in a product/fleet register and never claim
security properties the platform notes below don't support.

Session context: if `scratch/` exists (gitignored, local only), read its newest handoff/
evaluation notes before starting work — decisions recorded there are settled; do not
re-litigate them.

## Build, deploy, test

Machine-specific paths live in an untracked `local.mk` at the repo root
(`TA_DEV_KIT_DIR`, `OPTEE_CLIENT_EXPORT`, `BOARD_HOST`, optional `SSH`/`SCP` overrides).
Board addresses and credentials belong there or in `scratch/` — never in tracked files.

```sh
make                                  # signed .ta + signer-client (cross, stable Rust)
make deploy BOARD_HOST=<ip>           # scp + sudo install onto the board
make test   BOARD_HOST=<ip>           # tests/board-smoke.sh; REBOOT=1 adds the reboot check
make lint                             # cargo fmt --check + clippy (strict lint set for the TA)
```

Run `make lint` before considering any Rust change done. The TA crate is gated on
`clippy::unwrap_used` / `expect_used` / `panic` — secure-world code must return errors, not
panic.

CI (`.github/workflows/ci.yml`) covers only what a runner can honestly check: fmt for all
crates, clippy/build for the crates that need no board artifacts, and the `verify.py`
self-test. CI green is **not** acceptance — the board smoke test is.

- The dev loop is `scp`, never a Yocto image rebuild (seconds vs an hour).
- Don't run bare `cargo build` in `signer/ta/` — the TA needs `TA_DEV_KIT_DIR`,
  `RUSTFLAGS="-C panic=abort"`, and the sign step; go through the Makefiles.
- Manual end-to-end check:
  `ssh <user>@<board> signer-client sign <64-hex> | tests/verify.py`
  (host needs the Python `cryptography` package).
- Mutation-test every new check before trusting it: make it fail for the right reason first
  (tamper the signature, digest, and pubkey; confirm all three are rejected). A green check
  that cannot fail is worse than no check.

## Architecture

TrustZone splits the board into two worlds. Normal world (Linux) runs `signer-client`
(no secrets, a messenger over `libteec`) and `tee-supplicant` (loads TA files from
`/lib/optee_armtz/`, persists encrypted secure-storage blobs to `/var/lib/tee/`). Secure
world runs OP-TEE OS plus the TA — the only place private key material exists in plaintext.

A newcomer-oriented walkthrough (crates, execution environments, key lifecycle, build/test
path) lives in `docs/application-flow.md`. Three crates under `signer/` share one contract:

- `proto/` — `no_std` wire contract: command IDs, buffer sizes, and the UUID read from
  `signer/uuid.txt` via `include_str!`. Both sides depend on it so they cannot drift.
- `ta/` — `no_std` TA against the GP Internal Core API (`optee-utee`). `build.rs` uses the
  SDK's `optee-utee-build` (reads `TA_DEV_KIT_DIR`); the ELF is stripped and signed by the
  dev kit's `sign_encrypt.py` into `<uuid>.ta`.
- `host/` — the CLI. Contract: exactly one JSON object on stdout, everything human-readable
  on stderr, exit 0/2/3 (ok / TEE error / bad args) — output pipes into `tests/verify.py`.

Design decisions that look odd but are deliberate:

- **The key pair is stored as a fixed 104-byte blob** (magic | format | d | x | y), not a GP
  persistent key object: the SDK's public API cannot pass a transient key to
  `TEE_CreatePersistentObject`. The blob is re-populated into a transient object per
  operation and zeroized after use.
- **Left-pad every scalar/coordinate to 32 bytes.** The GP API strips leading zero bytes on
  attribute reads (~1 in 256 per value); flush-left writing corrupts occasional keys or
  signatures — a bug that only appears on unlucky runs.
- **Fail closed on a damaged key object.** Only `ITEM_NOT_FOUND` (genuine first boot)
  triggers key generation; anything else is an error. Regenerating over tamper would
  silently replace the device identity.
- **Exclusive creation**: no `OVERWRITE` flag — a leftover object fails with
  `ACCESS_CONFLICT` instead of being replaced. (`TEE_DATA_FLAG_EXCLUSIVE` does not exist in
  this GP API version; exclusive is the default.)
- **Verification happens on the host** with a different crypto library
  (`tests/verify.py`) — stronger evidence than the client checking its own work.

## Hard rules

- **`third_party/teaclave-trustzone-sdk/` is never edited in place.** It is byte-identical
  to upstream v0.7.0 (the OP-TEE 4.8-aligned release) except one documented line — see its
  README. In particular, never "fix" a build error by patching a vendored file. Changing the
  SDK means deliberately re-vendoring a pinned upstream tag and updating that README,
  accepted only after the full board smoke test passes.
- **Confirm design decisions with the owner before writing code.** Reviewing a plan is not
  approving it.
- **Public-repo hygiene**: no lab IPs, credentials, local absolute paths, or
  sibling-project paths in anything tracked.
- **Commit trailers**: `Assisted-by: <tool>:<model-id>`, one line per model. Never
  `Co-Authored-By:` for an AI. Never copy a model id from a doc or a past commit.
- Check the mundane explanation first when hardware "disproves" something — a glob once
  expanded as the wrong user and looked like a broken experiment.

## Platform truths that bound every security claim

- **Secure storage has confidentiality and integrity but no freshness**
  (`CFG_RPMB_FS=n`): normal-world root can delete or roll back `/var/lib/tee`, and a
  deleted store makes the TA mint a fresh identity. Keys are durable against accident, not
  against a privileged local attacker. Say this plainly; do not claim more.
- **The board's `tee-supplicant` is built `RPMB_EMU=1`** — enabling `CFG_RPMB_FS=y` today
  would silently bind to a volatile fake RPMB that looks like it works.
- **The packaged `optee-os-tadevkit` lacks `CFG_RZ_SCE`** — harmless for GP-API TAs, blocks
  Renesas Secure IP work (the planned hardware-key iteration) until its bbappend is fixed.
- **ABI pin**: the TA dev kit must come from the same OP-TEE build that runs on the board.
  After any Yocto rebuild + reflash: rebuild, redeploy, re-run `make test`.
