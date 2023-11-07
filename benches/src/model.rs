mod linear;
mod quadratic;

pub use linear::fit_linear;
pub use quadratic::fit_quadratic;

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
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x)));
        let data = points.take(50).collect();

        let linear_fit = fit_linear(&data);
        let quadratic_fit = fit_quadratic(&data).expect("");

        assert!(linear_fit > quadratic_fit);
    }

    #[test]
    fn quadratic_regression_with_quadratic() {
        let function = |x: f64| 1.0 * x.powi(2) + 1.0 * x + 1.0;
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x)));
        let data = points.take(50).collect();

        let linear_fit = fit_linear(&data);
        let quadratic_fit = fit_quadratic(&data).expect("");
        dbg!(linear_fit, quadratic_fit);

        assert!(linear_fit < quadratic_fit);
    }

    #[test]
    fn quadratic_regression_with_quadratic_2() {
        let function = |x: f64| -1.0 * x.powi(2) + 1.0 * x + 1.0;
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x)));
        let data = points.take(50).collect();

        let linear_fit = fit_linear(&data);
        let quadratic_fit = fit_quadratic(&data).expect("");
        dbg!(linear_fit, quadratic_fit);

        assert!(linear_fit < quadratic_fit);
    }
}
