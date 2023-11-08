/// Constant Coefficients
#[derive(Debug, Clone, Copy)]
pub struct ConstantCoefficients {
    pub y: f64,
}

impl ConstantCoefficients {
    pub fn new(y: f64) -> Self {
        Self { y }
    }
}

impl ConstantCoefficients {
    /// Sum of square residuals
    fn ssr(&self, points: &Vec<(f64, f64)>) -> f64 {
        let ConstantCoefficients { y } = self;
        let real_values = points.iter().map(|(_, y)| *y);
        let predicted_values = points.iter().map(|(_, _)| y);
        real_values
            .zip(predicted_values)
            .map(|(real, predicted)| real - predicted)
            .map(|v| v.powi(2))
            .sum()
    }

    /// Sum of square errors
    fn sst(&self, points: &Vec<(f64, f64)>) -> f64 {
        let real_values = points.iter().map(|(_, y)| *y);
        let avg_y = real_values.clone().sum::<f64>() / points.len() as f64;
        real_values
            .map(|real| real - avg_y)
            .map(|v| v.powi(2))
            .sum()
    }

    /// R^2 = 1 - (ssr/sst)
    pub fn r_squared(&self, points: &Vec<(f64, f64)>) -> f64 {
        let ssr = self.ssr(points);
        let sst = self.sst(points);
        1.0 - ssr / sst
    }
}

// /// Quantification of constant fit.
// /// A value of 1.0 represents perfect constant fit.
// /// A value of 0.0 represents no constant fit.
// pub fn fit_constant(points: &Vec<(f64, f64)>) -> (f64, ConstantCoefficients) {
//     let coefficients = constant_regression(points).expect("");
//     let ssr = ssr(points, coefficients);
//     let sst = sst(points);
//     let fit = r_squared(ssr, sst);
//     (fit, coefficients)
// }
//
// #[cfg(test)]
// mod tests {
//     use crate::model::apply_noise;
//     use rand::{
//         prelude::StdRng,
//         SeedableRng,
//     };
//     use test_case::test_case;
//
//     use super::*;
//
//     // #[test_case(ConstantCoefficients::new(5.0))]
//     // #[test_case(ConstantCoefficients::new(-11.5))]
//     // #[test_case(ConstantCoefficients::new(572.1))]
//     // #[test_case(ConstantCoefficients::new(0.0))]
//     // fn test_fit_linear_with_linear_function_exact(coefficients: ConstantCoefficients) {
//     //     let function = |_x: f64| {
//     //         let ConstantCoefficients { c } = coefficients;
//     //         c
//     //     };
//     //     let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
//     //     let points = independent.map(|x| (x, function(x)));
//     //     let data = points.take(50).collect();
//     //     let (fit, _) = fit_constant(&data);
//     //     assert_eq!(fit, 1.0);
//     // }
//
//     #[test_case(ConstantCoefficients::new(5.0))]
//     #[test_case(ConstantCoefficients::new(-11.5))]
//     #[test_case(ConstantCoefficients::new(572.1))]
//     #[test_case(ConstantCoefficients::new(0.0))]
//     fn test_fit_linear_with_linear_function_noisy(coefficients: ConstantCoefficients) {
//         let mut rng = rand::thread_rng();
//         let mut function = |_x: f64| {
//             let ConstantCoefficients { c } = coefficients;
//             apply_noise(c * c, 10000.0, &mut rng)
//         };
//         let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
//         let points = independent.map(|x| (x, function(x)));
//         let data = points.take(50).collect();
//         let (fit, _) = fit_constant(&data);
//         dbg!(fit);
//         let v = within_epsilon(fit, 1.0, 0.05);
//         assert!(v);
//     }
// }
