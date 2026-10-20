//! Electrical: each measurement's maximum over the ambient (or the coldest measurement as the
//! similar-component reference), graded with NETA.
use super::{TradeAnalysis, TradeCtx, TradeRows};
use crate::analysis::{self, Basis};
use crate::report::prepare::signed;

pub(super) struct Electrical;

impl TradeAnalysis for Electrical {
    fn rows(&self, ctx: &TradeCtx<'_>) -> TradeRows {
        if !ctx.has_rows {
            return TradeRows::default();
        }
        let (c, unit) = (ctx.conditions, ctx.unit);
        let reference = match c.basis {
            Basis::Ambient => c.ambient_k.map(|k| (k, "ambient".to_string())),
            Basis::Similar => ctx
                .stats
                .iter()
                .zip(ctx.measurements)
                .filter_map(|(s, m)| s.map(|s| (s.max, m.label.clone())))
                .min_by(|a, b| a.0.total_cmp(&b.0)),
        };
        let electrical = match reference {
            Some((rk, rname)) => ctx
                .stats
                .iter()
                .zip(ctx.measurements)
                .filter_map(|(s, m)| {
                    let s = s.as_ref()?;
                    let d = s.max - rk;
                    let sev = analysis::neta_severity(d, c.basis);
                    if c.basis == Basis::Similar && m.label == rname {
                        return Some(vec![
                            m.label.clone(),
                            unit.format(s.max),
                            "Reference".into(),
                            "–".into(),
                            "Coolest similar component".into(),
                        ]);
                    }
                    let grade = if d < 1.0 {
                        ("–".to_string(), "No significant rise".to_string())
                    } else {
                        (sev.label, sev.action)
                    };
                    Some(vec![
                        m.label.clone(),
                        unit.format(s.max),
                        format!("{} ({rname})", signed(unit, d)),
                        grade.0,
                        grade.1,
                    ])
                })
                .collect(),
            None => Vec::new(),
        };
        TradeRows { electrical, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Unit;
    use crate::measure::{Measurement, Shape, Stats};
    use crate::report::Conditions;

    fn stats(max: f32) -> Option<Stats> {
        Some(Stats { min: max, max, avg: max, std_dev: 0.0, min_at: [0, 0], max_at: [0, 0], count: 1 })
    }

    fn two() -> Vec<Measurement> {
        vec![
            Measurement::new("Hot", Shape::Spot { x: 1.0, y: 1.0 }),
            Measurement::new("Cold", Shape::Spot { x: 2.0, y: 2.0 }),
        ]
    }

    #[test]
    fn similar_basis_marks_coolest_as_reference() {
        let (m, s) = (two(), [stats(310.0), stats(300.0)]);
        let ctx = TradeCtx {
            measurements: &m,
            stats: &s,
            conditions: Conditions { basis: Basis::Similar, ..Default::default() },
            unit: Unit::Celsius,
            has_rows: true,
        };
        let rows = Electrical.rows(&ctx).electrical;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1][2], "Reference");
        assert_eq!(rows[0][0], "Hot");
        assert!(rows[0][2].contains("(Cold)"));
        assert!(!rows[0][3].is_empty());
    }

    #[test]
    fn no_rows_means_no_grading() {
        let (m, s) = (two(), [stats(310.0), stats(300.0)]);
        let ctx = TradeCtx {
            measurements: &m,
            stats: &s,
            conditions: Conditions::default(),
            unit: Unit::Celsius,
            has_rows: false,
        };
        assert!(Electrical.rows(&ctx).electrical.is_empty());
    }
}
