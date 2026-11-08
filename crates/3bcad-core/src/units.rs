//! Document units. Geometry is always stored in millimetres; units only change what is shown and
//! how typed values are read.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnitSystem {
    /// Millimetre, gram, second.
    #[default]
    Mmgs,
    /// Centimetre, gram, second.
    Cgs,
    /// Inch, pound, second.
    Ips,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Units {
    pub system: UnitSystem,
    /// Decimal places shown for lengths.
    pub decimals: usize,
}

impl Default for Units {
    fn default() -> Self {
        Self {
            system: UnitSystem::Mmgs,
            decimals: 2,
        }
    }
}

/// Length units that can be typed after a number, with their size in mm.
pub(crate) const LENGTHS: [(&str, f64); 8] = [
    ("mm", 1.0),
    ("cm", 10.0),
    ("m", 1000.0),
    ("in", 25.4),
    ("\"", 25.4),
    ("ft", 304.8),
    ("'", 304.8),
    ("um", 0.001),
];

impl UnitSystem {
    pub const ALL: [UnitSystem; 3] = [UnitSystem::Mmgs, UnitSystem::Cgs, UnitSystem::Ips];

    pub fn name(self) -> &'static str {
        match self {
            UnitSystem::Mmgs => "MMGS (millimetre, gram, second)",
            UnitSystem::Cgs => "CGS (centimetre, gram, second)",
            UnitSystem::Ips => "IPS (inch, pound, second)",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            UnitSystem::Mmgs => "MMGS",
            UnitSystem::Cgs => "CGS",
            UnitSystem::Ips => "IPS",
        }
    }

    pub fn length_unit(self) -> &'static str {
        match self {
            UnitSystem::Mmgs => "mm",
            UnitSystem::Cgs => "cm",
            UnitSystem::Ips => "in",
        }
    }

    /// Millimetres in one length unit.
    pub fn mm_per_unit(self) -> f64 {
        match self {
            UnitSystem::Mmgs => 1.0,
            UnitSystem::Cgs => 10.0,
            UnitSystem::Ips => 25.4,
        }
    }

    pub fn mass_unit(self) -> &'static str {
        match self {
            UnitSystem::Ips => "lb",
            _ => "g",
        }
    }

    pub fn grams_per_unit(self) -> f64 {
        match self {
            UnitSystem::Ips => 453.592_37,
            _ => 1.0,
        }
    }
}

impl Units {
    /// A length in mm, in document units with the unit name: "25.40 mm", "1.00 in".
    pub fn length(&self, mm: f64) -> String {
        format!(
            "{:.*} {}",
            self.decimals,
            mm / self.system.mm_per_unit(),
            self.system.length_unit()
        )
    }

    pub fn area(&self, mm2: f64) -> String {
        format!(
            "{:.*} {}²",
            self.decimals,
            mm2 / self.system.mm_per_unit().powi(2),
            self.system.length_unit()
        )
    }

    pub fn volume(&self, mm3: f64) -> String {
        format!(
            "{:.*} {}³",
            self.decimals,
            mm3 / self.system.mm_per_unit().powi(3),
            self.system.length_unit()
        )
    }

    pub fn mass(&self, grams: f64) -> String {
        format!(
            "{:.*} {}",
            self.decimals,
            grams / self.system.grams_per_unit(),
            self.system.mass_unit()
        )
    }

    /// Reads a typed length into mm: a plain number is in document units, a number with a unit
    /// (`1in`, `2.5 cm`, `3"`) is converted, and arithmetic works (`10 + 2in`, `(4 - 1) * 2`).
    pub fn parse_length(&self, text: &str) -> Option<f64> {
        crate::equations::evaluate(text, *self)
            .ok()
            .map(|v| v * self.system.mm_per_unit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_units_convert() {
        let mm = Units::default();
        let ips = Units {
            system: UnitSystem::Ips,
            decimals: 3,
        };
        assert_eq!(mm.parse_length("10"), Some(10.0));
        assert_eq!(mm.parse_length("1in"), Some(25.4));
        assert_eq!(mm.parse_length(" 2.5 cm "), Some(25.0));
        assert!((mm.parse_length("3\"").unwrap() - 76.2).abs() < 1e-9);
        assert_eq!(ips.parse_length("2"), Some(50.8));
        assert!((ips.parse_length("12.7mm").unwrap() - 12.7).abs() < 1e-12);
        assert_eq!(mm.parse_length("1 parsec"), None);
        assert_eq!(mm.parse_length("abc"), None);
        assert_eq!(mm.parse_length("10 + 2 cm"), Some(30.0));
        assert_eq!(ips.length(25.4), "1.000 in");
        assert_eq!(mm.length(25.4), "25.40 mm");
        // A 1 in cube: 16387.064 mm³ is 1 in³.
        assert_eq!(ips.volume(16_387.064), "1.000 in³");
        assert_eq!(ips.mass(453.592_37), "1.000 lb");
    }
}
