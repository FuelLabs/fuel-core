use crate::Fit;

/// Constant Coefficients
#[derive(Debug, Clone, Copy)]
pub struct ConstantCoefficients {
    pub y: f64,
}

impl ConstantCoefficients {
    pub fn new(y: f64) -> Self {
        Self { y }
    }

    pub fn resolve(&self, _x: f64) -> f64 {
        self.y
    }
}

impl ConstantCoefficients {
    /// Sum of square residuals
    fn ssr(&self, points: &[(f64, f64)]) -> f64 {
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
    fn sst(&self, points: &[(f64, f64)]) -> f64 {
        let real_values = points.iter().map(|(_, y)| *y);
        let avg_y = real_values.clone().sum::<f64>() / points.len() as f64;
        real_values
            .map(|real| real - avg_y)
            .map(|v| v.powi(2))
            .sum()
    }

    /// R^2 = 1 - (ssr/sst)
    pub fn r_squared(&self, points: &[(f64, f64)]) -> f64 {
        let ssr = self.ssr(points);
        let sst = self.sst(points);
        1.0 - ssr / sst
    }
}

impl Fit for ConstantCoefficients {
    fn fit(&self, points: &[(f64, f64)]) -> f64 {
        self.r_squared(points)
    }
}
