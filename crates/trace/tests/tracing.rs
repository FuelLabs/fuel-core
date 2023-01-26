use fuel_core_trace::enable_tracing;
use tracing::*;

enable_tracing!();

#[test]
fn works() {
    let s = error_span!("works");
    let _g = s.enter();
    error!("I'm visible if FUEL_TRACE=1 is set");
    info!("I'm visible if FUEL_TRACE=1 and RUST_LOG=info are set");
}

#[test]
fn also_works() {
    let s = error_span!("also works");
    let _g = s.enter();
    error!("I'm visible if FUEL_TRACE=1 is set");
    info!("I'm visible if FUEL_TRACE=1 and RUST_LOG=info are set");
}
