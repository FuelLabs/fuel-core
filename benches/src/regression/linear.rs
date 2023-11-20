use crate::LinearCoefficients;

pub fn linear_regression(points: &[(f64, f64)]) -> LinearCoefficients {
    let avg_x = points.iter().map(|(x, _)| *x).sum::<f64>() / points.len() as f64;
    let avg_y = points.iter().map(|(_, y)| *y).sum::<f64>() / points.len() as f64;
    let covariance: f64 = points
        .iter()
        .map(|(x, y)| (*x - avg_x) * (*y - avg_y))
        .sum();
    let variance: f64 = points.iter().map(|(x, _)| (*x - avg_x).powi(2)).sum();
    let slope = covariance / variance;
    let intercept = avg_y - slope * avg_x;
    LinearCoefficients { slope, intercept }
}

#[cfg(test)]
mod test {
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use test_case::test_case;

    use super::linear_regression;
    use crate::{
        within_epsilon,
        ConstantCoefficients,
        LinearCoefficients,
        Resolve,
    };

    fn apply_noise<R: Rng>(value: f64, bound: f64, rng: &mut R) -> f64 {
        let delta = rng.gen_range(-bound..bound);
        value + delta
    }

    fn near_equal(lhs: f64, rhs: f64, epsilon: f64) -> bool {
        let diff = lhs - rhs;
        within_epsilon(diff, 0.0, epsilon)
    }

    #[test_case(ConstantCoefficients::new( 10.00); "y = 10")]
    #[test_case(ConstantCoefficients::new(-01.00); "y = -1")]
    #[test_case(ConstantCoefficients::new( 04.50); "y = 4.5")]
    #[test_case(ConstantCoefficients::new( 01.25); "y = 0.25")]
    fn linear_regression_on_constant_function(coefficients: ConstantCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let function = |x: f64| coefficients.resolve(x);
        let noise = { |(x, y): (f64, f64)| (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();
        let linear_coefficients = linear_regression(&data);

        const EPSILON: f64 = 0.01;
        let ConstantCoefficients { y } = coefficients;
        assert!(near_equal(linear_coefficients.slope, 0.0, EPSILON));
        assert!(near_equal(linear_coefficients.intercept, y, EPSILON));
    }

    #[test_case(LinearCoefficients::new( 10.00, 05.00); "y = 10x + 5")]
    #[test_case(LinearCoefficients::new(-01.00, 10.00); "y = -x + 10")]
    #[test_case(LinearCoefficients::new( 04.50, 02.00); "y = 4.5x + 2")]
    #[test_case(LinearCoefficients::new( 00.25, 50.00); "y = 0.25x + 50")]
    fn linear_regression_on_linear_function(coefficients: LinearCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let function = |x: f64| coefficients.resolve(x);
        let noise = { |(x, y): (f64, f64)| (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();
        let linear_coefficients = linear_regression(&data);

        const EPSILON: f64 = 0.01;
        let LinearCoefficients { slope, intercept } = coefficients;
        assert!(near_equal(linear_coefficients.slope, slope, EPSILON));
        assert!(near_equal(
            linear_coefficients.intercept,
            intercept,
            EPSILON
        ));
    }
}
