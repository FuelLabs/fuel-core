use super::within_epsilon;
use crate::{
    Fit,
    Resolve,
};

/// Linear Coefficients
#[derive(Debug, Clone, Copy)]
pub struct LinearCoefficients {
    pub slope: f64,
    pub intercept: f64,
}

fn is_zero(value: f64) -> bool {
    within_epsilon(value, 0.0, 0.0001)
}

fn is_nonzero(value: f64) -> bool {
    !is_zero(value) && value.is_normal() && value.is_finite()
}

impl LinearCoefficients {
    pub fn new(slope: f64, intercept: f64) -> Self {
        Self { slope, intercept }
    }

    pub fn is_zero(&self) -> bool {
        is_zero(self.slope) && is_zero(self.intercept)
    }

    pub fn is_constant(&self) -> bool {
        is_zero(self.slope) && is_nonzero(self.intercept)
    }

    pub fn is_linear(&self) -> bool {
        is_nonzero(self.slope)
    }

    /// Sum of squares residual
    fn ssr(&self, points: &[(f64, f64)]) -> f64 {
        let LinearCoefficients { slope, intercept } = self;
        let real_values = points.iter().map(|(_, y)| *y);
        let predicted = points.iter().map(|(x, _)| *slope * *x + *intercept);
        real_values
            .zip(predicted)
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

impl Fit for LinearCoefficients {
    fn fit(&self, points: &[(f64, f64)]) -> f64 {
        self.r_squared(points)
    }
}

impl Resolve for LinearCoefficients {
    fn resolve(&self, x: f64) -> f64 {
        self.slope * x + self.intercept
    }
}
