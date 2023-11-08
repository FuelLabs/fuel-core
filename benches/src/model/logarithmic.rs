#[derive(Debug, Clone, Copy)]
pub struct LogarithmicCoefficients {
    pub base: f64,
    pub c: f64,
}

impl LogarithmicCoefficients {
    pub fn new(base: f64, c: f64) -> Self {
        Self { base, c }
    }

    pub fn resolve(&self, x: f64) -> f64 {
        x.log(self.base) + self.c
    }
}
