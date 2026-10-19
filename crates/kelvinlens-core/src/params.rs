//! Capture parameters: the object and environment settings temperatures are computed with.

use libblackbody::CaptureParameters;
use serde::{Deserialize, Serialize};
use uom::si::length::meter;
use uom::si::thermodynamic_temperature::kelvin;

use crate::{Error, Result};

/// Capture parameters in SI units: temperatures in kelvin, distance in metres and relative
/// humidity as a fraction (0.5 = 50 %). `None` where the file stores no value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Params {
    pub emissivity: Option<f32>,
    pub reflected_k: Option<f32>,
    pub atmosphere_k: Option<f32>,
    pub distance_m: Option<f32>,
    pub humidity: Option<f32>,
    pub window_k: Option<f32>,
    pub window_transmission: Option<f32>,
}

/// One editable parameter, as named in the UI and in `ThermalImage::editable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Param {
    Emissivity,
    ReflectedK,
    AtmosphereK,
    DistanceM,
    Humidity,
    WindowK,
    WindowTransmission,
}

impl Param {
    pub const ALL: [Param; 7] = [
        Param::Emissivity,
        Param::ReflectedK,
        Param::AtmosphereK,
        Param::DistanceM,
        Param::Humidity,
        Param::WindowK,
        Param::WindowTransmission,
    ];
}

impl Params {
    pub fn get(&self, p: Param) -> Option<f32> {
        match p {
            Param::Emissivity => self.emissivity,
            Param::ReflectedK => self.reflected_k,
            Param::AtmosphereK => self.atmosphere_k,
            Param::DistanceM => self.distance_m,
            Param::Humidity => self.humidity,
            Param::WindowK => self.window_k,
            Param::WindowTransmission => self.window_transmission,
        }
    }

    pub fn set(&mut self, p: Param, value: Option<f32>) {
        match p {
            Param::Emissivity => self.emissivity = value,
            Param::ReflectedK => self.reflected_k = value,
            Param::AtmosphereK => self.atmosphere_k = value,
            Param::DistanceM => self.distance_m = value,
            Param::Humidity => self.humidity = value,
            Param::WindowK => self.window_k = value,
            Param::WindowTransmission => self.window_transmission = value,
        }
    }

    /// `base` with the `editable` fields taken from `self` where `self` has a value.
    pub fn overlay(&self, base: &Params, editable: &[Param]) -> Params {
        let mut out = *base;
        for &p in editable {
            if let Some(v) = self.get(p) {
                out.set(p, Some(v));
            }
        }
        out
    }

    /// Rejects values that are physically meaningless or outside the model's range.
    pub fn validate(&self) -> Result<()> {
        let check = |value: Option<f32>, ok: fn(f32) -> bool, what: &str| match value {
            Some(v) if !v.is_finite() || !ok(v) => Err(Error::Params(format!("{what} is out of range: {v}"))),
            _ => Ok(()),
        };
        check(self.emissivity, |v| v > 0.0 && v <= 1.0, "Emissivity (0.01 to 1.00)")?;
        check(self.window_transmission, |v| v > 0.0 && v <= 1.0, "Window transmission (0.01 to 1.00)")?;
        check(self.humidity, |v| (0.0..=1.0).contains(&v), "Relative humidity (0 to 100 %)")?;
        check(self.distance_m, |v| (0.0..=100_000.0).contains(&v), "Distance")?;
        let temperature = |v: f32| (1.0..=2500.0).contains(&v);
        check(self.reflected_k, temperature, "Reflected temperature")?;
        check(self.atmosphere_k, temperature, "Atmospheric temperature")?;
        check(self.window_k, temperature, "Window temperature")?;
        Ok(())
    }
}

impl From<CaptureParameters> for Params {
    fn from(c: CaptureParameters) -> Self {
        Params {
            emissivity: c.emissivity,
            reflected_k: c.reflected_temperature.map(|t| t.get::<kelvin>()),
            atmosphere_k: c.atmospheric_temperature.map(|t| t.get::<kelvin>()),
            distance_m: c.distance.map(|d| d.get::<meter>()),
            humidity: c.relative_humidity,
            window_k: c.ir_window_temperature.map(|t| t.get::<kelvin>()),
            window_transmission: c.transmissivity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_only_touches_editable_fields_with_values() {
        let base = Params { emissivity: Some(0.95), distance_m: Some(1.0), ..Default::default() };
        let edit = Params { emissivity: Some(0.8), distance_m: Some(5.0), ..Default::default() };
        let out = edit.overlay(&base, &[Param::Emissivity, Param::Humidity]);
        assert_eq!(out.emissivity, Some(0.8));
        assert_eq!(out.distance_m, Some(1.0));
        assert_eq!(out.humidity, None);
    }

    #[test]
    fn validate_rejects_out_of_range() {
        assert!(Params { emissivity: Some(0.0), ..Default::default() }.validate().is_err());
        assert!(Params { emissivity: Some(1.2), ..Default::default() }.validate().is_err());
        assert!(Params { humidity: Some(1.5), ..Default::default() }.validate().is_err());
        assert!(Params { distance_m: Some(-1.0), ..Default::default() }.validate().is_err());
        assert!(Params { reflected_k: Some(f32::NAN), ..Default::default() }.validate().is_err());
        assert!(Params { emissivity: Some(0.95), humidity: Some(0.5), ..Default::default() }.validate().is_ok());
    }

    #[test]
    fn serializes_camel_case() {
        let json = serde_json::to_string(&Params { reflected_k: Some(293.15), ..Default::default() }).unwrap();
        assert!(json.contains("\"reflectedK\":293.15"), "{json}");
        assert_eq!(serde_json::to_string(&Param::WindowTransmission).unwrap(), "\"windowTransmission\"");
    }
}
