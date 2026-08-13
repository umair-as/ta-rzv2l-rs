# Security model

What the signer does and does not protect, by attacker capability, with the evidence for each
claim. The rule of this project is to claim exactly what has been demonstrated on this board —
no more. This document will grow as mechanisms land; it is not a compliance artifact.

## The claim, precisely

> The RZ/V2L TEE holds a persistent ECDSA P-256 private key and produces valid signatures over
> caller-supplied SHA-256 digests, without exposing the private key to normal-world Linux.

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
| Normal-world root | No, not via any software-only read path demonstrated here | **Destroy: yes** — can delete `/var/lib/tee/`, forcing a new identity; can also roll storage back |
| Root, determined, firmware-level | **Ultimately yes** on this board's current configuration (see below) | Yes |
| Physical attacker | Out of scope — a TEE offers no tamper resistance | Yes |

## Platform limitations that bound the claims

These are properties of this board's current OP-TEE configuration, stated here so the table
above cannot be read as stronger than it is.

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
  secure storage — key **theft**, not just deletion. This is why the root row above says
  "no software-only read path *demonstrated here*" rather than "impossible".
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
