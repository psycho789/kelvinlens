//! Building: thermal index and surface humidity at each measurement's coldest point.
use super::{TradeAnalysis, TradeCtx, TradeRows};
use crate::analysis;

pub(super) struct Building;

impl TradeAnalysis for Building {
    fn rows(&self, ctx: &TradeCtx<'_>) -> TradeRows {
        let (c, unit) = (ctx.conditions, ctx.unit);
        let building = ctx
            .stats
            .iter()
            .zip(ctx.measurements)
            .filter_map(|(s, m)| {
                let s = s.as_ref()?;
                let ti = match (c.inside_k, c.outside_k) {
                    (Some(i), Some(o)) if (i - o).abs() > 0.5 => {
                        format!("{:.0} %", analysis::thermal_index(s.min, i, o) * 100.0)
                    }
                    _ => "–".into(),
                };
                let (rh, risk) = match (c.inside_k, c.humidity) {
                    (Some(i), Some(rh)) => {
                        let srh = analysis::surface_humidity(i, rh, s.min);
                        let risk = if srh >= 1.0 {
                            "Condensation"
                        } else if srh >= 0.8 {
                            "Mould risk"
                        } else {
                            "OK"
                        };
                        (format!("{:.0} %", (srh * 100.0).min(999.0)), risk.to_string())
                    }
                    _ => ("–".into(), "–".into()),
                };
                Some(vec![m.label.clone(), unit.format(s.min), ti, rh, risk])
            })
            .collect();
        TradeRows { building, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Unit;
    use crate::measure::{Measurement, Shape, Stats};
    use crate::report::Conditions;

    #[test]
    fn missing_conditions_show_dashes() {
        let m = vec![Measurement::new("Wall", Shape::Spot { x: 1.0, y: 1.0 })];
        let s = [Some(Stats {
            min: 280.0,
            max: 290.0,
            avg: 285.0,
            std_dev: 0.0,
            min_at: [0, 0],
            max_at: [0, 0],
            count: 1,
        })];
        let ctx = TradeCtx {
            measurements: &m,
            stats: &s,
            conditions: Conditions::default(),
            unit: Unit::Celsius,
            has_rows: true,
        };
        let rows = TradeAnalysis::rows(&Building, &ctx).building;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][2..], ["–", "–", "–"]);
    }
}
