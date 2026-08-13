// SPDX-License-Identifier: Apache-2.0
//
// signer — ECDSA P-256 device signer (OP-TEE Trusted Application).
//
// Owns one key pair: generated inside the TEE on first use, held in OP-TEE
// secure storage, never exported. There is no command that returns private
// key material (see proto).
//
// Fail-closed rule: ItemNotFound on open means first boot and triggers key
// generation. ANY other failure — a key object that is present but
// unreadable, too short, or carrying the wrong magic — is returned as an
// error. The TA never regenerates over a damaged object: silently minting a
// fresh key would replace the device's identity, which is exactly what a
// tamperer wants to look like a first boot.

#![no_std]
#![no_main]

use optee_utee::{
    ta_close_session, ta_create, ta_destroy, ta_invoke_command, ta_open_session, trace_println,
};
use optee_utee::{AlgorithmId, Asymmetric, OperationMode};
use optee_utee::{AttributeId, AttributeMemref, AttributeValue};
use optee_utee::{
    DataFlag, GenericObject, ObjectStorageConstants, PersistentObject, TransientObject,
    TransientObjectType,
};
use optee_utee::{Error, ErrorKind, ParamType, Parameters, Result};
use proto::{Command, DIGEST_LEN, PUBKEY_LEN, SIGNATURE_LEN};

// ---- Persistent key blob (fixed size, little-endian header) ---------------
//
// The vendored SDK cannot hand a transient key to TEE_CreatePersistentObject
// (its ObjectHandle constructor is private), so the key pair is stored as a
// fixed-layout data blob and re-populated into a transient object per
// operation. Same secure-storage protection either way; the private scalar
// exists in plaintext only inside TA memory.
const O_MAGIC: usize = 0; // u32
const O_FORMAT: usize = 4; // u32
const O_D: usize = 8; // private scalar d
const O_X: usize = 40; // public X
const O_Y: usize = 72; // public Y
const BLOB_SIZE: usize = 104;

const MAGIC: u32 = 0x5349_474B; // "SIGK"
const FORMAT: u32 = 1;
const OBJ_ID: &[u8] = b"signer.key.v1";

const KEY_BITS: usize = 256;
// Every scalar/coordinate field is stored left-padded to this width. The GP
// API strips leading zero bytes when reading attributes, so a value can
// legitimately come back short (~1 in 256 per value); writing it flush-left
// would corrupt roughly one key or signature per few hundred.
const FIELD: usize = 32;

#[inline]
fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

#[inline]
fn wr_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// Best-effort scrub of key material before a buffer goes back to the heap
/// or stack. Volatile so the writes cannot be optimized away.
fn zeroize(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        unsafe { core::ptr::write_volatile(b, 0) };
    }
}

/// Read one object attribute left-padded into a FIELD-byte destination.
/// `tmp` may hold private material after a partial read, so it is scrubbed
/// on every path out, error or success.
fn read_padded(key: &TransientObject, id: AttributeId, out: &mut [u8]) -> Result<()> {
    let mut tmp = [0u8; FIELD];
    let res = match key.ref_attribute(id, &mut tmp) {
        Ok(n) if n <= FIELD => {
            out[FIELD - n..].copy_from_slice(&tmp[..n]);
            Ok(())
        }
        Ok(_) => Err(Error::new(ErrorKind::Generic)),
        Err(e) => Err(e),
    };
    zeroize(&mut tmp);
    res
}

/// Serialize the key pair into `blob`. On error the caller must scrub `blob`:
/// the private scalar may already have been copied in.
fn fill_key_blob(key: &TransientObject, blob: &mut [u8; BLOB_SIZE]) -> Result<()> {
    wr_u32(blob, O_MAGIC, MAGIC);
    wr_u32(blob, O_FORMAT, FORMAT);
    read_padded(
        key,
        AttributeId::EccPrivateValue,
        &mut blob[O_D..O_D + FIELD],
    )?;
    read_padded(
        key,
        AttributeId::EccPublicValueX,
        &mut blob[O_X..O_X + FIELD],
    )?;
    read_padded(
        key,
        AttributeId::EccPublicValueY,
        &mut blob[O_Y..O_Y + FIELD],
    )?;
    Ok(())
}

/// Generate a fresh P-256 key pair inside the TEE and serialize it.
fn generate_key_blob() -> Result<[u8; BLOB_SIZE]> {
    let key = TransientObject::allocate(TransientObjectType::EcdsaKeypair, KEY_BITS)?;
    let curve = AttributeValue::from_value(
        AttributeId::EccCurve,
        optee_utee_sys::TEE_ECC_CURVE_NIST_P256,
        0,
    );
    key.generate_key(KEY_BITS, &[curve.into()])?;

    let mut blob = [0u8; BLOB_SIZE];
    match fill_key_blob(&key, &mut blob) {
        Ok(()) => Ok(blob),
        Err(e) => {
            zeroize(&mut blob);
            Err(e)
        }
    }
}

/// Load the key blob, generating and persisting it on genuine first boot.
fn load_or_create_blob() -> Result<[u8; BLOB_SIZE]> {
    match PersistentObject::open(
        ObjectStorageConstants::Private,
        OBJ_ID,
        DataFlag::ACCESS_READ,
    ) {
        Ok(obj) => {
            let mut blob = [0u8; BLOB_SIZE];
            let n = match obj.read(&mut blob) {
                Ok(n) => n,
                Err(e) => {
                    // A failed read may still have partially filled the blob.
                    zeroize(&mut blob);
                    return Err(e);
                }
            };
            if n as usize != BLOB_SIZE
                || rd_u32(&blob, O_MAGIC) != MAGIC
                || rd_u32(&blob, O_FORMAT) != FORMAT
            {
                // Present but not ours / damaged: FAIL CLOSED, never regenerate.
                zeroize(&mut blob);
                return Err(Error::new(ErrorKind::CorruptObject));
            }
            Ok(blob)
        }
        Err(e) if e.kind() == ErrorKind::ItemNotFound => {
            let mut blob = generate_key_blob()?;
            // No OVERWRITE flag: creation is exclusive in this GP API version,
            // so a concurrent or leftover object fails with AccessConflict
            // instead of being silently replaced.
            match PersistentObject::create(
                ObjectStorageConstants::Private,
                OBJ_ID,
                DataFlag::ACCESS_READ | DataFlag::ACCESS_WRITE,
                None,
                &blob,
            ) {
                Ok(obj) => {
                    drop(obj);
                    trace_println!("[+] signer: generated new P-256 key pair");
                    Ok(blob)
                }
                Err(e) => {
                    zeroize(&mut blob);
                    Err(e)
                }
            }
        }
        Err(e) => Err(e),
    }
}

/// Sign a 32-byte digest with the stored key. `sig_out` receives r || s.
fn sign_with_blob(blob: &[u8; BLOB_SIZE], digest: &[u8], sig_out: &mut [u8]) -> Result<()> {
    let mut key = TransientObject::allocate(TransientObjectType::EcdsaKeypair, KEY_BITS)?;
    let curve = AttributeValue::from_value(
        AttributeId::EccCurve,
        optee_utee_sys::TEE_ECC_CURVE_NIST_P256,
        0,
    );
    let d = AttributeMemref::from_ref(AttributeId::EccPrivateValue, &blob[O_D..O_D + FIELD]);
    let x = AttributeMemref::from_ref(AttributeId::EccPublicValueX, &blob[O_X..O_X + FIELD]);
    let y = AttributeMemref::from_ref(AttributeId::EccPublicValueY, &blob[O_Y..O_Y + FIELD]);
    key.populate(&[curve.into(), d.into(), x.into(), y.into()])?;

    let op = Asymmetric::allocate(AlgorithmId::EcDsaSha256, OperationMode::Sign, KEY_BITS)?;
    op.set_key(&key)?;

    let mut sig = [0u8; SIGNATURE_LEN];
    // OP-TEE core writes r and s each left-padded into a fixed 32-byte half,
    // so anything but exactly 64 bytes means an assumption broke: refuse
    // rather than emit a signature with ambiguous component boundaries.
    let n = op.sign_digest(&[], digest, &mut sig)?;
    if n != SIGNATURE_LEN {
        return Err(Error::new(ErrorKind::Generic));
    }
    sig_out[..SIGNATURE_LEN].copy_from_slice(&sig);
    Ok(())
}

// ---- Parameter contract ---------------------------------------------------

/// Require the exact GP parameter layout for a command. `as_memref()` alone
/// accepts any memref direction, so an REE client not constrained by
/// signer-client could pass input where the contract says output (or fill
/// unused slots). Check the raw types first; reject everything else.
fn expect_layout(params: &Parameters, want: [ParamType; 4]) -> Result<()> {
    let got = [
        params.0.param_type,
        params.1.param_type,
        params.2.param_type,
        params.3.param_type,
    ];
    for (g, w) in got.iter().zip(want.iter()) {
        if *g as u32 != *w as u32 {
            return Err(Error::new(ErrorKind::BadParameters));
        }
    }
    Ok(())
}

// ---- Command handlers -----------------------------------------------------

fn handle_get_pubkey(params: &mut Parameters) -> Result<()> {
    expect_layout(
        params,
        [
            ParamType::MemrefOutput,
            ParamType::None,
            ParamType::None,
            ParamType::None,
        ],
    )?;
    let mut p0 = unsafe { params.0.as_memref()? };
    if p0.buffer().len() < PUBKEY_LEN {
        p0.set_updated_size(PUBKEY_LEN);
        return Err(Error::new(ErrorKind::ShortBuffer));
    }

    let mut blob = load_or_create_blob()?;
    let out = p0.buffer();
    out[..FIELD].copy_from_slice(&blob[O_X..O_X + FIELD]);
    out[FIELD..PUBKEY_LEN].copy_from_slice(&blob[O_Y..O_Y + FIELD]);
    p0.set_updated_size(PUBKEY_LEN);
    zeroize(&mut blob);
    Ok(())
}

fn handle_sign(params: &mut Parameters) -> Result<()> {
    expect_layout(
        params,
        [
            ParamType::MemrefInput,
            ParamType::MemrefOutput,
            ParamType::None,
            ParamType::None,
        ],
    )?;
    let mut p0 = unsafe { params.0.as_memref()? };
    let din = p0.buffer();
    // Exactly one SHA-256 digest. The TA does not hash and does not accept
    // anything it could mistake for a message.
    if din.len() != DIGEST_LEN {
        return Err(Error::new(ErrorKind::BadParameters));
    }
    let mut digest = [0u8; DIGEST_LEN];
    digest.copy_from_slice(din);

    let mut p1 = unsafe { params.1.as_memref()? };
    if p1.buffer().len() < SIGNATURE_LEN {
        p1.set_updated_size(SIGNATURE_LEN);
        return Err(Error::new(ErrorKind::ShortBuffer));
    }

    let mut blob = load_or_create_blob()?;
    let res = sign_with_blob(&blob, &digest, p1.buffer());
    zeroize(&mut blob);
    res?;
    p1.set_updated_size(SIGNATURE_LEN);
    Ok(())
}

// ---- TA entry points ------------------------------------------------------

#[ta_create]
fn create() -> Result<()> {
    trace_println!("[+] signer TA create");
    Ok(())
}

#[ta_open_session]
fn open_session(params: &mut Parameters) -> Result<()> {
    // Sessions carry no parameters in this protocol.
    expect_layout(params, [ParamType::None; 4])
}

#[ta_close_session]
fn close_session() {}

#[ta_destroy]
fn destroy() {
    trace_println!("[+] signer TA destroy");
}

#[ta_invoke_command]
fn invoke_command(cmd_id: u32, params: &mut Parameters) -> Result<()> {
    match Command::from(cmd_id) {
        Command::GetPubkey => handle_get_pubkey(params),
        Command::Sign => handle_sign(params),
        Command::Unknown => Err(Error::new(ErrorKind::NotSupported)),
    }
}

// TA configuration (flags, stack/data sizes) lives in build.rs via
// optee-utee-build; the generated header is included here.
include!(concat!(env!("OUT_DIR"), "/user_ta_header.rs"));
