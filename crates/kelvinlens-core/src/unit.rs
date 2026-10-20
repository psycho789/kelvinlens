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

    /// A temperature in kelvin as text with one decimal and the unit symbol, such as `20.0 °C`.
    pub fn format(self, kelvin: f32) -> String {
        self.format_value(f64::from(self.from_kelvin(kelvin)))
    }

    /// A value already in this unit, with one decimal and the unit symbol. Takes `f64` because
    /// formula results are `f64`; an `f32` widens exactly, so it prints the same digits.
    pub fn format_value(self, value: f64) -> String {
        format!("{value:.1} {}", self.symbol())
    }

    /// A difference already in this unit, with a leading `+` when it is zero or more, such as
    /// `+2.0 °C`.
    pub fn format_signed(self, value: f64) -> String {
        format!("{}{}", if value >= 0.0 { "+" } else { "" }, self.format_value(value))
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

    #[test]
    fn formats_values_with_one_decimal_and_symbol() {
        assert_eq!(Unit::Celsius.format(293.15), "20.0 °C");
        assert_eq!(Unit::Fahrenheit.format(373.15), "212.0 °F");
        assert_eq!(Unit::Kelvin.format(300.0), "300.0 K");
        assert_eq!(Unit::Celsius.format_value(-3.04), "-3.0 °C");
        // f64 0.15 is 0.1499999999999999944...; the f64 path must not round it up as f32 would.
        assert_eq!(Unit::Celsius.format_value(0.15), "0.1 °C");
        assert_eq!(Unit::Celsius.format_signed(2.04), "+2.0 °C");
        assert_eq!(Unit::Celsius.format_signed(0.0), "+0.0 °C");
        assert_eq!(Unit::Fahrenheit.format_signed(-1.5), "-1.5 °F");
        assert_eq!(Unit::Kelvin.format_signed(0.5), "+0.5 K");
    }
}
