//! Global variables and equations (`wall = 2`, `"D2@Sketch1" = "D1@Sketch1" * 2`): an expression
//! parser with units, a solver that evaluates them in dependency order, and the part dimensions
//! they drive.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::units::LENGTHS;
use crate::{FeatureKind, History, Units};

/// One row of the Equations dialog: what it sets (a global variable, or a dimension named like
/// `D1@Sketch1`) and the expression it is set to.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Equation {
    pub name: String,
    pub expr: String,
}

impl Equation {
    pub fn new(name: &str, expr: &str) -> Self {
        Self {
            name: name.into(),
            expr: expr.into(),
        }
    }

    /// The name without surrounding spaces or quotes.
    pub fn target(&self) -> &str {
        self.name.trim().trim_matches('"').trim()
    }
}

/// What a dimension measures, which says how its value is shown and typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantity {
    /// Stored in mm, shown in document units.
    Length,
    /// Stored in radians, shown in degrees.
    Angle,
    /// Turns, scale factors.
    Number,
}

impl Quantity {
    /// A stored value (mm, radians) as shown (document units, degrees).
    pub fn shown(self, v: f64, units: Units) -> f64 {
        match self {
            Quantity::Length => v / units.system.mm_per_unit(),
            Quantity::Angle => v.to_degrees(),
            Quantity::Number => v,
        }
    }

    pub fn stored(self, v: f64, units: Units) -> f64 {
        match self {
            Quantity::Length => v * units.system.mm_per_unit(),
            Quantity::Angle => v.to_radians(),
            Quantity::Number => v,
        }
    }

    pub fn unit(self, units: Units) -> &'static str {
        match self {
            Quantity::Length => units.system.length_unit(),
            Quantity::Angle => "°",
            Quantity::Number => "",
        }
    }
}

/// A dimension equations can drive.
#[derive(Clone, Debug, PartialEq)]
pub struct Dimension {
    /// `D3@Sketch1`.
    pub name: String,
    /// What it is: "Distance", "Depth".
    pub label: String,
    pub quantity: Quantity,
    /// Stored value (mm, radians).
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Num(f64),
    Name(String),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

struct Parser {
    s: Vec<char>,
    at: usize,
    units: Units,
}

impl Parser {
    fn skip(&mut self) {
        while self.s.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    /// The next character after spaces.
    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.s.get(self.at).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(c);
        self.at += found as usize;
        found
    }

    fn sum(&mut self) -> Result<Expr, String> {
        let mut e = self.product()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.at += 1;
            e = Expr::Bin(op, Box::new(e), Box::new(self.product()?));
        }
        Ok(e)
    }

    fn product(&mut self) -> Result<Expr, String> {
        let mut e = self.unary()?;
        while let Some(op @ ('*' | '/')) = self.peek() {
            self.at += 1;
            e = Expr::Bin(op, Box::new(e), Box::new(self.unary()?));
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat('-') {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat('+') {
            return self.unary();
        }
        let base = self.atom()?;
        if self.eat('^') {
            return Ok(Expr::Bin('^', Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.peek() {
            None => Err("the expression ends too early".into()),
            Some('(') => {
                self.at += 1;
                let e = self.sum()?;
                if !self.eat(')') {
                    return Err("a ')' is missing".into());
                }
                Ok(self.with_unit(e))
            }
            Some('"') => {
                self.at += 1;
                let start = self.at;
                while self.s.get(self.at).is_some_and(|&c| c != '"') {
                    self.at += 1;
                }
                if self.at == self.s.len() {
                    return Err("a closing '\"' is missing".into());
                }
                let name: String = self.s[start..self.at].iter().collect();
                self.at += 1;
                Ok(Expr::Name(name.trim().to_string()))
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.at;
                while self.s.get(self.at).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
                    self.at += 1;
                }
                // An exponent only when digits follow, so `2e` is not half a number.
                if matches!(self.s.get(self.at), Some('e' | 'E')) {
                    let sign = matches!(self.s.get(self.at + 1), Some('+' | '-')) as usize;
                    if self.s.get(self.at + 1 + sign).is_some_and(char::is_ascii_digit) {
                        self.at += 1 + sign;
                        while self.s.get(self.at).is_some_and(char::is_ascii_digit) {
                            self.at += 1;
                        }
                    }
                }
                let text: String = self.s[start..self.at].iter().collect();
                let v: f64 = text.parse().map_err(|_| format!("'{text}' is not a number"))?;
                Ok(self.with_unit(Expr::Num(v)))
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                let start = self.at;
                while self.s.get(self.at).is_some_and(|&c| c.is_alphanumeric() || c == '_') {
                    self.at += 1;
                }
                // A dimension: `D1@Boss-Extrude1` (feature names may hold dashes).
                if self.s.get(self.at) == Some(&'@') {
                    self.at += 1;
                    while self
                        .s
                        .get(self.at)
                        .is_some_and(|&c| c.is_alphanumeric() || c == '_' || c == '-')
                    {
                        self.at += 1;
                    }
                }
                let name: String = self.s[start..self.at].iter().collect();
                if self.eat('(') {
                    let mut args = vec![self.sum()?];
                    while self.eat(',') {
                        args.push(self.sum()?);
                    }
                    if !self.eat(')') {
                        return Err(format!("a ')' is missing after {name}("));
                    }
                    return Ok(Expr::Call(name.to_lowercase(), args));
                }
                Ok(if name.eq_ignore_ascii_case("pi") {
                    Expr::Num(std::f64::consts::PI)
                } else {
                    Expr::Name(name)
                })
            }
            Some(c) => Err(format!("unexpected '{c}'")),
        }
    }

    /// `e` followed by an optional unit (`5 mm`, `2"`, `30 deg`), converted to document units
    /// (lengths) or degrees (angles).
    fn with_unit(&mut self, e: Expr) -> Expr {
        let scale = |v: f64| Expr::Bin('*', Box::new(e.clone()), Box::new(Expr::Num(v)));
        let per = self.units.system.mm_per_unit();
        // Inch and foot marks only right after the number.
        match self.s.get(self.at) {
            Some('"') => {
                self.at += 1;
                return scale(25.4 / per);
            }
            Some('\'') => {
                self.at += 1;
                return scale(304.8 / per);
            }
            Some('°') => {
                self.at += 1;
                return e;
            }
            _ => {}
        }
        let back = self.at;
        self.skip();
        let start = self.at;
        while self.s.get(self.at).is_some_and(|c| c.is_alphabetic()) {
            self.at += 1;
        }
        let word: String = self.s[start..self.at].iter().collect::<String>().to_lowercase();
        // A word followed by `(` or `@` is a function or a dimension, not a unit.
        let named = matches!(self.s.get(self.at), Some('(' | '@' | '_'))
            || self.s.get(self.at).is_some_and(char::is_ascii_digit);
        let factor = match word.as_str() {
            _ if named => None,
            "deg" => Some(1.0),
            "rad" => Some(1f64.to_degrees()),
            w => LENGTHS.iter().find(|(u, _)| *u == w).map(|(_, mm)| mm / per),
        };
        match factor {
            Some(1.0) => e,
            Some(f) => scale(f),
            None => {
                self.at = back;
                e
            }
        }
    }
}

fn parse(text: &str, units: Units) -> Result<Expr, String> {
    let mut p = Parser {
        s: text.chars().collect(),
        at: 0,
        units,
    };
    let e = p.sum()?;
    match p.peek() {
        None => Ok(e),
        Some(c) => Err(format!("unexpected '{c}'")),
    }
}

impl Expr {
    fn eval(&self, get: &mut dyn FnMut(&str) -> Result<f64, String>) -> Result<f64, String> {
        Ok(match self {
            Expr::Num(v) => *v,
            Expr::Name(n) => get(n)?,
            Expr::Neg(e) => -e.eval(get)?,
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(get)?, b.eval(get)?);
                match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' if b == 0.0 => return Err("division by zero".into()),
                    '/' => a / b,
                    _ => a.powf(b),
                }
            }
            Expr::Call(f, args) => {
                let v = args.iter().map(|a| a.eval(get)).collect::<Result<Vec<_>, _>>()?;
                let one = |g: fn(f64) -> f64| match v[..] {
                    [x] => Ok(g(x)),
                    _ => Err(format!("{f}() takes one value")),
                };
                match f.as_str() {
                    "sin" => one(|x| x.to_radians().sin())?,
                    "cos" => one(|x| x.to_radians().cos())?,
                    "tan" => one(|x| x.to_radians().tan())?,
                    "asin" => one(|x| x.asin().to_degrees())?,
                    "acos" => one(|x| x.acos().to_degrees())?,
                    "atan" => one(|x| x.atan().to_degrees())?,
                    "sqrt" => one(f64::sqrt)?,
                    "abs" => one(f64::abs)?,
                    "round" => one(f64::round)?,
                    "floor" | "int" => one(f64::floor)?,
                    "ceil" => one(f64::ceil)?,
                    "min" => v.iter().copied().fold(f64::INFINITY, f64::min),
                    "max" => v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    _ => return Err(format!("unknown function {f}()")),
                }
            }
        })
    }
}

/// Evaluates `text` (lengths in document units, angles in degrees) with no names in it: a typed
/// value like `10 + 2in`.
pub fn evaluate(text: &str, units: Units) -> Result<f64, String> {
    let v = parse(text, units)?.eval(&mut |n| Err(format!("unknown name {n}")))?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err("the result is not a number".into())
    }
}

/// Evaluates equations in dependency order, whatever order they are listed in. A name no
/// equation sets is read with `known` (a dimension's current shown value). Each result is the
/// value (document units, degrees), or why it could not be worked out.
pub fn solve(eqs: &[Equation], units: Units, known: &dyn Fn(&str) -> Option<f64>) -> Vec<Result<f64, String>> {
    let mut out: Vec<Option<Result<f64, String>>> = vec![None; eqs.len()];
    let mut by_name = HashMap::new();
    for (i, e) in eqs.iter().enumerate() {
        let name = e.target();
        if name.is_empty() {
            out[i] = Some(Err("give it a name".into()));
        } else if by_name.contains_key(name) {
            out[i] = Some(Err(format!("{name} is already set by another equation")));
        } else {
            by_name.insert(name, i);
        }
    }
    let mut s = Solver {
        eqs,
        parsed: eqs.iter().map(|e| parse(&e.expr, units)).collect(),
        by_name,
        known,
        out,
        stack: Vec::new(),
    };
    (0..eqs.len()).map(|i| s.visit(i)).collect()
}

struct Solver<'a> {
    eqs: &'a [Equation],
    parsed: Vec<Result<Expr, String>>,
    by_name: HashMap<&'a str, usize>,
    known: &'a dyn Fn(&str) -> Option<f64>,
    out: Vec<Option<Result<f64, String>>>,
    /// Equations being evaluated, outermost first: finding one of them again is a cycle.
    stack: Vec<usize>,
}

impl Solver<'_> {
    fn visit(&mut self, i: usize) -> Result<f64, String> {
        if let Some(r) = &self.out[i] {
            return r.clone();
        }
        if let Some(at) = self.stack.iter().position(|&k| k == i) {
            let mut chain: Vec<&str> = self.stack[at..].iter().map(|&k| self.eqs[k].target()).collect();
            chain.push(self.eqs[i].target());
            let err = Err(format!("circular reference: {}", chain.join(" -> ")));
            for &k in &self.stack[at..] {
                self.out[k] = Some(err.clone());
            }
            return err;
        }
        self.stack.push(i);
        let r = match self.parsed[i].clone() {
            Err(e) => Err(e),
            Ok(e) => e.eval(&mut |name| match self.by_name.get(name).copied() {
                Some(j) => self.visit(j).map_err(|e| {
                    if e.starts_with("circular") {
                        e
                    } else {
                        format!("uses {name}, which has an error")
                    }
                }),
                None => (self.known)(name).ok_or_else(|| format!("unknown name {name}")),
            }),
        }
        .and_then(|v| {
            v.is_finite()
                .then_some(v)
                .ok_or_else(|| "the result is not a number".to_string())
        });
        self.stack.pop();
        // A cycle found below this equation already gave it its error.
        self.out[i].get_or_insert(r).clone()
    }
}

impl FeatureKind {
    /// The feature's own sizes, numbered for equations (`D1@Boss-Extrude1`): lengths in mm and
    /// angles in radians. Numbers are fixed per kind, so an unused one (no second direction) leaves
    /// a gap instead of renumbering the rest. Sketch dimensions live in the sketch.
    // shortcut: instance counts are integers and left out; drive them once someone needs it.
    pub fn dimensions_mut(&mut self) -> Vec<(u32, &'static str, Quantity, &mut f64)> {
        use crate::feature::*;
        use Quantity::{Angle, Length, Number};
        fn end(e: &mut End) -> Option<&mut f64> {
            match e {
                End::Blind(d) | End::MidPlane(d) | End::OffsetFromSurface(_, d) => Some(d),
                _ => None,
            }
        }
        let mut out = Vec::new();
        macro_rules! put {
            ($n:expr, $label:expr, $q:expr, $v:expr) => {
                if let Some(v) = $v {
                    out.push(($n, $label, $q, v))
                }
            };
        }
        match self {
            FeatureKind::Extrude(e) => {
                put!(1, "Depth", Length, end(&mut e.end));
                put!(2, "Depth (direction 2)", Length, e.end2.as_mut().and_then(end));
                put!(3, "Draft angle", Angle, Some(&mut e.draft));
                if let Some(t) = &mut e.thin {
                    put!(4, "Wall thickness", Length, Some(&mut t.t1));
                    put!(5, "Wall thickness 2", Length, Some(&mut t.t2));
                }
            }
            FeatureKind::Revolve(r) => {
                put!(1, "Angle", Angle, Some(&mut r.angle));
                put!(2, "Angle (direction 2)", Angle, Some(&mut r.angle2));
            }
            FeatureKind::Fillet { radius, .. } | FeatureKind::FaceFillet { radius, .. } => {
                put!(1, "Radius", Length, Some(radius))
            }
            FeatureKind::Chamfer { d1, d2, angle, .. } => {
                put!(1, "Distance", Length, Some(d1));
                put!(2, "Distance 2", Length, Some(d2));
                put!(3, "Angle", Angle, Some(angle));
            }
            FeatureKind::Shell { thickness, .. } | FeatureKind::Rib { thickness, .. } => {
                put!(1, "Thickness", Length, Some(thickness))
            }
            FeatureKind::Hole(h) => put!(1, "Depth", Length, end(&mut h.end)),
            FeatureKind::LinearPattern(p) => {
                put!(1, "Spacing", Length, Some(&mut p.dir1.spacing));
                put!(2, "Spacing 2", Length, p.dir2.as_mut().map(|d| &mut d.spacing));
            }
            FeatureKind::CircularPattern(p) => put!(1, "Angle", Angle, Some(&mut p.angle)),
            FeatureKind::RefPlane(RefPlaneDef::Offset { distance, .. }) => put!(1, "Offset", Length, Some(distance)),
            FeatureKind::RefPlane(RefPlaneDef::Angle { angle, .. }) | FeatureKind::Draft { angle, .. } => {
                put!(1, "Angle", Angle, Some(angle))
            }
            FeatureKind::RefPoint(RefPointDef::Coords(p)) => {
                let [x, y, z] = p;
                put!(1, "X", Length, Some(x));
                put!(2, "Y", Length, Some(y));
                put!(3, "Z", Length, Some(z));
            }
            FeatureKind::Sweep(w) => {
                put!(1, "Twist", Angle, Some(&mut w.twist));
                if let SweepProfile::Circle(d) = &mut w.profile {
                    put!(2, "Diameter", Length, Some(d));
                }
            }
            FeatureKind::Loft(l) => {
                put!(1, "Start tangency", Length, Some(&mut l.start_tangent));
                put!(2, "End tangency", Length, Some(&mut l.end_tangent));
            }
            FeatureKind::Helix(h) => {
                match &mut h.size {
                    HelixSize::PitchTurns { pitch, turns } | HelixSize::Spiral { pitch, turns } => {
                        put!(1, "Pitch", Length, Some(pitch));
                        put!(2, "Revolutions", Number, Some(turns));
                    }
                    HelixSize::HeightPitch { height, pitch } => {
                        put!(1, "Pitch", Length, Some(pitch));
                        put!(3, "Height", Length, Some(height));
                    }
                    HelixSize::HeightTurns { height, turns } => {
                        put!(2, "Revolutions", Number, Some(turns));
                        put!(3, "Height", Length, Some(height));
                    }
                }
                put!(4, "Taper angle", Angle, Some(&mut h.taper));
                put!(5, "Start angle", Angle, Some(&mut h.start));
            }
            FeatureKind::Thread(t) => {
                put!(1, "Length", Length, t.length.as_mut());
                put!(2, "Offset", Length, Some(&mut t.offset));
            }
            FeatureKind::MoveBody(m) => {
                let [x, y, z] = &mut m.translate;
                put!(1, "X", Length, Some(x));
                put!(2, "Y", Length, Some(y));
                put!(3, "Z", Length, Some(z));
                put!(4, "Angle", Angle, m.rotate.as_mut().map(|r| &mut r.1));
            }
            FeatureKind::ScaleBody { factors, .. } => {
                let [x, y, z] = factors;
                put!(1, "Scale X", Number, Some(x));
                put!(2, "Scale Y", Number, Some(y));
                put!(3, "Scale Z", Number, Some(z));
            }
            FeatureKind::CurvePattern { spacing, .. } => put!(1, "Spacing", Length, spacing.as_mut()),
            FeatureKind::Dome { height, .. } => put!(1, "Height", Length, Some(height)),
            FeatureKind::Wrap { depth, .. } => put!(1, "Depth", Length, Some(depth)),
            FeatureKind::Fit { clearance, .. } => put!(1, "Clearance", Length, Some(clearance)),
            _ => {}
        }
        out
    }
}

impl History {
    /// Calls `f` with every dimension of the part (sketch dimensions and feature sizes, in tree
    /// order), letting it change the value.
    fn each_dimension(&mut self, f: &mut dyn FnMut(&str, &str, Quantity, &mut f64)) {
        let ids: Vec<_> = self.iter().map(|(id, _)| id).collect();
        for id in ids {
            let Some(feature) = self.get_mut(id) else { continue };
            let owner = feature.name.clone();
            if let FeatureKind::Sketch { sketch, .. } = &mut feature.kind {
                for i in 0..sketch.constraints().len() {
                    let c = &sketch.constraints()[i];
                    let Some(v) = c.dimension() else { continue };
                    let q = if c.is_angular() {
                        Quantity::Angle
                    } else {
                        Quantity::Length
                    };
                    let (label, mut new) = (c.name(), v);
                    f(&format!("D{}@{owner}", sketch.number(i)), label, q, &mut new);
                    if new != v {
                        sketch.set_dimension(i, new);
                    }
                }
            } else {
                for (n, label, q, v) in feature.kind.dimensions_mut() {
                    f(&format!("D{n}@{owner}"), label, q, v);
                }
            }
        }
    }

    /// Every dimension of the part, named for equations.
    pub fn dimensions(&self) -> Vec<Dimension> {
        let mut out = Vec::new();
        self.clone().each_dimension(&mut |name, label, quantity, v| {
            out.push(Dimension {
                name: name.into(),
                label: label.into(),
                quantity,
                value: *v,
            })
        });
        out
    }

    /// Evaluates the active configuration's equations and writes the values into the dimensions
    /// they name. Returns each equation's value or error, in the order of `active_equations`.
    ///
    /// A dimension a configuration's equation sets gets its own value back once that equation no
    /// longer applies (another configuration is active), so configurations never leak into
    /// each other.
    pub fn apply_equations(&mut self, units: Units) -> Vec<Result<f64, String>> {
        // Most parts have no equations: every rebuild (a dimension drag) skips the walk.
        if self.held.is_empty()
            && self.equations().is_empty()
            && self.configurations().iter().all(|c| c.equations.is_empty())
        {
            return Vec::new();
        }
        let overrides: Vec<String> = self
            .configurations()
            .get(self.active_configuration())
            .map_or(Vec::new(), |c| {
                c.equations.iter().map(|e| e.target().to_string()).collect()
            });
        let restore: HashMap<String, f64> = self
            .held
            .iter()
            .filter(|(n, _)| !overrides.contains(n))
            .cloned()
            .collect();
        self.held.retain(|(n, _)| overrides.contains(n));
        let mut shown = HashMap::new();
        let mut stored = HashMap::new();
        self.each_dimension(&mut |name, _, q, v| {
            if let Some(&back) = restore.get(name) {
                *v = back;
            }
            shown.insert(name.to_string(), (q, q.shown(*v, units)));
            stored.insert(name.to_string(), *v);
        });
        for n in overrides {
            if let Some(&v) = stored.get(&n).filter(|_| !self.held.iter().any(|(h, _)| *h == n)) {
                self.held.push((n, v));
            }
        }
        let eqs = self.active_equations();
        if eqs.is_empty() {
            return Vec::new();
        }
        let mut results = solve(&eqs, units, &|n| shown.get(n).map(|s| s.1));
        let mut set = HashMap::new();
        for (e, r) in eqs.iter().zip(&mut results) {
            let name = e.target();
            match (&r, shown.contains_key(name)) {
                (Ok(v), true) => {
                    set.insert(name, *v);
                }
                (Ok(_), false) if name.contains('@') => *r = Err(format!("there is no dimension {name}")),
                _ => {}
            }
        }
        self.each_dimension(&mut |name, _, q, v| {
            if let Some(&x) = set.get(name) {
                *v = q.stored(x, units);
            }
        });
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UnitSystem;

    fn mm() -> Units {
        Units::default()
    }

    #[test]
    fn expressions_with_units_and_functions() {
        let ev = |t: &str| evaluate(t, mm());
        assert_eq!(ev("2 + 3 * 4"), Ok(14.0));
        assert_eq!(ev("(2 + 3) * 4"), Ok(20.0));
        assert_eq!(ev("-2^2"), Ok(-4.0));
        assert_eq!(ev("2^3^2"), Ok(512.0));
        assert_eq!(ev("1in + 2 mm"), Ok(27.4));
        assert!((ev("3\"").unwrap() - 76.2).abs() < 1e-12);
        assert_eq!(ev("1.5e1cm"), Ok(150.0));
        assert_eq!(ev("(1 + 1) in"), Ok(50.8));
        assert!((ev("1 rad").unwrap() - 57.29577951308232).abs() < 1e-12);
        assert!((ev("sin(30) + cos(60deg)").unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(ev("max(1, 4, 2) - min(3, abs(-1))"), Ok(3.0));
        assert!((ev("pi").unwrap() - std::f64::consts::PI).abs() < 1e-15);
        // In an inch part a bare number is inches and millimetres convert.
        let ips = Units {
            system: UnitSystem::Ips,
            decimals: 3,
        };
        assert_eq!(evaluate("1 + 25.4mm", ips), Ok(2.0));
        for bad in ["", "2 +", "(1 + 2", "1 / 0", "2 parsec", "foo(1)", "wall * 2", "3 3"] {
            assert!(ev(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn equations_solve_in_dependency_order() {
        let eqs = [
            Equation::new("\"D2@Sketch1\"", "\"D1@Sketch1\" * 2"),
            Equation::new("D1@Sketch1", "wall * 5 + 1 mm"),
            Equation::new("wall", "2"),
        ];
        let r = solve(&eqs, mm(), &|n| {
            (n == "D1@Sketch1" || n == "D2@Sketch1").then_some(99.0)
        });
        assert_eq!(r, [Ok(22.0), Ok(11.0), Ok(2.0)]);
        // A name nothing sets is the dimension's current value.
        let r = solve(&[Equation::new("x", "D9@Boss-Extrude1 / 2")], mm(), &|_| Some(30.0));
        assert_eq!(r, [Ok(15.0)]);
    }

    #[test]
    fn cycles_and_errors_are_reported_per_equation() {
        let eqs = [
            Equation::new("a", "b + 1"),
            Equation::new("b", "c * 2"),
            Equation::new("c", "a"),
            Equation::new("d", "a + 1"),
            Equation::new("e", "nothing"),
            Equation::new("a", "1"),
            Equation::new("f", "2 * "),
            Equation::new("g", "5"),
        ];
        let r = solve(&eqs, mm(), &|_| None);
        for (i, e) in r.iter().take(3).enumerate() {
            let e = e.clone().unwrap_err();
            assert!(e.starts_with("circular reference"), "{i}: {e}");
        }
        assert!(r[0].clone().unwrap_err().contains("a -> b -> c -> a"), "{:?}", r[0]);
        assert_eq!(r[3], Err("circular reference: a -> b -> c -> a".into()));
        assert_eq!(r[4], Err("unknown name nothing".into()));
        assert!(r[5].clone().unwrap_err().contains("already set"));
        assert!(r[6].is_err());
        assert_eq!(r[7], Ok(5.0), "the rest still solve");
        // Self reference.
        let r = solve(&[Equation::new("x", "x + 1")], mm(), &|_| Some(1.0));
        assert_eq!(r, [Err("circular reference: x -> x".into())]);
    }
}
