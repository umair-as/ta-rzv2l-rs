# Security model

What the signer does and does not protect, by attacker capability, with the evidence for each
claim. The rule of this project is to claim exactly what has been demonstrated on this board —
no more. This document will grow as mechanisms land; it is not a compliance artifact.

## The claim, precisely

> The RZ/V2L TEE holds a persistent ECDSA P-256 private key and produces valid signatures over
> caller-supplied SHA-256 digests, without exposing the private key *through its interface* to
> normal-world Linux.

The qualifier is load-bearing: no command returns private material, and an unprivileged
attacker cannot reach it. But on this board's current configuration key confidentiality
against a **root** attacker can no longer be claimed — root can read secure-world DRAM directly
(see "Secure DRAM is not isolated" below) — so this is a demonstration of the signing primitive
and its interface discipline, not a claim of key secrecy against local root.

Evidence: the board smoke test (`tests/board-smoke.sh`), whose fourteen checks include
independent host-side verification with a different crypto library, a tampered-digest rejection,
direct malformed TA invocations via `ta-probe`, and key persistence across reboot. The negative
checks were mutation-tested — validation was deliberately disabled in a temporary build and the
checks were confirmed to fail — so a green run is meaningful.

The interface has no command that returns private material, and the TA fails closed on a
damaged key object rather than regenerating (tamper must not look like first boot). Key
material at rest lives in OP-TEE secure storage: the *ciphertext* sits on the normal-world
filesystem under `/var/lib/tee/`, encrypted by OP-TEE with device-derived keys; plaintext
exists only in secure-world memory, and buffers holding it are scrubbed after use on both
success and error paths.

## What that means per attacker

| Attacker | Can they extract the key? | Can they misuse or destroy it? |
|---|---|---|
| Remote, no code on device | No | No |
| Local unprivileged process, no TEE access | No | No |
| Local process with TEE client access | No | **Misuse: yes** — can request signatures over arbitrary digests |
| Normal-world root | **Confidentiality broken.** Root can read secure DRAM directly — demonstrated for OP-TEE code, no firmware step (see below). The private scalar sits in that same readable DRAM during a signing call, so extraction is very likely feasible; this project has not attempted it | **Destroy: yes** — can delete `/var/lib/tee/`, forcing a new identity; can also roll storage back |
| Root, determined, firmware-level | Yes (also available, via the firmware path below) | Yes |
| Physical attacker | Out of scope — a TEE offers no tamper resistance | Yes |

## Platform limitations that bound the claims

These are properties of this board's current OP-TEE configuration, stated here so the table
above cannot be read as stronger than it is.

- **Secure DRAM is not isolated from the normal world.** The core TEE guarantee — that Linux
  cannot read secure-world memory — does not hold on this board. Demonstrated on hardware
  (2026-08-13): as root, three consecutive reads

  ```sh
  devmem2 0x44100000 w   # -> 0xAA0003F3
  devmem2 0x44100004 w   # -> 0xAA0103F4
  devmem2 0x44100008 w   # -> 0xAA0203F5
  ```

  returned a valid AArch64 instruction sequence (`mov x19,x0` / `mov x20,x1` / `mov x21,x2`) —
  OP-TEE's own code, read cleanly with no bus fault — while a `STRICT_DEVMEM` control at the
  first System-RAM address (`0x48000000`) was correctly refused. The read addresses fall inside
  OP-TEE's TZDRAM. The firmware memory map is:

  | Region | Range |
  |---|---|
  | TF-A | `0x43f00000`–`0x440fffff` |
  | OP-TEE TZDRAM | `0x44100000`–`0x47dfffff` |
  | Linux System RAM | from `0x48000000` |

  TF-A brings up TZC-400 with a permissive region 0 but adds no secure-only DDR regions — the
  region that would fence TZDRAM is compiled only under `TRUSTED_BOARD_BOOT`, which is off here
  — so the DDR firewall leaves secure DRAM open to normal-world reads. **Consequence:** key
  confidentiality against root can no longer be claimed. The private scalar is in this readable
  DRAM while a signing operation runs, so extracting it is very likely feasible; this project
  has not attempted it, and it did not need the firmware path below. The fix is a TF-A/TZC
  change in the BSP, outside this repo.
- **No storage freshness.** Secure storage has confidentiality and integrity but no
  rollback-resistant counter (`CFG_RPMB_FS=n`; the platform port implements no non-volatile
  counter). Root can delete or restore `/var/lib/tee/`. Deletion presents to the TA as a
  genuine first boot, so the device mints a new identity. The key is therefore durable against
  accident, not against a privileged local attacker. Proven on hardware in the predecessor
  project. A fix path exists (RPMB-backed storage) and is deliberately not claimed until it is
  demonstrated.
- **Secure boot is not fused.** The boot ROM verifies nothing on this development board, and
  the Renesas OP-TEE fork ships a normal-world-reachable flash-write primitive. A determined
  root attacker could therefore replace the firmware, boot a modified OP-TEE, and read
  secure storage — key **theft**, not just deletion. On this board this is a second, heavier
  path to the same result the direct DRAM read above already gives.
- **No caller authentication.** The TA validates the *format* of every request strictly
  (exact GP parameter layout, exact digest length — enforced and probe-tested), but any
  process that can open the TEE client interface may request a signature. The signing oracle
  is protected; access to it is not. The TA also has no idea what a digest *means* — a
  temperature report and a firmware update hash look identical. Authorization and signing
  policy are deliberately later layers.

## The trust gap verification does not close

`tests/verify.py` answers: *was this signed by the private key corresponding to this public
key?* It does not answer: *does this public key belong to this device?* An attacker can present
their own key pair and a perfectly verifying signature. Binding a public key to a device
identity requires enrollment — recording the key through a trusted channel, comparing
fingerprints out of band, or certification by an authority. That is the next layer up and is
not claimed by the current signer.

## Direction

A planned iteration replaces the GP-API software key with a hardware-backed key (Renesas
Secure IP) behind the same two-command interface — the client, protocol, and this model's
interface rows are unchanged; only the key-protection rows deepen. Storage freshness (RPMB)
and boot authentication are separate, independent mechanisms; each moves its row in the table
only when it has been demonstrated on this board.
