// SPDX-License-Identifier: Apache-2.0
//
// signer client application.
//
// Contract:
//   * Exactly one JSON object is written to stdout per invocation.
//   * All human-readable text goes to stderr.
//   * Exit codes: 0 success, 2 TEE/transport error, 3 invalid arguments.
//
// `sign` output carries pubkey + digest + signature so it pipes directly
// into tests/verify.py on the development host.

use optee_teec::{Context, Operation, ParamNone, ParamTmpRef, Session, Uuid};
use proto::{Command, CURVE, DIGEST_LEN, PUBKEY_LEN, SIGNATURE_LEN, UUID};

const EXIT_SUCCESS: i32 = 0;
const EXIT_TRANSPORT: i32 = 2;
const EXIT_INVALID: i32 = 3;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().collect();

    let sub = match args.get(1) {
        Some(s) => s.as_str(),
        None => {
            print_usage();
            return fail("(none)", "missing subcommand", EXIT_INVALID);
        }
    };

    match sub {
        "help" | "--help" | "-h" => cmd_help(),
        "version" | "--version" | "-V" => cmd_version(),
        "pubkey" => cmd_pubkey(),
        "sign" => cmd_sign(&args[2..]),
        other => {
            print_usage();
            fail(other, "unknown subcommand", EXIT_INVALID)
        }
    }
}

fn print_usage() {
    eprintln!(
        "usage:\n  \
         signer-client pubkey            # print the device public key (X||Y hex)\n  \
         signer-client sign <64-hex>     # sign a SHA-256 digest, print r||s hex\n  \
         signer-client version           # print client version\n  \
         signer-client help              # this message"
    );
}

// ---- Subcommands ----------------------------------------------------------

fn cmd_help() -> i32 {
    print_usage();
    emit(
        "help",
        &[(
            "subcommands",
            "[\"pubkey\",\"sign\",\"version\",\"help\"]".into(),
        )],
    );
    EXIT_SUCCESS
}

fn cmd_version() -> i32 {
    emit("version", &[("version", jstr(env!("CARGO_PKG_VERSION")))]);
    EXIT_SUCCESS
}

fn cmd_pubkey() -> i32 {
    match with_session(get_pubkey) {
        Ok(pk) => {
            emit(
                "pubkey",
                &[("curve", jstr(CURVE)), ("pubkey", jstr(&hex(&pk)))],
            );
            EXIT_SUCCESS
        }
        Err(e) => fail("pubkey", &e, EXIT_TRANSPORT),
    }
}

fn cmd_sign(rest: &[String]) -> i32 {
    let digest = match rest {
        [d] => match parse_digest(d) {
            Ok(d) => d,
            Err(e) => return fail("sign", &e, EXIT_INVALID),
        },
        _ => {
            return fail(
                "sign",
                "usage: signer-client sign <64-hex-digest>",
                EXIT_INVALID,
            )
        }
    };

    match with_session(|sess| {
        let pk = get_pubkey(sess)?;
        let sig = sign(sess, &digest)?;
        Ok((pk, sig))
    }) {
        Ok((pk, sig)) => {
            emit(
                "sign",
                &[
                    ("curve", jstr(CURVE)),
                    ("pubkey", jstr(&hex(&pk))),
                    ("digest", jstr(&hex(&digest))),
                    ("signature", jstr(&hex(&sig))),
                ],
            );
            EXIT_SUCCESS
        }
        Err(e) => fail("sign", &e, EXIT_TRANSPORT),
    }
}

// ---- TEE invocation -------------------------------------------------------

fn with_session<T, F: FnOnce(&mut Session) -> Result<T, String>>(f: F) -> Result<T, String> {
    let mut ctx = Context::new().map_err(|e| format!("context: {}", e))?;
    let uuid = Uuid::parse_str(UUID.trim()).map_err(|e| format!("uuid: {}", e))?;
    let mut session = ctx
        .open_session(uuid)
        .map_err(|e| format!("open_session: {}", e))?;
    f(&mut session)
}

fn get_pubkey(session: &mut Session) -> Result<[u8; PUBKEY_LEN], String> {
    let mut pk = [0u8; PUBKEY_LEN];
    let p0 = ParamTmpRef::new_output(&mut pk);
    let mut op = Operation::new(0, p0, ParamNone, ParamNone, ParamNone);
    session
        .invoke_command(Command::GetPubkey as u32, &mut op)
        .map_err(|e| format!("invoke_command(pubkey): {}", e))?;
    Ok(pk)
}

fn sign(session: &mut Session, digest: &[u8; DIGEST_LEN]) -> Result<[u8; SIGNATURE_LEN], String> {
    let mut sig = [0u8; SIGNATURE_LEN];
    let p0 = ParamTmpRef::new_input(digest);
    let p1 = ParamTmpRef::new_output(&mut sig);
    let mut op = Operation::new(0, p0, p1, ParamNone, ParamNone);
    session
        .invoke_command(Command::Sign as u32, &mut op)
        .map_err(|e| format!("invoke_command(sign): {}", e))?;
    Ok(sig)
}

// ---- Output helpers -------------------------------------------------------

/// Emit exactly one JSON object on stdout.
fn emit(command: &str, fields: &[(&str, String)]) {
    let mut s = String::from("{");
    s.push_str(&format!("\"command\":{}", jstr(command)));
    for (k, v) in fields {
        s.push_str(&format!(",{}:{}", jstr(k), v));
    }
    s.push('}');
    println!("{}", s);
}

fn fail(command: &str, msg: &str, code: i32) -> i32 {
    eprintln!("signer-client {}: {}", command, msg);
    emit(command, &[("error", jstr(msg))]);
    code
}

fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn parse_digest(s: &str) -> Result<[u8; DIGEST_LEN], String> {
    if s.len() != DIGEST_LEN * 2 {
        return Err(format!(
            "digest must be {} hex chars (a SHA-256 value), got {}",
            DIGEST_LEN * 2,
            s.len()
        ));
    }
    let mut out = [0u8; DIGEST_LEN];
    for i in 0..DIGEST_LEN {
        out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16)
            .map_err(|_| format!("digest is not valid hex at byte {}", i))?;
    }
    Ok(out)
}
