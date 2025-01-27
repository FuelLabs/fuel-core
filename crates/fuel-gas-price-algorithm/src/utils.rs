#[allow(clippy::cast_possible_truncation)]
#[inline]
pub(crate) fn cumulative_percentage_change(
    new_exec_price: u64,
    for_height: u32,
    percentage: u64,
    height: u32,
) -> u64 {
    let blocks = height.saturating_sub(for_height) as f64;
    let percentage_as_decimal = percentage as f64 / 100.0;
    // optimized version of `f64::powf(1.0 + percentage_as_decimal, blocks)`
    // x^y = e^(y*ln(x))
    // we gain around 22%
    let multiple = f64::exp(blocks * f64::ln(1.0 + percentage_as_decimal));
    let mut approx = new_exec_price as f64 * multiple;
    // account for rounding errors and take a slightly higher value
    const ROUNDING_ERROR_CUTOFF: f64 = 16948547188989277.0;
    if approx > ROUNDING_ERROR_CUTOFF {
        const ROUNDING_ERROR_COMPENSATION: f64 = 2000.0;
        approx += ROUNDING_ERROR_COMPENSATION;
    }
    // `f64` over `u64::MAX` are cast to `u64::MAX`
    approx.ceil() as u64
}
