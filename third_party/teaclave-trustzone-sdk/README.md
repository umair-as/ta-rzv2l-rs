# Vendored: Apache Teaclave TrustZone SDK (patched for stable Rust)

Source: https://github.com/apache/incubator-teaclave-trustzone-sdk @ `ec3eefd9de68a18d5acee1a151e0d93f6898807f`
(the rev the upstream OP-TEE qemu_v8 4.5.0 manifest pins). Only the `optee-utee` and
`optee-teec` crate trees are vendored — the crates our TAs (`optee-utee`) and CAs
(`optee-teec`) depend on.

## Local patch — drop the nightly requirement (toolchain-only, ABI-identical)
The upstream rev needs nightly for two feature gates; both are removable without any ABI
change, so the TAs build on **stable** rustc:
- `optee-utee/optee-utee-sys/src/lib.rs`: removed `#![feature(c_size_t)]`; every `c_size_t`
  FFI token in `tee_api_types.rs`/`tee_api.rs`/`utee_syscalls.rs` replaced with `usize`
  (`core::ffi::c_size_t` *is* `usize` on every real target).
- `optee-utee/src/lib.rs`: removed `#![cfg_attr(not(feature = "std"), feature(error_in_core))]`
  (`core::error::Error` is stable since Rust 1.81).

No logic, type layout, or ABI changed. See VERSIONS.md ("TA build toolchain").

## Local addition — `AlgorithmId::EcdsaP256Sha256`
`optee-utee/src/crypto_op.rs`: added one `AlgorithmId` enum variant
`EcdsaP256Sha256 = 0x70003042` (the GP id current OP-TEE headers map `TEE_ALG_ECDSA_P256`
to). Upstream `ec3eefd9` predates ECDSA in that enum; modelguard needs it for manifest
signature verification and previously reached it via an unsound `transmute` — this variant
replaces that. Additive, backward-compatible.

## License

Apache-2.0. The upstream `LICENSE`, `NOTICE`, `DISCLAIMER-WIP`, and `KEYS` are
retained here verbatim per Apache-2.0 §4; every vendored `.rs` file keeps its
original ASF header. The two-line patch above does not alter licensing.
