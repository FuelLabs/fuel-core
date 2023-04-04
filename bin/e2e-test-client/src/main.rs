//! The `e2e-test-client` binary is used to run end-to-end tests for the Fuel client.

pub use fuel_core_e2e_client::*;
use libtest_mimic::Arguments;

pub fn main() {
    main_body(load_config_env(), Arguments::from_args())
}
