// SPDX-License-Identifier: Apache-2.0
//
// ta-probe — direct TA-boundary tester. Test tooling, not part of the client
// contract: it deliberately bypasses signer-client's argument validation and
// sends malformed invocations straight to the TA, so the smoke test can claim
// "the TA rejects X" rather than "the CLI rejects X".
//
// One case per invocation; exit 0 iff the TA behaved as the contract
// requires (error for malformed cases, success for `valid`). Diagnostics on
// stderr only.

use optee_teec::{
    Context, Operation, ParamNone, ParamTmpRef, ParamType, ParamValue, Session, Uuid,
};
use proto::{Command, DIGEST_LEN, PUBKEY_LEN, SIGNATURE_LEN, UUID};

fn main() {
    std::process::exit(run());
}

const CASES: &str =
    "sign-short-digest | unknown-command | pubkey-wrong-direction | sign-wrong-direction | \
     pubkey-extra-param | valid";

fn run() -> i32 {
    let case = match std::env::args().nth(1) {
        Some(c) => c,
        None => {
            eprintln!("usage: ta-probe <{}>", CASES);
            return 2;
        }
    };

    let mut ctx = match Context::new() {
        Ok(c) => c,
        Err(e) => return broken(&format!("context: {}", e)),
    };
    let uuid = match Uuid::parse_str(UUID.trim()) {
        Ok(u) => u,
        Err(e) => return broken(&format!("uuid: {}", e)),
    };
    let mut sess = match ctx.open_session(uuid) {
        Ok(s) => s,
        Err(e) => return broken(&format!("open_session: {}", e)),
    };

    match case.as_str() {
        "sign-short-digest" => expect_err(&case, sign_short_digest(&mut sess)),
        "unknown-command" => expect_err(&case, unknown_command(&mut sess)),
        "pubkey-wrong-direction" => expect_err(&case, pubkey_wrong_direction(&mut sess)),
        "sign-wrong-direction" => expect_err(&case, sign_wrong_direction(&mut sess)),
        "pubkey-extra-param" => expect_err(&case, pubkey_extra_param(&mut sess)),
        "valid" => expect_ok(&case, valid_pubkey(&mut sess)),
        other => {
            eprintln!("ta-probe: unknown case {:?} (want {})", other, CASES);
            2
        }
    }
}

// ---- Probe cases ----------------------------------------------------------

/// Correct layout (input, output) so the call gets past the type check and
/// exercises the digest-length check: 8 bytes instead of 32.
fn sign_short_digest(sess: &mut Session) -> optee_teec::Result<()> {
    let short = [0xabu8; 8];
    let mut sig = [0u8; SIGNATURE_LEN];
    let mut op = Operation::new(
        0,
        ParamTmpRef::new_input(&short),
        ParamTmpRef::new_output(&mut sig),
        ParamNone,
        ParamNone,
    );
    sess.invoke_command(Command::Sign as u32, &mut op)
}

/// Command ID outside the protocol.
fn unknown_command(sess: &mut Session) -> optee_teec::Result<()> {
    let mut op = Operation::new(0, ParamNone, ParamNone, ParamNone, ParamNone);
    sess.invoke_command(0x1337, &mut op)
}

/// GetPubkey requires a MemrefOutput; send MemrefInput instead.
fn pubkey_wrong_direction(sess: &mut Session) -> optee_teec::Result<()> {
    let buf = [0u8; PUBKEY_LEN];
    let mut op = Operation::new(
        0,
        ParamTmpRef::new_input(&buf),
        ParamNone,
        ParamNone,
        ParamNone,
    );
    sess.invoke_command(Command::GetPubkey as u32, &mut op)
}

/// Sign requires (input, output); send (output, output). The first buffer is
/// digest-sized so that without the type check the call would look valid and
/// succeed — which is exactly what the mutation test relies on.
fn sign_wrong_direction(sess: &mut Session) -> optee_teec::Result<()> {
    let mut not_a_digest = [0u8; DIGEST_LEN];
    let mut sig = [0u8; SIGNATURE_LEN];
    let mut op = Operation::new(
        0,
        ParamTmpRef::new_output(&mut not_a_digest),
        ParamTmpRef::new_output(&mut sig),
        ParamNone,
        ParamNone,
    );
    sess.invoke_command(Command::Sign as u32, &mut op)
}

/// GetPubkey with a correct param0 but an unexpected extra value param.
fn pubkey_extra_param(sess: &mut Session) -> optee_teec::Result<()> {
    let mut pk = [0u8; PUBKEY_LEN];
    let mut op = Operation::new(
        0,
        ParamTmpRef::new_output(&mut pk),
        ParamValue::new(7, 7, ParamType::ValueInput),
        ParamNone,
        ParamNone,
    );
    sess.invoke_command(Command::GetPubkey as u32, &mut op)
}

/// A fully well-formed GetPubkey; used after the malformed cases to show the
/// TA is still alive and functional.
fn valid_pubkey(sess: &mut Session) -> optee_teec::Result<()> {
    let mut pk = [0u8; PUBKEY_LEN];
    let mut op = Operation::new(
        0,
        ParamTmpRef::new_output(&mut pk),
        ParamNone,
        ParamNone,
        ParamNone,
    );
    sess.invoke_command(Command::GetPubkey as u32, &mut op)
}

// ---- Outcome plumbing -----------------------------------------------------

fn expect_err(case: &str, res: optee_teec::Result<()>) -> i32 {
    match res {
        Err(e) => {
            eprintln!("ta-probe {}: rejected as required ({})", case, e);
            0
        }
        Ok(()) => {
            eprintln!("ta-probe {}: TA ACCEPTED a malformed invocation", case);
            1
        }
    }
}

fn expect_ok(case: &str, res: optee_teec::Result<()>) -> i32 {
    match res {
        Ok(()) => {
            eprintln!("ta-probe {}: succeeded as required", case);
            0
        }
        Err(e) => {
            eprintln!("ta-probe {}: valid invocation FAILED ({})", case, e);
            1
        }
    }
}

/// Probe infrastructure itself failed (no context/session) — neither pass nor
/// fail, and must not be counted as a TA rejection.
fn broken(msg: &str) -> i32 {
    eprintln!("ta-probe: setup failed: {}", msg);
    2
}
