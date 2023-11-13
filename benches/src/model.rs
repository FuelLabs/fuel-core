mod constant;
mod linear;
mod logarithmic;
mod quadratic;

use crate::regression::{
    linear_regression,
    quadratic_regression,
};
use anyhow::anyhow;

pub use constant::ConstantCoefficients;
pub use linear::LinearCoefficients;
pub use logarithmic::LogarithmicCoefficients;
pub use quadratic::QuadraticCoefficients;

pub trait Fit {
    fn fit(&self, points: &[(f64, f64)]) -> f64;
}

pub trait Resolve {
    fn resolve(&self, x: f64) -> f64;
}

pub trait Error {
    fn squared_error(&self, x: f64, y: f64) -> f64;
    fn squared_errors(&self, points: &[(f64, f64)]) -> Vec<f64>;
}

impl<T: Resolve> Error for T {
    fn squared_error(&self, x: f64, y: f64) -> f64 {
        let real_value = y;
        let predicted_value = self.resolve(x);
        let difference = real_value - predicted_value;
        difference.powi(2)
    }

    fn squared_errors(&self, points: &[(f64, f64)]) -> Vec<f64> {
        let real_values = points.iter().map(|(_, y)| *y);
        let predicted_values = points.iter().map(|(x, _)| self.resolve(*x));
        real_values
            .zip(predicted_values)
            .map(|(real, predicted)| real - predicted)
            .map(|difference| difference.powi(2))
            .collect()
    }
}

pub fn within_epsilon(value: f64, expected: f64, epsilon: f64) -> bool {
    let range = (expected - epsilon)..=(expected + epsilon);
    range.contains(&value)
}

impl From<QuadraticCoefficients> for LinearCoefficients {
    fn from(quadratic: QuadraticCoefficients) -> Self {
        LinearCoefficients {
            slope: quadratic.b,
            intercept: quadratic.c,
        }
    }
}

impl From<LinearCoefficients> for ConstantCoefficients {
    fn from(linear: LinearCoefficients) -> Self {
        ConstantCoefficients {
            y: linear.intercept,
        }
    }
}

/// Model
#[derive(Debug, Clone)]
pub enum Model {
    Zero,
    Constant(ConstantCoefficients),
    Linear(LinearCoefficients),
    Quadratic(QuadraticCoefficients),
    Other,
}

impl Model {
    pub fn is_zero(&self) -> bool {
        matches!(self, Model::Zero)
    }

    pub fn is_constant(&self) -> bool {
        matches!(self, Model::Constant(_))
    }

    pub fn is_linear(&self) -> bool {
        matches!(self, Model::Linear(_))
    }

    pub fn is_quadratic(&self) -> bool {
        matches!(self, Model::Quadratic(_))
    }

    pub fn is_other(&self) -> bool {
        matches!(self, Model::Other)
    }

    pub fn resolve_point(&self, x: f64) -> anyhow::Result<f64> {
        match self {
            Model::Zero => Ok(0.0),
            Model::Constant(coefficients) => {
                let y = coefficients.resolve(x);
                Ok(y)
            }
            Model::Linear(coefficients) => {
                let y = coefficients.resolve(x);
                Ok(y)
            }
            Model::Quadratic(coefficients) => {
                let y = coefficients.resolve(x);
                Ok(y)
            }
            Model::Other => Err(anyhow!("Unable to resolve with undefined model")),
        }
    }

    pub fn squared_error(&self, x: f64, y: f64) -> anyhow::Result<f64> {
        match self {
            Model::Zero => Ok(y.powi(2)),
            Model::Constant(coefficients) => {
                let error = coefficients.squared_error(x, y);
                Ok(error)
            }
            Model::Linear(coefficients) => {
                let error = coefficients.squared_error(x, y);
                Ok(error)
            }
            Model::Quadratic(coefficients) => {
                let error = coefficients.squared_error(x, y);
                Ok(error)
            }
            Model::Other => Err(anyhow!(
                "Unable to calculate squared error with undefined model"
            )),
        }
    }

    pub fn squared_errors(&self, points: &[(f64, f64)]) -> anyhow::Result<Vec<f64>> {
        match self {
            Model::Zero => Ok(vec![0.0; points.len()]),
            Model::Constant(coefficients) => {
                let errors = coefficients.squared_errors(points);
                Ok(errors)
            }
            Model::Linear(coefficients) => {
                let errors = coefficients.squared_errors(points);
                Ok(errors)
            }
            Model::Quadratic(coefficients) => {
                let errors = coefficients.squared_errors(points);
                Ok(errors)
            }
            Model::Other => Err(anyhow!(
                "Unable to calculate squared errors with undefined model"
            )),
        }
    }
}

/// Generate a model that estimates the points in the dataset
pub fn estimate_model(points: &[(f64, f64)]) -> anyhow::Result<Model> {
    let coefficients = linear_regression(points);
    if coefficients.is_zero() {
        // Zero
        Ok(Model::Zero)
    } else if coefficients.is_constant() {
        // Constant
        let LinearCoefficients { intercept, .. } = coefficients;
        let constant_coefficients = ConstantCoefficients { y: intercept };
        Ok(Model::Constant(constant_coefficients))
    } else if coefficients.is_linear() {
        // Linear
        Ok(Model::Linear(coefficients))
    } else {
        // Other
        Ok(Model::Other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use test_case::test_case;

    fn apply_noise<R: Rng>(value: f64, bound: f64, rng: &mut R) -> f64 {
        let delta = rng.gen_range(-bound..bound);
        value + delta
    }

    #[test]
    fn evaluate_zero_function() {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |_x: f64| 0.0;
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 1.0, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();

        let model = estimate_model(&data).expect("Expected evaluation");

        assert!(model.is_zero());
    }

    #[test_case(ConstantCoefficients::new( 12.00); "y = 12.0")]
    #[test_case(ConstantCoefficients::new(-01.00); "y = -1.0")]
    #[test_case(ConstantCoefficients::new( 20.00); "y = 20.0")]
    #[test_case(ConstantCoefficients::new( 00.05); "y = 0.05")]
    fn evaluate_constant_function(coefficients: ConstantCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |_x: f64| {
            let ConstantCoefficients { y } = coefficients;
            y
        };
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 1.0, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();

        let model = estimate_model(&data).expect("Expected evaluation");

        assert!(model.is_constant());
    }

    #[test_case(LinearCoefficients::new( 05.00,  12.00); "y = 5x + 12.0")]
    #[test_case(LinearCoefficients::new(-01.00, -01.00); "y = -x - 1.0")]
    #[test_case(LinearCoefficients::new( 00.50,  20.00); "y = x/2 + 200.0")]
    #[test_case(LinearCoefficients::new( 20.00,  00.05); "y = 20x + 0.05")]
    fn evaluate_linear_function(coefficients: LinearCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |x: f64| {
            let LinearCoefficients { slope, intercept } = coefficients;
            slope * x + intercept
        };
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 1.0, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();

        let model = estimate_model(&data).expect("Expected evaluation");

        assert!(model.is_linear());
    }

    #[test_case(QuadraticCoefficients::new( 02.00,  05.00,  12.00); "y = 2x^2 + 5x + 12.0")]
    #[test_case(QuadraticCoefficients::new(-01.00, -01.00, -01.00); "y = -x^2 - x - 1.0")]
    #[test_case(QuadraticCoefficients::new( 03.50,  00.50,  20.00); "y = 3.5x^2 + 0.5x + 20.0")]
    #[test_case(QuadraticCoefficients::new(-05.00,  20.00,  00.05); "y = -5x^2 + 20x + 0.05")]
    fn evaluate_quadratic_function(coefficients: QuadraticCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |x: f64| {
            let QuadraticCoefficients { a, b, c } = coefficients;
            a * x.powi(2) + b * x + c
        };
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 1.0, &mut rng)) };
        let independent = core::iter::successors(Some(1.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();

        let model = estimate_model(&data).expect("Expected evaluation");

        assert!(model.is_quadratic());
    }

    #[test_case(LogarithmicCoefficients::new( 02.00,  10.00); "y = log2(x) + 10")]
    #[test_case(LogarithmicCoefficients::new( 03.00, -01.00); "y = log3(x) - 1")]
    #[test_case(LogarithmicCoefficients::new( 05.00,  50.00); "y = log5(x) + 50")]
    #[test_case(LogarithmicCoefficients::new( 08.00,  50.00); "y = log8(x) + 50")]
    fn evaluate_logarithmic_function(coefficients: LogarithmicCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |x: f64| coefficients.resolve(x);
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(500).collect::<Vec<_>>();

        let model = estimate_model(&data).expect("Expected evaluation");

        // Currently, the logarithmic model is not supported
        assert!(model.is_other());
    }
}
