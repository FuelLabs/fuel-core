use super::within_epsilon;
use crate::Fit;

/// Quadratic coefficients
#[derive(Debug, Clone, Copy)]
pub struct QuadraticCoefficients {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

fn is_zero(value: f64) -> bool {
    within_epsilon(value, 0.0, 0.01)
}

fn is_nonzero(value: f64) -> bool {
    !is_zero(value) && value.is_normal() && value.is_finite()
}

impl QuadraticCoefficients {
    pub fn new(a: f64, b: f64, c: f64) -> Self {
        Self { a, b, c }
    }

    pub fn resolve(&self, x: f64) -> f64 {
        let QuadraticCoefficients { a, b, c } = self;
        a * x.powi(2) + b * x + c
    }

    pub fn is_zero(&self) -> bool {
        is_zero(self.a) && is_zero(self.b) && is_zero(self.c)
    }

    pub fn is_constant(&self) -> bool {
        is_zero(self.a) && is_zero(self.b) && is_nonzero(self.c)
    }

    pub fn is_linear(&self) -> bool {
        is_zero(self.a) && is_nonzero(self.b)
    }

    pub fn is_quadratic(&self) -> bool {
        is_nonzero(self.a)
    }

    /// Sum of squares residual
    fn ssr(&self, points: &[(f64, f64)]) -> f64 {
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

    /// Sum of squares total
    fn sst(&self, points: &[(f64, f64)]) -> f64 {
        let real_values = points.iter().map(|(_, y)| *y);
        let avg_y = real_values.clone().sum::<f64>() / points.len() as f64;
        real_values
            .map(|real| real - avg_y)
            .map(|v| v.powi(2))
            .sum()
    }

    /// R^2 = 1 - (sst/sse)
    fn r_squared(&self, points: &[(f64, f64)]) -> f64 {
        let ssr = self.ssr(points);
        let sst = self.sst(points);
        if sst == 0.0 {
            1.0
        } else {
            1.0 - ssr / sst
        }
    }
}

impl Fit for QuadraticCoefficients {
    fn fit(&self, points: &[(f64, f64)]) -> f64 {
        self.r_squared(points)
    }
}
