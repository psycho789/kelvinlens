//! Per-trade extra tables of a report image, chosen by `TemplateKind` (electrical NETA grading,
//! building thermal index, HVAC supply/return split). `General` has none.
use crate::Unit;
use crate::measure::{Measurement, Stats};
use crate::report::{Conditions, TemplateKind};

mod building;
mod electrical;
mod hvac;

/// What a trade reads. Plain data, so trade code is testable without a `ThermalImage`.
pub(super) struct TradeCtx<'a> {
    pub measurements: &'a [Measurement],
    pub stats: &'a [Option<Stats>],
    pub conditions: Conditions,
    pub unit: Unit,
    /// `prepare_image` built at least one `MeasurementRow` (electrical only grades then).
    pub has_rows: bool,
}

/// The rows each trade fills; the others stay empty. Same shapes as `PreparedImage`.
#[derive(Default)]
pub(super) struct TradeRows {
    pub electrical: Vec<Vec<String>>,
    pub building: Vec<Vec<String>>,
    pub hvac: Vec<[String; 2]>,
}

pub(super) trait TradeAnalysis {
    fn rows(&self, ctx: &TradeCtx<'_>) -> TradeRows;
}

pub(super) fn for_kind(kind: TemplateKind) -> Option<&'static dyn TradeAnalysis> {
    match kind {
        TemplateKind::General => None,
        TemplateKind::Electrical => Some(&electrical::Electrical),
        TemplateKind::Building => Some(&building::Building),
        TemplateKind::Hvac => Some(&hvac::Hvac),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_has_no_trade() {
        assert!(for_kind(TemplateKind::General).is_none());
        for k in [TemplateKind::Electrical, TemplateKind::Building, TemplateKind::Hvac] {
            assert!(for_kind(k).is_some());
        }
    }
}
