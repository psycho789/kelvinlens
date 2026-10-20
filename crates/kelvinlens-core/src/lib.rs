//! Core of Kelvinlens: opening thermograms and radiometric video, recomputing temperatures with
//! new capture parameters, measurements and formulas, isotherms and automatic inspection,
//! rendering, panoramas, exports and reports.
//!
//! Decoding is done by [libblackbody](https://crates.io/crates/libblackbody). Everything here
//! works in kelvin; conversion to the user's unit happens only at the edges (CSV, report, UI).

pub mod analysis;
pub mod avi;
mod error;
pub mod export;
pub mod fff;
pub mod folder;
pub mod formula;
pub mod geo;
pub mod measure;
pub mod palette;
pub mod panorama;
pub mod params;
pub mod render;
pub mod report;
mod report_html;
pub mod sidecar;
pub mod thermogram;
pub mod unit;
pub mod video;

pub use error::{Error, Result};
pub use measure::{Measurement, Shape, Stats};
pub use params::{Param, Params};
pub use thermogram::{Format, ThermalImage};
pub use unit::Unit;
