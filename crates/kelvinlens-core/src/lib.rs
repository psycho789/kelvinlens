//! Core of Kelvinlens: opening thermograms, recomputing temperatures with new capture
//! parameters, measurement statistics, palettes, exports and PDF reports.
//!
//! Decoding is done by [libblackbody](https://crates.io/crates/libblackbody). Everything here
//! works in kelvin; conversion to the user's unit happens only at the edges (CSV, report, UI).

mod error;
pub mod export;
pub mod folder;
pub mod measure;
pub mod palette;
pub mod params;
pub mod report;
pub mod sidecar;
pub mod thermogram;
pub mod unit;

pub use error::{Error, Result};
pub use measure::{Measurement, Shape, Stats};
pub use params::{Param, Params};
pub use thermogram::{Format, ThermalImage};
pub use unit::Unit;
