// SPDX-License-Identifier: Apache-2.0
//
// TA header + linker setup via the SDK's optee-utee-build crate
// (reads TA_DEV_KIT_DIR from the environment).

use optee_utee_build::{Error, RustEdition, TaConfig};

// TA_FLAG_SINGLE_INSTANCE (optee-utee-sys user_ta_header.rs): one TA instance,
// gpd.ta.multiSession=false.
const TA_FLAG_SINGLE_INSTANCE: u32 = 1 << 2;

fn main() -> Result<(), Error> {
    let config = TaConfig::new_default_with_cargo_env(proto::UUID.trim())?
        .ta_flags(TA_FLAG_SINGLE_INSTANCE)
        .ta_stack_size(16 * 1024)
        .ta_data_size(32 * 1024);
    optee_utee_build::build(RustEdition::Before2024, config)
}
