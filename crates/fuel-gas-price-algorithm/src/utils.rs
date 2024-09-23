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
    const ROUNDING_ERROR_CUTOFF: f64 = 16948547188989277.0;
    if approx > ROUNDING_ERROR_CUTOFF {
        const ROUNDING_ERROR_COMPENSATION: f64 = 2000.0;
        approx += ROUNDING_ERROR_COMPENSATION;
    }
    // `f64` over `u64::MAX` are cast to `u64::MAX`
    approx.ceil() as u64
}

pub(crate) fn safe_signed_abs(n: i128) -> i128 {
    let n = if n == i128::MIN {
        n.saturating_add(1)
    } else {
        n
    };
    debug_assert!(n != i128::MIN);
    n.abs()
}

#[cfg(test)]
mod tests {
    use crate::utils::safe_signed_abs;

    #[test]
    fn safe_signed_abs_does_not_overflow_on_min_value() {
        let abs = safe_signed_abs(i128::MIN);
        assert_eq!(abs, i128::MAX);
    }
}
