use serde::{Deserialize, Serialize};

/// Display unit for temperatures. All computation is in kelvin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    #[default]
    #[serde(rename = "C")]
    Celsius,
    #[serde(rename = "F")]
    Fahrenheit,
    #[serde(rename = "K")]
    Kelvin,
}

impl Unit {
    pub fn from_kelvin(self, k: f32) -> f32 {
        match self {
            Unit::Celsius => k - 273.15,
            Unit::Fahrenheit => (k - 273.15) * 1.8 + 32.0,
            Unit::Kelvin => k,
        }
    }

    /// Converts a temperature difference, which has no offset.
    pub fn delta_from_kelvin(self, dk: f32) -> f32 {
        match self {
            Unit::Fahrenheit => dk * 1.8,
            Unit::Celsius | Unit::Kelvin => dk,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Unit::Celsius => "°C",
            Unit::Fahrenheit => "°F",
            Unit::Kelvin => "K",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_fixed_points() {
        assert!((Unit::Celsius.from_kelvin(273.15)).abs() < 1e-4);
        assert!((Unit::Fahrenheit.from_kelvin(373.15) - 212.0).abs() < 1e-3);
        assert_eq!(Unit::Kelvin.from_kelvin(300.0), 300.0);
        assert!((Unit::Fahrenheit.delta_from_kelvin(10.0) - 18.0).abs() < 1e-4);
    }
}
