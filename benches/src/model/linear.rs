use super::within_epsilon;

/// Linear Coefficients
#[derive(Debug, Clone, Copy)]
pub struct LinearCoefficients {
    pub slope: f64,
    pub intercept: f64,
}

impl LinearCoefficients {
    pub fn new(slope: f64, intercept: f64) -> Self {
        Self { slope, intercept }
    }

    pub fn is_linear(&self) -> bool {
        let slope_is_nonzero = !within_epsilon(self.slope, 0.0, 0.01);
        slope_is_nonzero
    }

    /// Residual sum of squares
    fn ssr(&self, points: &Vec<(f64, f64)>) -> f64 {
        let LinearCoefficients { slope, intercept } = self;
        let real_values = points.iter().map(|(_, y)| *y);
        let predicted = points.iter().map(|(x, _)| *slope * *x + *intercept);
        real_values
            .zip(predicted)
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
    pub fn r_squared(&self, points: &Vec<(f64, f64)>) -> f64 {
        let ssr = self.ssr(points);
        let sst = self.sst(points);
        if sst == 0.0 {
            1.0
        } else {
            1.0 - ssr / sst
        }
    }
}

pub fn linear_regression(points: &Vec<(f64, f64)>) -> LinearCoefficients {
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
        SeedableRng,
    };

    use super::{
        super::{
            apply_noise,
            within_epsilon,
        },
        linear_regression,
    };

    // #[test]
    // fn linear_regression_on_constant_function() {
    //     let mut rng = StdRng::seed_from_u64(0xf00df00d);
    //     let function = |_x: f64| 300.0;
    //     let noise = { |(x, y): (f64, f64)| (x, apply_noise(y, 0.5, &mut rng)) };
    //     let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
    //     let points = independent.map(|x| (x, function(x))).map(noise);
    //     let data = points.take(50).collect();
    //     let coefficients = linear_regression(&data);
    //     let fit = coefficients.r_squared(&data);
    //     let epsilon = 0.01;
    //     assert!(coefficients.is_constant());
    //     assert!(within_epsilon(fit, 1.0, epsilon));
    // }

    #[test]
    fn linear_regression_on_linear_function() {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let function = |x: f64| {
            let slope = 0.5;
            let intercept = 4.5;
            slope * x + intercept
        };
        let noise = { |(x, y): (f64, f64)| (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(50).collect();
        let coefficients = linear_regression(&data);
        let fit = coefficients.r_squared(&data);
        let epsilon = 0.01;
        assert!(coefficients.is_linear());
        assert!(within_epsilon(fit, 1.0, epsilon));
    }

    #[test]
    fn linear_regression_on_quadratic_function() {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let function = |x: f64| {
            // 2x^2 + 3x + 1
            2.0 * x.powi(2) + 3.0 * x + 1.0
        };
        let nudge = { |(x, y): (f64, f64)| (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(nudge);
        let data = points.take(50).collect();
        let coefficients = linear_regression(&data);
        let fit = coefficients.r_squared(&data);
        let epsilon = 0.05;
        assert!(!within_epsilon(fit, 1.0, epsilon));
    }
}
