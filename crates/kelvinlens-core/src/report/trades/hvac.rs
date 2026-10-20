//! HVAC: the split between measurements labelled Supply… and Return….
use super::{TradeAnalysis, TradeCtx, TradeRows};
use crate::report::prepare::signed;

pub(super) struct Hvac;

impl TradeAnalysis for Hvac {
    fn rows(&self, ctx: &TradeCtx<'_>) -> TradeRows {
        let unit = ctx.unit;
        let find = |p: &str| {
            ctx.measurements
                .iter()
                .zip(ctx.stats)
                .find(|(m, s)| m.label.to_lowercase().starts_with(p) && s.is_some())
                .map(|(m, s)| (m.label.clone(), s.unwrap().avg))
        };
        let mut rows = Vec::new();
        if let Some((l, k)) = find("supply") {
            rows.push([format!("Supply ({l}, average)"), unit.format(k)]);
        }
        if let Some((l, k)) = find("return") {
            rows.push([format!("Return ({l}, average)"), unit.format(k)]);
        }
        if let (Some((_, s)), Some((_, r))) = (find("supply"), find("return")) {
            rows.push(["Split (return − supply)".into(), signed(unit, r - s)]);
        }
        if rows.is_empty() {
            rows.push(["Split".into(), "Label measurements \"Supply\" and \"Return\" to show the split".into()]);
        }
        TradeRows { hvac: rows, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Unit;
    use crate::measure::{Measurement, Shape, Stats};
    use crate::report::Conditions;

    fn avg(k: f32) -> Option<Stats> {
        Some(Stats { min: k, max: k, avg: k, std_dev: 0.0, min_at: [0, 0], max_at: [0, 0], count: 1 })
    }

    fn run(labels: &[&str], s: &[Option<Stats>]) -> Vec<[String; 2]> {
        let m: Vec<_> = labels.iter().map(|l| Measurement::new(*l, Shape::Spot { x: 1.0, y: 1.0 })).collect();
        let ctx = TradeCtx {
            measurements: &m,
            stats: s,
            conditions: Conditions::default(),
            unit: Unit::Celsius,
            has_rows: true,
        };
        Hvac.rows(&ctx).hvac
    }

    #[test]
    fn no_supply_or_return_explains_labels() {
        let rows = run(&["Box 1"], &[avg(290.0)]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "Split");
        assert_eq!(rows[0][1], "Label measurements \"Supply\" and \"Return\" to show the split");
    }

    #[test]
    fn split_is_return_minus_supply() {
        let rows = run(&["Supply A", "Return A"], &[avg(290.0), avg(295.0)]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2][0], "Split (return − supply)");
    }
}
