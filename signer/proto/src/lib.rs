// SPDX-License-Identifier: Apache-2.0
//
// signer — device signer TA. Shared protocol between the host client and the TA.
//
// One ECDSA P-256 key pair, generated inside the TEE on first use and held in
// OP-TEE secure storage. The interface deliberately has no command that
// returns private key material.
//
//   ID | Command   | param0                  | param1
//   ---+-----------+-------------------------+--------------------------
//    0 | GetPubkey | memref-out: X||Y, 64 B  | -
//    1 | Sign      | memref-in: 32 B digest  | memref-out: r||s, 64 B
//   any other id -> NotSupported, no state change
//
// All coordinates and scalars are big-endian, each left-padded to 32 bytes.
// The digest is a SHA-256 value computed by the caller; the TA does NOT hash.

#![no_std]

/// signer command identifiers. Values are the raw TEEC_InvokeCommand command IDs.
#[repr(u32)]
pub enum Command {
    /// Return the public key as X || Y, 64 bytes.
    GetPubkey = 0,
    /// Sign a 32-byte SHA-256 digest; returns r || s, 64 bytes.
    Sign = 1,
    /// Any other id.
    Unknown,
}

impl From<u32> for Command {
    #[inline]
    fn from(value: u32) -> Command {
        match value {
            0 => Command::GetPubkey,
            1 => Command::Sign,
            _ => Command::Unknown,
        }
    }
}

/// Input to Sign: one SHA-256 digest.
pub const DIGEST_LEN: usize = 32;

/// Output of GetPubkey: X || Y, each left-padded to 32 bytes.
pub const PUBKEY_LEN: usize = 64;

/// Output of Sign: r || s, each left-padded to 32 bytes.
pub const SIGNATURE_LEN: usize = 64;

/// Curve name as it appears in the client's JSON output.
pub const CURVE: &str = "p256";

pub const UUID: &str = include_str!("../../uuid.txt");
