#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(warnings)]

pub mod v0;
pub mod v1;

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn cumulative_percentage_change(
    new_exec_price: u64,
    for_height: u32,
    percentage: u64,
    height: u32,
) -> u64 {
    let blocks = height.saturating_sub(for_height) as f64;
    let percentage_as_decimal = percentage as f64 / 100.0;
    let multiple = (1.0f64 + percentage_as_decimal).powf(blocks);
    let mut approx = new_exec_price as f64 * multiple;
    // account for rounding errors and take a slightly higher value
    const ARB_CUTOFF: f64 = 16948547188989277.0;
    if approx > ARB_CUTOFF {
        const ARB_ADDITION: f64 = 2000.0;
        approx += ARB_ADDITION;
    }
    // `f64` over `u64::MAX` are cast to `u64::MAX`
    approx.ceil() as u64
}
