use nalgebra::{
    DMatrix,
    DVector,
};

use crate::QuadraticCoefficients;

fn design_matrix(points: &[(f64, f64)]) -> DMatrix<f64> {
    let x_data = points.iter().map(|(x, _)| *x).collect::<Vec<_>>();
    let n = points.len();
    let ones = DVector::from_element(n, 1.0);
    let x = DVector::from_column_slice(x_data.as_slice());
    let x2 = x.component_mul(&x);
    DMatrix::from_columns(&[ones, x, x2])
}

pub fn quadratic_regression(
    points: &[(f64, f64)],
) -> anyhow::Result<QuadraticCoefficients> {
    let y_data = points.iter().map(|(_, y)| *y).collect::<Vec<_>>();
    let y_vec = DVector::from_column_slice(y_data.as_slice());
    let design_matrix = design_matrix(points);
    let design_matrix_transpose = design_matrix.transpose();
    // (X^T * X)^-1 * X^T
    let xtx_inv_xt = (design_matrix_transpose.clone() * design_matrix)
        .try_inverse()
        .ok_or(anyhow::anyhow!(
            "Regression failed: Matrix cannot be inverted."
        ))?
        * design_matrix_transpose;

    let v = xtx_inv_xt * y_vec;
    let (c, b, a) = (v[0], v[1], v[2]);
    let coefficients = QuadraticCoefficients { a, b, c };
    Ok(coefficients)
}

#[cfg(test)]
mod tests {
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use test_case::test_case;

    use super::quadratic_regression;
    use crate::{
        model::{
            ConstantCoefficients,
            LinearCoefficients,
            QuadraticCoefficients,
        },
        within_epsilon,
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
    fn quadratic_regression_on_constant_function(coefficients: ConstantCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |x: f64| coefficients.resolve(x);
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();
        let quadratic_coefficients =
            quadratic_regression(&data).expect("Expected quadratic regression");

        // Loose fit
        const EPSILON: f64 = 0.1;
        let ConstantCoefficients { y } = coefficients;
        assert!(near_equal(quadratic_coefficients.a, 0.0, EPSILON));
        assert!(near_equal(quadratic_coefficients.b, 0.0, EPSILON));
        assert!(near_equal(quadratic_coefficients.c, y, EPSILON));
    }

    #[test_case(LinearCoefficients::new( 10.00, 05.00); "y = 10x + 5")]
    #[test_case(LinearCoefficients::new(-01.00, 10.00); "y = -x + 10")]
    #[test_case(LinearCoefficients::new( 04.50, 02.00); "y = 4.5x + 2")]
    #[test_case(LinearCoefficients::new( 00.25, 50.00); "y = 0.25x + 50")]
    fn quadratic_regression_on_linear_function(coefficients: LinearCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |x: f64| coefficients.resolve(x);
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();
        let quadratic_coefficients =
            quadratic_regression(&data).expect("Expected quadratic regression");

        // Loose fit
        const EPSILON: f64 = 0.1;
        let LinearCoefficients { slope, intercept } = coefficients;
        assert!(near_equal(quadratic_coefficients.a, 0.0, EPSILON));
        assert!(near_equal(quadratic_coefficients.b, slope, EPSILON));
        assert!(near_equal(quadratic_coefficients.c, intercept, EPSILON));
    }

    #[test_case(QuadraticCoefficients::new( 02.00,  05.00,  12.00); "y = 2x^2 + 5x + 12.0")]
    #[test_case(QuadraticCoefficients::new(-01.00, -01.00, -01.00); "y = -x^2 - x - 1.0")]
    #[test_case(QuadraticCoefficients::new( 03.50,  00.50,  20.00); "y = 3.5x^2 + 0.5x + 20.0")]
    #[test_case(QuadraticCoefficients::new(-05.00,  20.00,  00.05); "y = -5x^2 + 20x + 0.05")]
    fn quadratic_regression_on_quadratic_function(coefficients: QuadraticCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |x: f64| coefficients.resolve(x);
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 1.0, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();
        let quadratic_coefficients =
            quadratic_regression(&data).expect("Expected quadratic regression");

        const EPSILON: f64 = 0.01;
        let QuadraticCoefficients { a, b, c } = coefficients;
        assert!(near_equal(quadratic_coefficients.a, a, EPSILON));
        assert!(near_equal(quadratic_coefficients.b, b, EPSILON));
        assert!(near_equal(quadratic_coefficients.c, c, EPSILON));
    }
}
