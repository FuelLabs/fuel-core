use nalgebra::{
    DMatrix,
    DVector,
};

fn design_matrix(points: &Vec<(f64, f64)>) -> DMatrix<f64> {
    let x_data = points.iter().map(|(x, _)| *x).collect::<Vec<_>>();
    let n = points.len();
    let ones = DVector::from_element(n, 1.0);
    let x = DVector::from_column_slice(x_data.as_slice());
    let x2 = x.component_mul(&x);
    DMatrix::from_columns(&[ones, x, x2])
}

fn quadratic_regression(points: &Vec<(f64, f64)>) -> anyhow::Result<(f64, f64, f64)> {
    let y_data = points.iter().map(|(_, y)| *y).collect::<Vec<_>>();
    let y_vec = DVector::from_column_slice(y_data.as_slice());

    let design_matrix = design_matrix(points);
    let design_matrix_transpose = design_matrix.transpose();
    // (X^T * X)^-1 * X^T
    let xtx_inv_xt = (design_matrix_transpose.clone() * design_matrix)
        .try_inverse()
        .ok_or(anyhow::anyhow!(
            "Matrix is singular and cannot be inverted."
        ))?
        * design_matrix_transpose;

    let v = xtx_inv_xt * y_vec;
    let (c, b, a) = (v[0], v[1], v[2]);
    Ok((a, b, c))
}

/// Residual sum of squares
fn sse(points: &Vec<(f64, f64)>, a: f64, b: f64, c: f64) -> f64 {
    let real_values = points.iter().map(|(_, y)| *y);
    let expected_values = points.iter().map(|(x, _)| a * x.powi(2) + b * x + c);
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
        dbg!("SSE is 0");
        1.0
    } else {
        1.0 - sse / sst
    }
}

/// Quantification of linear fit.
/// A value of 1.0 represents perfect linear fit.
/// A value of 0.0 represents no linear fit.
pub fn fit_quadratic(points: &Vec<(f64, f64)>) -> anyhow::Result<f64> {
    let (a, b, c) = quadratic_regression(points)?;
    dbg!(a, b, c);
    let sse = sse(points, a, b, c);
    let sst = sst(points);
    let fit = r_squared(sse, sst);
    Ok(fit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadratic_regression_with_linear() {
        let function = |x: f64| {
            let slope = 0.5;
            let intercept = 4.5;
            slope * x + intercept
        };
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x)));
        let data = points.take(50).collect();
        let fit = fit_quadratic(&data).expect("Expected quadratic fit");

        dbg!(fit);
    }

    #[test]
    fn quadratic_regression_with_quadratic() {
        let function = |x: f64| 0.2 * x.powi(2) - 4.0 * x + 1.0;
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x)));
        let data = points.take(50).collect();
        let fit = fit_quadratic(&data).expect("Expected quadratic fit");

        dbg!(fit);
    }

    #[test]
    fn quadratic_regression_with_quadratic_convex() {
        let function = |x: f64| -1.0 * x.powi(2) + 1.0 * x + 1.0;
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x)));
        let data = points.take(50).collect();
        let fit = fit_quadratic(&data).expect("Expected quadratic fit");

        dbg!(fit);
    }
}
