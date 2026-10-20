//! Formulas over measurement results, such as `Bx1.max - Sp1` or `avg(Sp1, Sp2, Sp3)`.
//!
//! Syntax: numbers, `+ - * / ^`, parentheses, and the functions `abs`, `min`, `max`, `avg`,
//! `sqrt` and `round(x, digits)`. A measurement is referenced by its label, optionally with a
//! statistic: `.max`, `.min`, `.avg`, `.sd` (standard deviation) or `.count` (pixels). A bare label
//! means the spot's value, or the average of a line or area. Labels with spaces or symbols are
//! written in brackets: `[Breaker 3].max`. An earlier formula's label can be used like a number.
//!
//! Temperatures enter in the display unit, so constants are typed in that unit too
//! (`Bx1.max - 40` means 40 °C when the unit is °C).

use serde::{Deserialize, Serialize};

use std::fmt;

use crate::Unit;
use crate::measure::{self, Measurement, Stats};

/// Why a formula has no value. The text is what the user sees in `FormulaResult::error`.
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaError(String);

impl fmt::Display for FormulaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormulaError {}

impl From<String> for FormulaError {
    fn from(m: String) -> Self {
        FormulaError(m)
    }
}

impl From<&str> for FormulaError {
    fn from(m: &str) -> Self {
        FormulaError(m.to_string())
    }
}

/// How a formula's result is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultKind {
    #[default]
    Temperature,
    Difference,
    Number,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Formula {
    pub label: String,
    pub expression: String,
    pub kind: ResultKind,
}

/// A formula's value in the display unit, or why it has none.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaResult {
    pub value: Option<f64>,
    pub error: Option<String>,
}

/// What a label refers to.
pub enum Value<'a> {
    /// A measurement's statistics (`None` if it covers no temperature).
    Measurement { stats: Option<&'a Stats> },
    /// An earlier formula's result.
    Number(f64),
}

/// Evaluates `formulas` for the measurements of an image with temperatures `kelvin`.
pub fn evaluate_for_image(
    kelvin: &[f32],
    width: usize,
    height: usize,
    measurements: &[Measurement],
    formulas: &[Formula],
    unit: Unit,
) -> Vec<FormulaResult> {
    let stats = measure::stats_each(kelvin, width, height, measurements.iter().map(|m| &m.shape));
    evaluate_measured(formulas, measurements, &stats, unit)
}

/// Evaluates `formulas` against `measurements`, whose statistics are `stats` in the same order. A
/// label matches a measurement exactly, or else ignoring case.
pub fn evaluate_measured(
    formulas: &[Formula],
    measurements: &[Measurement],
    stats: &[Option<Stats>],
    unit: Unit,
) -> Vec<FormulaResult> {
    evaluate_all(
        formulas,
        |name| {
            let i = measurements
                .iter()
                .position(|m| m.label == name)
                .or_else(|| measurements.iter().position(|m| m.label.eq_ignore_ascii_case(name)))?;
            Some(Value::Measurement { stats: stats.get(i)?.as_ref() })
        },
        unit,
    )
}

/// Evaluates `formulas` in order; each may use the labels of the measurements and of the
/// formulas before it.
fn evaluate_all<'a>(
    formulas: &[Formula],
    measurement: impl Fn(&str) -> Option<Value<'a>>,
    unit: Unit,
) -> Vec<FormulaResult> {
    let mut done: Vec<(String, f64)> = Vec::new();
    formulas
        .iter()
        .map(|f| {
            let lookup = |name: &str| {
                measurement(name).or_else(|| {
                    done.iter()
                        .rev()
                        .find(|(l, _)| l == name || l.eq_ignore_ascii_case(name))
                        .map(|(_, v)| Value::Number(*v))
                })
            };
            let r = evaluate(&f.expression, lookup, unit);
            if let Ok(v) = r
                && !f.label.trim().is_empty()
            {
                done.push((f.label.trim().to_string(), v));
            }
            match r {
                Ok(v) => FormulaResult { value: Some(v), error: None },
                Err(e) => FormulaResult { value: None, error: Some(e.to_string()) },
            }
        })
        .collect()
}

pub fn evaluate<'a>(
    expression: &str,
    lookup: impl Fn(&str) -> Option<Value<'a>>,
    unit: Unit,
) -> Result<f64, FormulaError> {
    let tokens = tokenize(expression)?;
    if tokens.is_empty() {
        return Err("Empty formula".into());
    }
    let mut p = Parser { tokens, pos: 0, lookup: &lookup, unit };
    let v = p.expr()?;
    if p.pos < p.tokens.len() {
        return Err(format!("Unexpected {}", p.tokens[p.pos].describe()).into());
    }
    if v.is_finite() { Ok(v) } else { Err("The result is not a number (division by zero?)".into()) }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Num(f64),
    Name(String),
    Op(char),
}

impl Token {
    fn describe(&self) -> String {
        match self {
            Token::Num(n) => format!("number {n}"),
            Token::Name(s) => format!("\"{s}\""),
            Token::Op(c) => format!("\"{c}\""),
        }
    }
}

fn tokenize(s: &str) -> Result<Vec<Token>, FormulaError> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // Exponent, as in 1e-3.
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '-' || chars[j] == '+') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Token::Num(text.parse().map_err(|_| format!("Bad number {text}"))?));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token::Name(chars[start..i].iter().collect()));
        } else if c == '[' {
            let end = chars[i + 1..].iter().position(|&c| c == ']').ok_or("Missing ] after a bracketed label")?;
            out.push(Token::Name(chars[i + 1..i + 1 + end].iter().collect::<String>().trim().to_string()));
            i += end + 2;
        } else if "+-*/^(),.".contains(c) {
            out.push(Token::Op(c));
            i += 1;
        } else {
            return Err(format!("Unexpected character \"{c}\"").into());
        }
    }
    Ok(out)
}

struct Parser<'p, 'a, F: Fn(&str) -> Option<Value<'a>>> {
    tokens: Vec<Token>,
    pos: usize,
    lookup: &'p F,
    unit: Unit,
}

impl<'a, F: Fn(&str) -> Option<Value<'a>>> Parser<'_, 'a, F> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn eat(&mut self, op: char) -> bool {
        if self.peek() == Some(&Token::Op(op)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: char) -> Result<(), FormulaError> {
        if self.eat(op) {
            Ok(())
        } else {
            Err(match self.peek() {
                Some(t) => format!("Expected \"{op}\" but found {}", t.describe()).into(),
                None => format!("Expected \"{op}\" at the end").into(),
            })
        }
    }

    fn expr(&mut self) -> Result<f64, FormulaError> {
        let mut v = self.term()?;
        loop {
            if self.eat('+') {
                v += self.term()?;
            } else if self.eat('-') {
                v -= self.term()?;
            } else {
                return Ok(v);
            }
        }
    }

    fn term(&mut self) -> Result<f64, FormulaError> {
        let mut v = self.power()?;
        loop {
            if self.eat('*') {
                v *= self.power()?;
            } else if self.eat('/') {
                v /= self.power()?;
            } else {
                return Ok(v);
            }
        }
    }

    fn power(&mut self) -> Result<f64, FormulaError> {
        let base = self.unary()?;
        if self.eat('^') {
            // Right-associative: 2^3^2 = 2^9.
            Ok(base.powf(self.power()?))
        } else {
            Ok(base)
        }
    }

    fn unary(&mut self) -> Result<f64, FormulaError> {
        if self.eat('-') {
            Ok(-self.unary()?)
        } else if self.eat('+') {
            self.unary()
        } else {
            self.primary()
        }
    }

    fn primary(&mut self) -> Result<f64, FormulaError> {
        match self.peek().cloned() {
            Some(Token::Num(n)) => {
                self.pos += 1;
                Ok(n)
            }
            Some(Token::Op('(')) => {
                self.pos += 1;
                let v = self.expr()?;
                self.expect(')')?;
                Ok(v)
            }
            Some(Token::Name(name)) => {
                self.pos += 1;
                if self.peek() == Some(&Token::Op('(')) && FUNCTIONS.contains(&name.to_ascii_lowercase().as_str()) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if !self.eat(')') {
                        loop {
                            args.push(self.expr()?);
                            if self.eat(')') {
                                break;
                            }
                            self.expect(',')?;
                        }
                    }
                    return call(&name.to_ascii_lowercase(), &args);
                }
                let stat = if self.eat('.') {
                    match self.peek().cloned() {
                        Some(Token::Name(s)) => {
                            self.pos += 1;
                            Some(s.to_ascii_lowercase())
                        }
                        _ => return Err(format!("Expected max, min, avg, sd or count after \"{name}.\"").into()),
                    }
                } else {
                    None
                };
                self.reference(&name, stat.as_deref())
            }
            Some(t) => Err(format!("Unexpected {}", t.describe()).into()),
            None => Err("The formula ends too early".into()),
        }
    }

    fn reference(&self, name: &str, stat: Option<&str>) -> Result<f64, FormulaError> {
        let value = (self.lookup)(name).ok_or_else(|| format!("No measurement or formula called \"{name}\""))?;
        match value {
            Value::Number(n) => match stat {
                None => Ok(n),
                Some(s) => Err(format!("\"{name}\" is a formula; \".{s}\" only applies to measurements").into()),
            },
            Value::Measurement { stats } => {
                let s = stats.ok_or_else(|| format!("{name} has no temperature"))?;
                let t = |k: f32| self.unit.from_kelvin(k) as f64;
                match stat {
                    None | Some("avg" | "mean" | "average") => Ok(t(s.avg)),
                    Some("max") => Ok(t(s.max)),
                    Some("min") => Ok(t(s.min)),
                    Some("sd" | "std" | "stddev") => Ok(self.unit.delta_from_kelvin(s.std_dev) as f64),
                    Some("count" | "area" | "pixels") => Ok(s.count as f64),
                    Some(other) => {
                        Err(format!("Unknown statistic \".{other}\"; use max, min, avg, sd or count").into())
                    }
                }
            }
        }
    }
}

const FUNCTIONS: [&str; 6] = ["abs", "min", "max", "avg", "sqrt", "round"];

fn call(name: &str, args: &[f64]) -> Result<f64, FormulaError> {
    let need = |n: usize| {
        if args.len() == n {
            Ok(())
        } else {
            Err(FormulaError::from(format!("{name}() takes {n} value{}", if n == 1 { "" } else { "s" })))
        }
    };
    match name {
        "abs" => need(1).map(|_| args[0].abs()),
        "sqrt" => need(1).map(|_| args[0].sqrt()),
        "round" => match args {
            [x] => Ok(x.round()),
            [x, d] => {
                let f = 10f64.powi(*d as i32);
                Ok((x * f).round() / f)
            }
            _ => Err("round() takes a value and optionally a number of digits".into()),
        },
        _ if args.is_empty() => Err(format!("{name}() needs at least one value").into()),
        "min" => Ok(args.iter().copied().fold(f64::INFINITY, f64::min)),
        "max" => Ok(args.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        "avg" => Ok(args.iter().sum::<f64>() / args.len() as f64),
        _ => unreachable!("listed in FUNCTIONS"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(min: f32, max: f32, avg: f32) -> Stats {
        Stats { min, max, avg, std_dev: 2.0, min_at: [0, 0], max_at: [0, 0], count: 9 }
    }

    fn eval(expr: &str, unit: Unit) -> Result<f64, String> {
        let sp1 = stats(300.15, 300.15, 300.15);
        let bx1 = stats(290.15, 320.15, 305.15);
        let br = stats(310.15, 330.15, 315.15);
        evaluate(
            expr,
            |name| match name {
                "Sp1" => Some(Value::Measurement { stats: Some(&sp1) }),
                "Bx1" => Some(Value::Measurement { stats: Some(&bx1) }),
                "Breaker 3" => Some(Value::Measurement { stats: Some(&br) }),
                "Li1" => Some(Value::Measurement { stats: None }),
                _ => None,
            },
            unit,
        )
        .map_err(|e| e.to_string())
    }

    #[test]
    fn formula_error_displays_its_message() {
        let e = FormulaError::from("Empty formula");
        assert_eq!(e.to_string(), "Empty formula");
        assert_eq!(evaluate("", |_| None, Unit::Celsius).unwrap_err(), e);
    }

    fn close(a: Result<f64, String>, b: f64) {
        let a = a.unwrap();
        assert!((a - b).abs() < 1e-4, "{a} != {b}");
    }

    #[test]
    fn arithmetic_and_precedence() {
        close(eval("1 + 2 * 3", Unit::Celsius), 7.0);
        close(eval("(1 + 2) * 3", Unit::Celsius), 9.0);
        close(eval("2 ^ 3 ^ 2", Unit::Celsius), 512.0);
        close(eval("-2 ^ 2", Unit::Celsius), 4.0);
        close(eval("10 / 4 - 1.5e1", Unit::Celsius), -12.5);
    }

    #[test]
    fn references_use_display_unit() {
        close(eval("Sp1", Unit::Celsius), 27.0);
        close(eval("Bx1.max - Sp1", Unit::Celsius), 20.0);
        close(eval("Bx1.max - Sp1", Unit::Fahrenheit), 36.0);
        close(eval("Bx1", Unit::Kelvin), 305.15);
        close(eval("Bx1.sd", Unit::Fahrenheit), 3.6);
        close(eval("Bx1.count", Unit::Celsius), 9.0);
        close(eval("[Breaker 3].max - Bx1.max", Unit::Celsius), 10.0);
    }

    #[test]
    fn functions() {
        close(eval("avg(Sp1, Bx1.max)", Unit::Celsius), 37.0);
        close(eval("max(1, 5, 3) + min(4, 2)", Unit::Celsius), 7.0);
        close(eval("abs(Sp1 - Bx1.max)", Unit::Celsius), 20.0);
        close(eval("round(10 / 3, 2)", Unit::Celsius), 3.33);
        close(eval("sqrt(16)", Unit::Celsius), 4.0);
    }

    #[test]
    fn errors_are_explained() {
        assert_eq!(eval("Sp9", Unit::Celsius).unwrap_err(), "No measurement or formula called \"Sp9\"");
        assert_eq!(eval("Li1.max", Unit::Celsius).unwrap_err(), "Li1 has no temperature");
        assert!(eval("Bx1.hot", Unit::Celsius).unwrap_err().contains("Unknown statistic"));
        assert!(eval("1 +", Unit::Celsius).unwrap_err().contains("ends too early"));
        assert!(eval("(1 + 2", Unit::Celsius).unwrap_err().contains("Expected \")\""));
        assert!(eval("1 / 0", Unit::Celsius).unwrap_err().contains("not a number"));
        assert!(eval("2 $ 3", Unit::Celsius).unwrap_err().contains("Unexpected character"));
        assert!(eval("abs(1, 2)", Unit::Celsius).unwrap_err().contains("takes 1 value"));
        assert!(eval("", Unit::Celsius).is_err());
        assert!(eval("1 2", Unit::Celsius).unwrap_err().contains("Unexpected number"));
    }

    #[test]
    fn evaluate_measured_matches_labels_exactly_then_ignoring_case() {
        use crate::measure::Shape;
        let measurements = [
            Measurement::new("Sp1", Shape::Spot { x: 0.5, y: 0.5 }),
            Measurement::new("BX1", Shape::Spot { x: 1.5, y: 0.5 }),
        ];
        let stats = [Some(stats(300.15, 300.15, 300.15)), Some(stats(310.15, 310.15, 310.15))];
        let formulas = vec![
            Formula { label: "A".into(), expression: "Sp1".into(), kind: ResultKind::Temperature },
            Formula { label: "B".into(), expression: "bx1 - Sp1".into(), kind: ResultKind::Difference },
            Formula { label: "C".into(), expression: "Nope".into(), kind: ResultKind::Number },
        ];
        let out = evaluate_measured(&formulas, &measurements, &stats, Unit::Celsius);
        assert!((out[0].value.unwrap() - 27.0).abs() < 1e-3);
        assert!((out[1].value.unwrap() - 10.0).abs() < 1e-3);
        assert!(out[2].value.is_none() && out[2].error.is_some());
    }

    #[test]
    fn later_formulas_use_earlier_ones() {
        let sp1 = stats(300.15, 300.15, 300.15);
        let formulas = vec![
            Formula { label: "Rise".into(), expression: "Sp1 - 20".into(), kind: ResultKind::Difference },
            Formula { label: "Double".into(), expression: "Rise * 2".into(), kind: ResultKind::Number },
            Formula { label: "Bad".into(), expression: "Nope".into(), kind: ResultKind::Number },
            Formula { label: "".into(), expression: "Double + 1".into(), kind: ResultKind::Number },
        ];
        let out = evaluate_all(
            &formulas,
            |n| (n == "Sp1").then_some(Value::Measurement { stats: Some(&sp1) }),
            Unit::Celsius,
        );
        assert!((out[0].value.unwrap() - 7.0).abs() < 1e-4);
        assert!((out[1].value.unwrap() - 14.0).abs() < 1e-4);
        assert!(out[2].error.is_some() && out[2].value.is_none());
        assert!((out[3].value.unwrap() - 15.0).abs() < 1e-4);
    }

    #[test]
    fn evaluate_for_image_matches_manual_stats() {
        let k = vec![300.0f32; 4];
        let ms = [Measurement::new("Sp1", measure::Shape::Spot { x: 0.5, y: 0.5 })];
        let formulas = [Formula { label: "A".into(), expression: "Sp1.avg".into(), kind: ResultKind::Number }];
        let stats = [measure::stats(&k, 2, 2, &ms[0].shape)];
        let expected = evaluate_measured(&formulas, &ms, &stats, Unit::Kelvin);
        let got = evaluate_for_image(&k, 2, 2, &ms, &formulas, Unit::Kelvin);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].value, expected[0].value);
        assert!(got[0].value.is_some());
    }
}
