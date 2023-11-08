use super::within_epsilon;

use nalgebra::{
    DMatrix,
    DVector,
};

/// Quadratic coefficients
#[derive(Debug, Clone, Copy)]
pub struct QuadraticCoefficients {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

impl QuadraticCoefficients {
    pub fn new(a: f64, b: f64, c: f64) -> Self {
        Self { a, b, c }
    }

    pub fn is_zero(&self) -> bool {
        let a_is_zero = within_epsilon(self.a, 0.0, 0.01);
        let b_is_zero = within_epsilon(self.b, 0.0, 0.01);
        let c_is_zero = within_epsilon(self.c, 0.0, 0.01);
        a_is_zero && b_is_zero && c_is_zero
    }

    pub fn is_constant(&self) -> bool {
        let a_is_zero = within_epsilon(self.a, 0.0, 0.01);
        let b_is_zero = within_epsilon(self.b, 0.0, 0.01);
        let c_is_nonzero = !within_epsilon(self.c, 0.0, 0.01);
        a_is_zero && b_is_zero && c_is_nonzero
    }

    pub fn is_linear(&self) -> bool {
        let a_is_zero = within_epsilon(self.a, 0.0, 0.01);
        let b_is_nonzero = !within_epsilon(self.b, 0.0, 0.01);
        a_is_zero && b_is_nonzero
    }

    pub fn is_quadratic(&self) -> bool {
        let a_is_nonzero = !within_epsilon(self.a, 0.0, 0.01);
        let a_is_number = self.a.is_finite() && self.a.is_normal();
        a_is_nonzero && a_is_number
    }

    /// Residual sum of squares
    fn ssr(&self, points: &Vec<(f64, f64)>) -> f64 {
        let QuadraticCoefficients { a, b, c } = self;
        let real_values = points.iter().map(|(_, y)| *y);
        let f = |x: f64| a * x.powi(2) + b * x + c;
        let predicted_values = points.iter().map(|(x, _)| f(*x));
        real_values
            .zip(predicted_values)
            .map(|(real, predicted)| real - predicted)
            .map(|v| v.powi(2))
            .sum()
    }

    /// Total sum of squares
    fn sst(&self, points: &Vec<(f64, f64)>) -> f64 {
        let real_values = points.iter().map(|(_, y)| *y);
        let avg_y = real_values.clone().sum::<f64>() / points.len() as f64;
        real_values
            .map(|real| real - avg_y)
            .map(|v| v.powi(2))
            .sum()
    }

    /// R^2 = 1 - (sst/sse)
    pub(crate) fn r_squared(&self, points: &Vec<(f64, f64)>) -> f64 {
        let ssr = self.ssr(points);
        let sst = self.sst(points);
        if sst == 0.0 {
            1.0
        } else {
            1.0 - ssr / sst
        }
    }
}

fn design_matrix(points: &Vec<(f64, f64)>) -> DMatrix<f64> {
    let x_data = points.iter().map(|(x, _)| *x).collect::<Vec<_>>();
    let n = points.len();
    let ones = DVector::from_element(n, 1.0);
    let x = DVector::from_column_slice(x_data.as_slice());
    let x2 = x.component_mul(&x);
    DMatrix::from_columns(&[ones, x, x2])
}

pub fn quadratic_regression(
    points: &Vec<(f64, f64)>,
) -> anyhow::Result<QuadraticCoefficients> {
    let y_data = points.iter().map(|(_, y)| *y).collect::<Vec<_>>();
    let y_vec = DVector::from_column_slice(y_data.as_slice());
    let design_matrix = design_matrix(points);
    let design_matrix_transpose = design_matrix.transpose();
    // (X^T * X)^-1 * X^T
    let xtx_inv_xt = (design_matrix_transpose.clone() * design_matrix)
        .try_inverse()
        .ok_or(anyhow::anyhow!("Matrix cannot be inverted."))?
        * design_matrix_transpose;

    let v = xtx_inv_xt * y_vec;
    let (c, b, a) = (v[0], v[1], v[2]);
    let coefficients = QuadraticCoefficients { a, b, c };
    Ok(coefficients)
}

#[cfg(test)]
mod tests {
    use super::{
        super::*,
        *,
    };
    use crate::model::{
        ConstantCoefficients,
        LinearCoefficients,
    };
    use rand::thread_rng;
    use test_case::test_case;

    #[test_case(ConstantCoefficients::new( 10.00); "y = 10")]
    #[test_case(ConstantCoefficients::new(-01.00); "y = -1")]
    #[test_case(ConstantCoefficients::new( 04.50); "y = 4.5")]
    #[test_case(ConstantCoefficients::new( 01.25); "y = 0.25")]
    fn quadratic_regression_on_constant_function(coefficients: ConstantCoefficients) {
        let mut rng = thread_rng();
        let function = |_x: f64| {
            let ConstantCoefficients { y: c } = coefficients;
            c
        };
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 1.0, &mut rng)) };
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(50_000).collect();
        let quadratic_coefficients =
            quadratic_regression(&data).expect("Expected quadratic regression");
        dbg!(
            &quadratic_coefficients,
            quadratic_coefficients.is_constant(),
            quadratic_coefficients.is_linear(),
            quadratic_coefficients.is_quadratic()
        );
        let quadratic_fit = quadratic_coefficients.r_squared(&data);

        let linear_coefficients = linear_regression(&data);
        let linear_fit = linear_coefficients.r_squared(&data);

        let constant_coefficients: ConstantCoefficients = linear_coefficients.into();
        let constant_fit = constant_coefficients.r_squared(&data);

        dbg!(quadratic_fit, linear_fit, constant_fit);
    }

    #[test_case(LinearCoefficients::new( 10.00, 05.00); "y = 10x + 5")]
    #[test_case(LinearCoefficients::new(-01.00, 10.00); "y = -x + 10")]
    #[test_case(LinearCoefficients::new( 04.50, 02.00); "y = 4.5x + 2")]
    #[test_case(LinearCoefficients::new( 00.25, 50.00); "y = 0.25x + 50")]
    fn quadratic_regression_on_linear_function(coefficients: LinearCoefficients) {
        let mut rng = thread_rng();
        let function = |x: f64| {
            let LinearCoefficients { slope, intercept } = coefficients;
            slope * x + intercept
        };
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(50).collect();
        let quadratic_coefficients =
            quadratic_regression(&data).expect("Expected quadratic regression");

        let quadratic_fit = quadratic_coefficients.r_squared(&data);

        let linear_coefficients = linear_regression(&data);
        let linear_fit = linear_coefficients.r_squared(&data);

        dbg!(quadratic_fit, linear_fit);
    }

    // #[test_case(QuadraticCoefficients::new( 02.00,  10.00); "y = log2(x) + 10")]
    // #[test_case(QuadraticCoefficients::new( 03.00, -01.00); "y = log3(x) - 1")]
    // #[test_case(QuadraticCoefficients::new( 2.718,  04.50); "y = ln(x) + 4.5")]
    // #[test_case(QuadraticCoefficients::new( 00.00,  50.00); "y = log0(x) + 50")]
    // fn quadratic_regression_on_logarithmic_function(
    //     coefficients: LogarithmicCoefficients,
    // ) {
    //     let mut rng = thread_rng();
    //     let function = |x: f64| {
    //         let QuadraticCoefficients { a, b, c } = coefficients;
    //         a * x.powi(2) - b * x + c
    //     };
    //     let noise =
    //         |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 0.5, &mut rng)) };
    //     let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
    //     let points = independent.map(|x| (x, function(x))).map(noise);
    //     let data = points.take(50).collect();
    //     let quadratic_coefficients =
    //         quadratic_regression(&data).expect("Expected quadratic fit");
    //     dbg!(
    //         &quadratic_coefficients,
    //         quadratic_coefficients.is_constant(),
    //         quadratic_coefficients.is_linear(),
    //         quadratic_coefficients.is_quadratic()
    //     );
    //     let quadratic_fit = quadratic_coefficients.r_squared(&data);
    //
    //     let linear_coefficients = linear_regression(&data);
    //     let linear_fit = linear_coefficients.r_squared(&data);
    //
    //     dbg!(quadratic_fit, linear_fit);
    // }
}
