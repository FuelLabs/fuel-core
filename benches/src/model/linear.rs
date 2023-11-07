fn linear_regression(points: &Vec<(f64, f64)>) -> (f64, f64) {
    let avg_x = points.iter().map(|(x, _)| *x).sum::<f64>() / points.len() as f64;
    let avg_y = points.iter().map(|(_, y)| *y).sum::<f64>() / points.len() as f64;
    let covariance: f64 = points
        .iter()
        .map(|(x, y)| (*x - avg_x) * (*y - avg_y))
        .sum();
    let variance: f64 = points.iter().map(|(x, _)| (*x - avg_x).powi(2)).sum();
    let slope = covariance / variance;
    let intercept = avg_y - slope * avg_x;
    (slope, intercept)
}

/// Residual sum of squares
fn sse(points: &Vec<(f64, f64)>, slope: f64, intercept: f64) -> f64 {
    let real_values = points.iter().map(|(_, y)| *y as f64);
    let expected_values = points.iter().map(|(x, _)| slope * (*x as f64) + intercept);
    real_values
        .zip(expected_values)
        .map(|(real, expected)| real - expected)
        .map(|v| v.powi(2))
        .sum()
}

/// Total sum of squares
fn sst(points: &Vec<(f64, f64)>) -> f64 {
    let real_values = points.iter().map(|(_, y)| *y);
    let avg_y = real_values.clone().sum::<f64>() / points.len() as f64;
    real_values
        .map(|real| real - avg_y)
        .map(|v| v.powi(2))
        .sum()
}

/// R^2 = 1 - (sst/sse)
fn r_squared(sse: f64, sst: f64) -> f64 {
    if sst == 0.0 {
        1.0
    } else {
        1.0 - sse / sst
    }
}

/// Quantification of linear fit.
/// A value of 1.0 represents perfect linear fit.
/// A value of 0.0 represents no linear fit.
pub fn fit_linear(points: &Vec<(f64, f64)>) -> f64 {
    let (slope, intercept) = linear_regression(points);
    let sse = sse(points, slope, intercept);
    let sst = sst(points);
    r_squared(sse, sst)
}

#[cfg(test)]
mod test {
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

    use super::fit_linear;

    fn within_epsilon(value: f64, expected: f64, epsilon: f64) -> bool {
        let range = (expected - epsilon)..=(expected + epsilon);
        range.contains(&value)
    }

    fn apply_random<R: Rng>(value: f64, bound: f64, rng: &mut R) -> f64 {
        let delta = rng.gen_range(-bound..bound);
        value + delta
    }

    #[test]
    fn test_fit_linear_with_linear_function_exact() {
        let function = |x: f64| {
            let slope = 0.5;
            let intercept = 4.5;
            slope * x + intercept
        };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x)));
        let data = points.take(50).collect();
        let fit = fit_linear(&data);
        assert_eq!(fit, 1.0);
    }

    #[test]
    fn test_fit_linear_with_linear_function_noise() {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let function = |x: f64| {
            let slope = 0.5;
            let intercept = 4.5;
            slope * x + intercept
        };
        let noise = { |(x, y): (f64, f64)| (x, apply_random(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(50).collect();
        let fit = fit_linear(&data);
        let epsilon = 0.01;
        assert!(within_epsilon(fit, 1.0, epsilon));
    }

    #[test]
    fn test_fit_linear_with_quadratic_function() {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let function = |x: f64| {
            // 2x^2 + 3x + 1
            2.0 * x.powi(2) + 3.0 * x + 1.0
        };
        let nudge = { |(x, y): (f64, f64)| (x, apply_random(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(nudge);
        let data = points.take(50).collect();
        let fit = fit_linear(&data);
        let epsilon = 0.05;
        assert!(!within_epsilon(fit, 1.0, epsilon));
    }
}
