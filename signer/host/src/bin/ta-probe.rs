// SPDX-License-Identifier: Apache-2.0
//
// ta-probe — direct TA-boundary tester. Test tooling, not part of the client
// contract: it deliberately bypasses signer-client's argument validation and
// sends malformed invocations straight to the TA, so the smoke test can claim
// "the TA rejects X" rather than "the CLI rejects X".
//
// The whole sequence runs in ONE process and ONE session, ending with a valid
// request in that same session. A malformed case is only a pass if the TA
// itself (error origin TA) returned the specific expected error code — a TA
// panic (TargetDead), transport failure, or unrelated error is a FAIL, and a
// crash cannot hide behind a freshly loaded TA instance in a later process.
//
// Output: one machine-readable line per case on stdout
// (`PASS <case>: detail` / `FAIL <case>: detail`); setup problems go to
// stderr with exit 2. Exit 0 iff every case behaved as required.

use optee_teec::{
    Context, Error, ErrorKind, ErrorOrigin, Operation, ParamNone, ParamTmpRef, ParamType,
    ParamValue, Session, Uuid,
};
use proto::{Command, DIGEST_LEN, PUBKEY_LEN, SIGNATURE_LEN, UUID};

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
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

    let mut failures = 0;
    failures += rejected(
        "sign-short-digest",
        ErrorKind::BadParameters,
        sign_short_digest(&mut sess),
    );
    failures += rejected(
        "unknown-command",
        ErrorKind::NotSupported,
        unknown_command(&mut sess),
    );
    failures += rejected(
        "pubkey-wrong-direction",
        ErrorKind::BadParameters,
        pubkey_wrong_direction(&mut sess),
    );
    failures += rejected(
        "sign-wrong-direction",
        ErrorKind::BadParameters,
        sign_wrong_direction(&mut sess),
    );
    failures += rejected(
        "pubkey-extra-param",
        ErrorKind::BadParameters,
        pubkey_extra_param(&mut sess),
    );
    // Same session as every malformed case above: proves the instance that
    // absorbed them is still the one answering.
    failures += accepted("valid-after", valid_pubkey(&mut sess));

    if failures == 0 {
        0
    } else {
        1
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

/// A fully well-formed GetPubkey.
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

fn describe(e: &Error) -> String {
    format!("{:?} from {:?}", e.kind(), e.origin())
}

/// Pass iff the TA itself rejected the call with exactly `want`.
fn rejected(case: &str, want: ErrorKind, res: optee_teec::Result<()>) -> i32 {
    match res {
        Err(e) => {
            let kind_ok = e.raw_code() == want as u32;
            let origin_ok = matches!(e.origin(), Some(ErrorOrigin::TA));
            if kind_ok && origin_ok {
                println!("PASS {}: rejected with {:?} by the TA", case, want);
                0
            } else {
                println!(
                    "FAIL {}: wanted {:?} from TA, got {}",
                    case,
                    want,
                    describe(&e)
                );
                1
            }
        }
        Ok(()) => {
            println!("FAIL {}: TA accepted a malformed invocation", case);
            1
        }
    }
}

fn accepted(case: &str, res: optee_teec::Result<()>) -> i32 {
    match res {
        Ok(()) => {
            println!("PASS {}: valid request succeeded in the same session", case);
            0
        }
        Err(e) => {
            println!("FAIL {}: valid request failed ({})", case, describe(&e));
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
