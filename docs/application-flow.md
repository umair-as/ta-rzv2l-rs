# Application and development flow

This project is easiest to understand as two programs running in two security worlds, built and
tested from a third machine. The `signer/` directory is the complete feature, not one Rust
application.

## The three environments

```text
Development computer
  cross-compiles, deploys, drives tests, verifies signatures independently
           |
           | SSH / SCP
           v
RZ/V2L normal world (Linux / REE)
  signer-client -> libteec -> OP-TEE kernel driver
           |
           | open session by TA UUID, invoke command
           v
RZ/V2L secure world (OP-TEE)
  signer TA -> GP Internal Core API -> secure storage and P-256 operations
```

The development computer is outside the TEE. It builds both board programs and runs the final
signature verifier.

The board's **normal world**, also called the Rich Execution Environment (REE), runs Linux and
`signer-client`. The client contains no private key and performs no signing itself. It is a
messenger between a command-line caller and OP-TEE.

The board's **secure world** runs OP-TEE OS and the signer Trusted Application (TA). This is where
the P-256 key is generated and where signing occurs. Private key material exists in plaintext
only in TA memory; it is never returned by the TA interface.

## Why `signer/` contains three Rust crates

```text
signer/
├── uuid.txt       one identifier shared by the build, client, deployment and tests
├── proto/         shared no_std protocol library
├── host/          normal-world client executable
└── ta/            secure-world Trusted Application
```

### `proto`: the shared contract

`signer/proto` is a small `no_std` library used by both programs. It defines:

- the TA UUID;
- command `0`, `GetPubkey`;
- command `1`, `Sign`;
- the 32-byte digest size;
- the 64-byte public-key and signature sizes; and
- the curve name emitted in JSON.

Keeping these values in one crate prevents the two sides from silently assigning different
meanings to a command number or buffer layout. It is `no_std` because the TA cannot use the
ordinary operating-system-backed Rust standard library. A normal Linux program can still depend
on a `no_std` library.

### `host`: the normal-world client

In OP-TEE examples, “host” means the **Client Application** side of the TEE boundary. Here it is
cross-compiled on the development computer but runs on the RZ/V2L board as:

```text
/usr/local/bin/signer-client
```

It depends on Teaclave's `optee-teec` crate, which wraps the GlobalPlatform TEE Client API and
links to `libteec`. It parses CLI arguments, opens the signer TA by UUID, invokes a command, and
prints exactly one JSON object. Human-readable diagnostics go to stderr so successful output can
be piped directly into another program.

The directory could have been named `client/`; `host/` follows the terminology used by the SDK.
It does not mean that this binary runs on the development computer.

### `ta`: the secure-world application

`signer/ta` is a `no_std`, `no_main` executable using Teaclave's `optee-utee` wrapper around the
GlobalPlatform TEE Internal Core API. Its entry points are called by OP-TEE rather than by a
normal operating-system process.

Its `build.rs` uses the vendored `optee-utee-build` crate to generate the TA header and linker
configuration from the board's exact OP-TEE TA development kit. The resulting ELF is stripped
and signed by the dev kit's `sign_encrypt.py`, producing:

```text
e1975de8-38de-4cf2-ae71-8c010c86425e.ta
```

That file is installed in `/lib/optee_armtz/` on the board. The signature on the TA file lets
OP-TEE authenticate loadable TA code; it is separate from the P-256 signatures the application
produces.

## What the UUID does

The UUID in `signer/uuid.txt` is the TA's public identifier:

```text
e1975de8-38de-4cf2-ae71-8c010c86425e
```

It connects five places:

1. the UUID embedded in the TA header;
2. the `<uuid>.ta` filename;
3. the UUID passed by `signer-client` when opening a session;
4. the deployment destination; and
5. the installed-file check in the smoke test.

OP-TEE receives an open-session request for that UUID and asks `tee-supplicant` in the normal
world to load the matching TA file. The UUID is routing and identity, not a secret, credential,
or proof of which normal-world process is calling. This milestone does not authenticate REE
callers; a process able to use the TEE client interface can request a signature.

## Key lifecycle

On the first `GetPubkey` or `Sign` request, the TA tries to open its private storage object named
`signer.key.v1`.

```text
open secure-storage object
        |
        +-- ITEM_NOT_FOUND --> generate P-256 key in the TEE --> persist it
        |
        +-- success ---------> validate and use the existing object
        |
        +-- any other error --> return the error; do not replace the key
```

The key is represented in storage by a fixed 104-byte record:

```text
magic (4) | format (4) | private d (32) | public X (32) | public Y (32)
```

The SDK cannot pass its public transient-key wrapper directly to
`TEE_CreatePersistentObject`, so the TA reads the generated key attributes into this record and
stores it as private TA data. For an operation, it loads the record and repopulates a transient
P-256 key object. Key buffers are overwritten after use as a best-effort reduction of their
lifetime in TA memory.

The scalar and coordinates are always left-padded to 32 bytes. OP-TEE may return a valid integer
without its leading zero bytes; failing to restore those bytes would cause rare, intermittent
key or signature corruption.

Only `ITEM_NOT_FOUND` triggers generation. If an existing object cannot be read or has an invalid
length, magic, or format, the TA fails closed. It does not make corruption appear to be a fresh
device by silently replacing the identity.

## Request flow

### Reading the public key

```text
signer-client pubkey
  -> TEEC_OpenSession(signer UUID)
  -> InvokeCommand(GetPubkey, 64-byte output buffer)
  -> TA loads or creates the key
  -> TA returns X || Y
  -> client prints lowercase hex in JSON
```

`X` and `Y` are each 32-byte, big-endian P-256 coordinates. The returned public key is therefore
64 bytes; it is not a DER or PEM object.

### Signing a digest

```text
signer-client sign <64 hex characters>
  -> parse exactly 32 bytes
  -> TEEC_OpenSession(signer UUID)
  -> InvokeCommand(Sign, digest input, 64-byte signature output)
  -> TA loads the key and performs ECDSA P-256/SHA-256
  -> TA returns r || s
  -> client also fetches the public key and prints one JSON object
```

The TA signs a caller-supplied SHA-256 **digest**. It does not hash an arbitrary message. `r` and
`s` are each 32-byte, big-endian integers, so the raw signature is 64 bytes rather than ASN.1 DER.

## Build and deploy flow

The root Makefile delegates to separate Makefiles for the client and TA:

```text
make
  -> signer/host: cargo build for aarch64 + strip
  -> signer/ta:   cargo build for aarch64 + strip + sign TA

make deploy
  -> copy signer-client and <uuid>.ta to the board
  -> install the client in /usr/local/bin/
  -> install the TA in /lib/optee_armtz/

make test
  -> drive signer-client and ta-probe over SSH
  -> verify returned signatures on the development computer

make lint
  -> cargo fmt --check + clippy; the TA crate additionally forbids
     unwrap/expect/panic (secure-world code returns errors)
```

Machine-specific paths and the board address belong in the untracked `local.mk`. The TA dev kit
must come from the same OP-TEE build that runs on the board; a TA built against an unrelated kit
can have ABI or configuration mismatches even if it compiles.

Both OP-TEE's TA signing script and `tests/verify.py` require Python's `cryptography` package.
Check the interpreter selected by the build environment, especially when a virtual environment is
active:

```sh
python3 -c 'import cryptography; print(cryptography.__version__)'
```

The normal development loop deploys with `scp`; it does not rebuild a Yocto image for every TA
change. Yocto packaging is a separate future integration step.

## Verification and its boundaries

`tests/verify.py` runs on the development computer and uses Python `cryptography`, a different
cryptographic implementation from OP-TEE. It reconstructs the P-256 public point and verifies
the raw `r || s` signature over the reported digest.

The smoke test also substitutes a different digest and requires verification to fail. That
mutation demonstrates that the positive verification is actually bound to the requested digest,
instead of being a check that always passes.

With reboot testing enabled, the test records the public key, reboots the board, and requires the
same key afterward. This demonstrates persistence for the tested storage state and board build.

Negative coverage is split by the boundary it exercises, and the smoke test labels each check
accordingly:

- **CLI checks**: a short digest and an unknown subcommand are rejected by `signer-client`
  before it opens a TA session. These prove the public CLI validates input; they say nothing
  about the TA.
- **TA-boundary checks**: a second on-board binary, `ta-probe`, deliberately bypasses the
  CLI and speaks raw TEEC to the TA. In one process and one session it sends a short digest
  with a correct parameter layout, an unknown numeric command ID, wrong memref directions for
  both commands, and an unexpected extra parameter, then a valid request in that same
  session. A malformed case passes only if the TA itself (error origin TA) returned the
  exact expected code — `BadParameters`, or `NotSupported` for the unknown command — so a TA
  panic, transport failure, or unrelated error counts as a failure, and a crash cannot hide
  behind a freshly loaded TA instance in a later process. The TA enforces the exact GP
  parameter layout per command (`GetPubkey`: one output memref; `Sign`: input memref +
  output memref; unused slots must be `None`; sessions carry no parameters), so a custom or
  hostile REE client gets the same rejections.

These probes were mutation-tested twice: with the TA's validation deliberately disabled in a
temporary build, all five TA-boundary checks fail; with the TA returning the wrong error kind
(`Generic` instead of `BadParameters`), the layout-driven cases fail with a precise diagnostic
while the digest-length case — a different code path — still passes. Validation restored,
everything passes.

## Security boundary and storage limitation

Secure storage on this board provides confidentiality and integrity, but not freshness. The
normal-world files under `/var/lib/tee/` do not reveal the plaintext key, but privileged Linux
code can delete them or restore an older valid snapshot because this platform has no working
rollback-resistant counter backing the storage.

Consequences:

- deletion makes the TA observe a genuine `ITEM_NOT_FOUND` and create a new identity;
- restoring an older valid storage snapshot can roll state backward; and
- reboot persistence demonstrates durability, not rollback resistance.

The signer is therefore a concrete demonstration of TEE key isolation and signing, with the
storage limitations of this exact RZ/V2L platform stated explicitly. It is not presented as a
production device-identity system.

