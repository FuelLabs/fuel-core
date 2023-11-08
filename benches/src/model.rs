mod constant;
mod linear;
mod quadratic;

use rand::Rng;

pub use constant::ConstantCoefficients;
pub use linear::{
    linear_regression,
    LinearCoefficients,
};
pub use quadratic::{
    quadratic_regression,
    QuadraticCoefficients,
};

pub fn within_epsilon(value: f64, expected: f64, epsilon: f64) -> bool {
    let range = (expected - epsilon)..=(expected + epsilon);
    range.contains(&value)
}

fn apply_noise<R: Rng>(value: f64, bound: f64, rng: &mut R) -> f64 {
    let delta = rng.gen_range(-bound..bound);
    value + delta
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
pub enum Model {
    Zero,
    Constant(ConstantCoefficients),
    Linear(LinearCoefficients),
    Quadratic(QuadraticCoefficients),
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
}

pub fn evaluate_model(points: &Vec<(f64, f64)>) -> anyhow::Result<Model> {
    let coefficients = quadratic_regression(points)?;
    if coefficients.is_zero() {
        Ok(Model::Zero)
    } else if coefficients.is_constant() {
        let QuadraticCoefficients { c: y, .. } = coefficients;
        let constant_coefficients = ConstantCoefficients { y };
        Ok(Model::Constant(constant_coefficients))
    } else if coefficients.is_linear() {
        let QuadraticCoefficients {
            b: slope,
            c: intercept,
            ..
        } = coefficients;
        let linear_coefficients = LinearCoefficients { slope, intercept };
        Ok(Model::Linear(linear_coefficients))
    } else if coefficients.is_quadratic() {
        Ok(Model::Quadratic(coefficients))
    } else {
        Err(anyhow::anyhow!(
            "Failed to evaluate an appropriate model for the dataset"
        ))
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

    #[test_case(ConstantCoefficients::new(12.0); "y = 12.0")]
    #[test_case(ConstantCoefficients::new(-1.0); "y = -1.0")]
    #[test_case(ConstantCoefficients::new(200.0); "y = 200.0")]
    #[test_case(ConstantCoefficients::new(0.05); "y = 0.05")]
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

    #[test_case(LinearCoefficients::new(5.0, 12.0); "y = 5x + 12.0")]
    #[test_case(LinearCoefficients::new(-1.0, -1.0); "y = -x - 1.0")]
    #[test_case(LinearCoefficients::new(0.5, 200.0); "y = 0.5x + 200.0")]
    #[test_case(LinearCoefficients::new(200.0, 0.05); "y = 200x + 0.05")]
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

    #[test_case(QuadraticCoefficients::new(2.0, 5.0, 12.0); "y = 2x^2 + 5x + 12.0")]
    #[test_case(QuadraticCoefficients::new(-1.0, -1.0, -1.0); "y = -x^2 - x - 1.0")]
    #[test_case(QuadraticCoefficients::new(3.5, 0.5, 200.0); "y = 3.5x^2 + 0.5x + 200.0")]
    #[test_case(QuadraticCoefficients::new(-5.0, 200.0, 0.05); "y = -5x^2 + 200x + 0.05")]
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
}
