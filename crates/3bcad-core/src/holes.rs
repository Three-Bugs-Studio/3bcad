//! Hole Wizard size tables (mm). ISO: clearance ISO 273 medium, tap drills ISO 2306, counterbores
//! for ISO 4762 socket head cap screws, 90° countersinks for ISO 10642. ANSI (inch, converted):
//! UNC tap drills, normal-fit clearance, counterbores for socket head cap screws, 82° countersinks.

use crate::feature::{HoleKind, HoleStandard};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoleSize {
    pub name: &'static str,
    /// Thread major diameter.
    pub major: f64,
    pub pitch: f64,
    pub clearance: f64,
    pub tap_drill: f64,
    pub cbore_diameter: f64,
    pub cbore_depth: f64,
    pub csk_diameter: f64,
    /// Included angle of the countersink, degrees.
    pub csk_angle: f64,
}

// One row of the ISO table, in the order of the columns.
#[allow(clippy::too_many_arguments)]
const fn iso(name: &'static str, major: f64, pitch: f64, clr: f64, tap: f64, cb: f64, cbd: f64, csk: f64) -> HoleSize {
    HoleSize {
        name,
        major,
        pitch,
        clearance: clr,
        tap_drill: tap,
        cbore_diameter: cb,
        cbore_depth: cbd,
        csk_diameter: csk,
        csk_angle: 90.0,
    }
}

const IN: f64 = 25.4;

const fn ansi(name: &'static str, major: f64, tpi: f64, clr: f64, tap: f64, cb: f64, csk: f64) -> HoleSize {
    HoleSize {
        name,
        major: major * IN,
        pitch: IN / tpi,
        clearance: clr * IN,
        tap_drill: tap * IN,
        cbore_diameter: cb * IN,
        cbore_depth: major * IN,
        csk_diameter: csk * IN,
        csk_angle: 82.0,
    }
}

pub const ISO: [HoleSize; 12] = [
    iso("M2", 2.0, 0.4, 2.4, 1.6, 4.4, 2.0, 4.4),
    iso("M2.5", 2.5, 0.45, 2.9, 2.05, 5.4, 2.5, 5.5),
    iso("M3", 3.0, 0.5, 3.4, 2.5, 6.5, 3.3, 6.3),
    iso("M4", 4.0, 0.7, 4.5, 3.3, 8.0, 4.4, 9.4),
    iso("M5", 5.0, 0.8, 5.5, 4.2, 10.0, 5.4, 10.4),
    iso("M6", 6.0, 1.0, 6.6, 5.0, 11.0, 6.4, 12.6),
    iso("M8", 8.0, 1.25, 9.0, 6.8, 15.0, 8.6, 17.3),
    iso("M10", 10.0, 1.5, 11.0, 8.5, 18.0, 10.6, 20.0),
    iso("M12", 12.0, 1.75, 13.5, 10.2, 20.0, 12.6, 24.0),
    iso("M14", 14.0, 2.0, 15.5, 12.0, 24.0, 14.6, 27.0),
    iso("M16", 16.0, 2.0, 17.5, 14.0, 26.0, 16.6, 31.0),
    iso("M20", 20.0, 2.5, 22.0, 17.5, 33.0, 20.6, 38.0),
];

pub const ANSI: [HoleSize; 10] = [
    ansi("#2", 0.086, 56.0, 0.0960, 0.0700, 0.1875, 0.172),
    ansi("#4", 0.112, 40.0, 0.1285, 0.0890, 0.2188, 0.225),
    ansi("#6", 0.138, 32.0, 0.1495, 0.1065, 0.2812, 0.279),
    ansi("#8", 0.164, 32.0, 0.1770, 0.1360, 0.3125, 0.332),
    ansi("#10", 0.190, 24.0, 0.2010, 0.1495, 0.3750, 0.385),
    ansi("1/4", 0.250, 20.0, 0.2660, 0.2010, 0.4375, 0.507),
    ansi("5/16", 0.3125, 18.0, 0.3320, 0.2570, 0.5312, 0.635),
    ansi("3/8", 0.375, 16.0, 0.3970, 0.3125, 0.6250, 0.762),
    ansi("7/16", 0.4375, 14.0, 0.4687, 0.3680, 0.7188, 0.812),
    ansi("1/2", 0.500, 13.0, 0.5312, 0.4219, 0.8125, 0.875),
];

pub fn table(standard: HoleStandard) -> &'static [HoleSize] {
    match standard {
        HoleStandard::Iso => &ISO,
        HoleStandard::Ansi => &ANSI,
    }
}

pub fn find(standard: HoleStandard, name: &str) -> Option<&'static HoleSize> {
    table(standard).iter().find(|s| s.name == name)
}

/// Cosmetic thread data a tapped hole carries for drawings.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadSpec {
    pub designation: String,
    pub major: f64,
    pub pitch: f64,
}

impl HoleSize {
    /// Drilled diameter for a hole of this kind.
    pub fn drill(&self, kind: HoleKind) -> f64 {
        match kind {
            HoleKind::Tapped => self.tap_drill,
            _ => self.clearance,
        }
    }

    pub fn thread(&self, standard: HoleStandard) -> ThreadSpec {
        let designation = match standard {
            HoleStandard::Iso => format!("{}x{}", self.name, self.pitch),
            HoleStandard::Ansi => format!("{}-{} UNC", self.name, (IN / self.pitch).round()),
        };
        ThreadSpec {
            designation,
            major: self.major,
            pitch: self.pitch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_ordered_and_consistent() {
        for t in [&ISO[..], &ANSI[..]] {
            for s in t {
                assert!(s.tap_drill < s.major && s.major < s.clearance, "{}", s.name);
                assert!(
                    s.clearance < s.cbore_diameter && s.clearance < s.csk_diameter,
                    "{}",
                    s.name
                );
            }
            assert!(t.windows(2).all(|w| w[0].major < w[1].major));
        }
        let m6 = find(HoleStandard::Iso, "M6").unwrap();
        assert_eq!(m6.thread(HoleStandard::Iso).designation, "M6x1");
        let q = find(HoleStandard::Ansi, "1/4").unwrap();
        assert_eq!(q.thread(HoleStandard::Ansi).designation, "1/4-20 UNC");
        assert!((q.major - 6.35).abs() < 1e-9);
    }
}
