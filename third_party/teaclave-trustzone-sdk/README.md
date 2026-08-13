# Vendored: Apache Teaclave TrustZone SDK v0.7.0 (one-line patch for stable Rust)

Source: https://github.com/apache/teaclave-trustzone-sdk
tag `v0.7.0` = `236d79dabc61bcf1925823c2928126031ff45f45` (2025-11-13), the release
aligned with OP-TEE 4.8.0 — the OP-TEE version this board's Renesas BSP builds.

Vendored trees: `optee-utee` (TA-side), `optee-teec` (host-side), and
`optee-utee-build` (TA build-script support: TA header generation and linker
setup; a build-dependency of every TA crate). Upstream `LICENSE`, `NOTICE`, and
`licenses/` are retained verbatim; every vendored `.rs` file keeps its original
ASF header. (The incubator-era `DISCLAIMER-WIP`/`KEYS` files no longer exist at
this upstream tag.)

Note: at this tag the `optee-utee` crate manifest still says version 0.6.0.
The tag/commit above is the source of truth, not the crate version string —
do not substitute a similarly-versioned crates.io package.

## Local patch — exactly one line

`optee-utee/src/lib.rs`: removed

```rust
#![cfg_attr(not(feature = "std"), feature(error_in_core))]
```

`core::error::Error` is stable since Rust 1.81, so on this repo's pinned stable
toolchain the gate is not only unnecessary — declaring it is itself a nightly-only
act, which is why the line must go rather than stay dormant. No logic, type
layout, or ABI is affected.

The previous vendored revision (`ec3eefd9`, March 2024) needed a 33-token
`c_size_t` rewrite and a locally added ECDSA `AlgorithmId` variant; v0.7.0 has
both natively (`usize` FFI, `AlgorithmId::EcDsaSha256`), so those local changes
are retired, not carried forward.

## License

Apache-2.0.
