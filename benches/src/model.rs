mod constant;
mod linear;
mod logarithmic;
mod quadratic;

use rand::Rng;

use crate::regression::quadratic_regression;

pub use constant::ConstantCoefficients;
pub use linear::LinearCoefficients;
pub use logarithmic::LogarithmicCoefficients;
pub use quadratic::QuadraticCoefficients;

pub trait Fit {
    fn fit(&self, points: &Vec<(f64, f64)>) -> f64;
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
    fn is_zero(&self) -> bool {
        match self {
            Model::Zero => true,
            _ => false,
        }
    }

    fn is_constant(&self) -> bool {
        match self {
            Model::Constant(_) => true,
            _ => false,
        }
    }

    fn is_linear(&self) -> bool {
        match self {
            Model::Linear(_) => true,
            _ => false,
        }
    }

    fn is_quadratic(&self) -> bool {
        match self {
            Model::Quadratic(_) => true,
            _ => false,
        }
    }

    fn is_other(&self) -> bool {
        match self {
            Model::Other => true,
            _ => false,
        }
    }
}

/// Generate a model that estimates the points in the dataset
pub fn evaluate_model(points: &Vec<(f64, f64)>) -> anyhow::Result<Model> {
    // let linear_coefficients = linear_regression(points);
    let quadratic_coefficients = quadratic_regression(points)?;

    // let linear_fit = linear_coefficients.r_squared()

    if quadratic_coefficients.is_zero() {
        // Zero
        Ok(Model::Zero)
    } else if quadratic_coefficients.is_constant() {
        // Constant
        let QuadraticCoefficients { c: y, .. } = quadratic_coefficients;
        let constant_coefficients = ConstantCoefficients { y };
        Ok(Model::Constant(constant_coefficients))
    } else if quadratic_coefficients.is_linear() {
        // Linear
        let QuadraticCoefficients {
            b: slope,
            c: intercept,
            ..
        } = quadratic_coefficients;
        let linear_coefficients = LinearCoefficients { slope, intercept };
        Ok(Model::Linear(linear_coefficients))
    } else if quadratic_coefficients.is_quadratic() {
        // Quadratic
        Ok(Model::Quadratic(quadratic_coefficients))
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
        let data = points.take(500).collect();

        let model = evaluate_model(&data).expect("Expected evaluation");

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
        let data = points.take(500).collect();

        let model = evaluate_model(&data).expect("Expected evaluation");

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
        let data = points.take(500).collect();

        let model = evaluate_model(&data).expect("Expected evaluation");

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
        let data = points.take(500).collect();

        let model = evaluate_model(&data).expect("Expected evaluation");

        assert!(model.is_quadratic());
    }

    #[test_case(LogarithmicCoefficients::new( 02.00,  10.00); "y = log2(x) + 10")]
    #[test_case(LogarithmicCoefficients::new( 03.00, -01.00); "y = log3(x) - 1")]
    #[test_case(LogarithmicCoefficients::new( 2.718,  04.50); "y = ln(x) + 4.5")]
    #[test_case(LogarithmicCoefficients::new( 01.00,  50.00); "y = log0(x) + 50")]
    fn evaluate_logarithmic_function(coefficients: LogarithmicCoefficients) {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let function = |x: f64| coefficients.resolve(x);
        let noise =
            |(x, y): (_, f64)| -> (f64, f64) { (x, apply_noise(y, 0.5, &mut rng)) };
        let independent = core::iter::successors(Some(0.0), |n| Some(n + 1.0));
        let points = independent.map(|x| (x, function(x))).map(noise);
        let data = points.take(50).collect();

        let model = evaluate_model(&data).expect("Expected evaluation");

        // Currently, the logarithmic model is not supported
        assert!(model.is_other());
    }
}
